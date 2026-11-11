//! The local server: one WebSocket for control and preview, plus the UI.
//!
//! Messages from the server:
//!
//! - text `hello`: app version, the effect list and palettes, sent once.
//! - text `state`: the whole show, sent on connect and after every change.
//! - text `audio`: the current audio features, 30 times a second.
//! - text `status`: frame rate, audio status, discovered devices, once a second.
//! - text `live`: held keys, a waiting scene and the tapped tempo, on change.
//! - text `library`: the shows on this computer and which is open, on change.
//! - text `result`: the outcome of a command that carried an `id`.
//! - binary: byte 0 says what it is (1 preview colours, 2 pixel positions),
//!   bytes 4..8 are the layout serial, the payload starts at byte 8.
//!
//! Messages from the client are commands (see `commands.rs`), as JSON text.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::header::{self, HeaderMap, HeaderValue};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use darkrgb_core::audio::{BAND_MAX_HZ, BASS, MID, TREBLE, band_edge_hz, group_bands};
use darkrgb_core::looks::LOOKS;
use darkrgb_core::show::FRAME_RATES;
use darkrgb_core::{BANDS, Kind, color, effects};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::app::{APP_NAME, BUILD, Discovered, Shared, VERSION};
use crate::commands::Command;

#[derive(rust_embed::RustEmbed)]
#[folder = "$OUT_DIR/ui"]
struct Assets;

/// Who may connect.
#[derive(Clone, Debug, Default)]
pub struct Access {
    /// Accept connections addressed to any host name, for use from other
    /// devices on the network. Off by default.
    pub lan: bool,
    /// One extra origin allowed to connect, for the UI's development server.
    pub dev_origin: Option<String>,
}

#[derive(Clone)]
struct AppState {
    shared: Arc<Shared>,
    access: Access,
}

/// Largest show file that can be imported. Far more than any real show.
const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;

pub fn router(shared: Arc<Shared>, access: Access) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route(
            "/api/import",
            post(import_handler).layer(DefaultBodyLimit::max(MAX_IMPORT_BYTES)),
        )
        .fallback(static_handler)
        .with_state(AppState { shared, access })
}

/// Takes a show file the user picked in the UI. A whole show does not
/// fit in a WebSocket message, so it comes as the body of a POST. The
/// same checks on who is asking apply; a page on another site cannot
/// send this without the browser first asking, and being refused.
async fn import_handler(State(state): State<AppState>, headers: HeaderMap, body: String) -> Response {
    if !allowed(&headers, &state.access) || headers.get(header::ORIGIN).is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let outcome = state.shared.import_show(&body);
    let (status, body) = match outcome {
        Ok(note) => (StatusCode::OK, json!({ "ok": true, "note": note })),
        Err(error) => (StatusCode::BAD_REQUEST, json!({ "ok": false, "error": error })),
    };
    (status, [(header::CONTENT_TYPE, "application/json")], body.to_string()).into_response()
}

/// The host part of a `Host` header or origin, without port or brackets.
fn host_name(host_port: &str) -> &str {
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host_port.rsplit_once(':').map_or(host_port, |(h, port)| {
            if port.chars().all(|c| c.is_ascii_digit()) {
                h
            } else {
                host_port
            }
        })
    };
    host.trim_end_matches('.')
}

fn is_loopback_name(host: &str) -> bool {
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1")
}

/// Decides whether a request may use the server.
///
/// A web page on any site can try to open a WebSocket to this machine, and
/// the browser will let it. Two checks stop that:
///
/// - `Host` must name this machine. That defeats DNS rebinding, where a
///   hostile site points its own name at 127.0.0.1.
/// - `Origin`, which browsers always send for WebSockets, must be the page
///   this server itself served. Programs that are not browsers send none.
fn allowed(headers: &HeaderMap, access: &Access) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    if !access.lan && !is_loopback_name(host_name(host)) {
        return false;
    }
    match headers.get(header::ORIGIN).map(|o| o.to_str()) {
        None => true,
        Some(Err(_)) => false,
        Some(Ok(origin)) => {
            let same_origin = origin
                .strip_prefix("http://")
                .is_some_and(|rest| rest.eq_ignore_ascii_case(host));
            same_origin || access.dev_origin.as_deref() == Some(origin)
        }
    }
}

async fn ws_handler(State(state): State<AppState>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    if !allowed(&headers, &state.access) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.max_message_size(64 * 1024)
        .on_upgrade(move |socket| client(socket, state.shared))
}

async fn static_handler(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Response {
    if !allowed(&headers, &state.access) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let path = uri.path().trim_start_matches('/');
    let (path, file) = match Assets::get(path) {
        Some(file) if !path.is_empty() => (path, file),
        // Anything else gets the page itself.
        _ => match Assets::get("index.html") {
            Some(file) => ("index.html", file),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // Built assets have a hash in their name and never change; the page
    // must always be fetched fresh so an update is picked up.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut response = file.data.into_owned().into_response();
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; connect-src 'self' ws:; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; frame-ancestors 'none'",
        ),
    );
    response
}

fn hello() -> String {
    let effects: Vec<_> = effects::registry().iter().map(|d| &d.info).collect();
    json!({
        "type": "hello",
        "app": APP_NAME,
        "version": VERSION,
        "build": BUILD,
        "frame_rates": FRAME_RATES,
        // First band and one past the last band of bass, mids and treble.
        "band_groups": ([BASS, MID, TREBLE].map(group_bands)),
        // Edges of the frequency bands in Hz, so the UI can label ranges.
        "band_hz": (0..=BANDS).map(|i| band_edge_hz(i, BAND_MAX_HZ).round()).collect::<Vec<f32>>(),
        "effects": effects,
        "palettes": color::palettes(),
        // The starter scenes there are, and which of them each kind of
        // show gets, so the UI can tell which are missing.
        "looks": LOOKS.iter().map(|l| json!({ "name": l.name, "description": l.description })).collect::<Vec<_>>(),
        "kinds": Kind::ALL.iter().map(|k| json!({
            "id": k,
            "name": k.name(),
            "description": k.description(),
            "looks": k.looks(),
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

fn state_message(shared: &Shared, revision: u64) -> String {
    let show = shared.show.lock().unwrap();
    json!({
        "type": "state",
        "revision": revision,
        "show": &*show,
        // The scene that is playing exactly as it was saved, if any.
        "current_scene": show.current_scene(),
    })
    .to_string()
}

fn live_message(shared: &Shared) -> String {
    let mut value = serde_json::to_value(shared.live.lock().unwrap().status()).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut value {
        map.insert("type".into(), "live".into());
    }
    value.to_string()
}

fn library_message(shared: &Shared) -> String {
    let mut value = shared.library_status();
    if let Value::Object(map) = &mut value {
        map.insert("type".into(), "library".into());
    }
    value.to_string()
}

fn audio_message(shared: &Shared) -> String {
    let mut value = serde_json::to_value(shared.features()).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut value {
        map.insert("type".into(), "audio".into());
    }
    value.to_string()
}

/// Whether a device seen on the network is this one from the show.
/// Hardware addresses decide when both are known; several devices can
/// share one IP address (simulators do), so the IP is only a fallback.
fn already_added(found: &Discovered, device: &darkrgb_core::Device) -> bool {
    match (&found.mac, &device.mac) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => found.address.split(':').next() == Some(device.address.as_str()),
    }
}

fn status_message(shared: &Shared) -> String {
    let discovered: Vec<Value> = {
        let show = shared.show.lock().unwrap();
        shared
            .discovered
            .lock()
            .unwrap()
            .iter()
            .map(|d| {
                let added = show.devices.iter().any(|dev| already_added(d, dev));
                let mut v = serde_json::to_value(d).unwrap_or(Value::Null);
                v["added"] = added.into();
                v
            })
            .collect()
    };
    json!({
        "type": "status",
        "render": &*shared.stats.lock().unwrap(),
        "audio": shared.audio.status(),
        "inputs": &*shared.inputs.lock().unwrap(),
        "networks": &*shared.networks.lock().unwrap(),
        "scan": &*shared.scan.lock().unwrap(),
        "discovered": discovered,
        "notice": &*shared.notice.lock().unwrap(),
        "show_file": shared.save_path.lock().unwrap().as_ref().map(|p| p.display().to_string()),
    })
    .to_string()
}

fn result_message(id: Option<&Value>, outcome: Result<Option<String>, String>) -> Option<String> {
    match (id, outcome) {
        // A command sent without an id only hears back if it failed.
        (None, Ok(None)) => None,
        (id, Ok(note)) => Some(json!({ "type": "result", "id": id, "ok": true, "note": note }).to_string()),
        (id, Err(error)) => Some(json!({ "type": "result", "id": id, "ok": false, "error": error }).to_string()),
    }
}

async fn client(socket: WebSocket, shared: Arc<Shared>) {
    let (mut tx, mut rx) = socket.split();
    if tx.send(Message::Text(hello().into())).await.is_err() {
        return;
    }

    let mut revision = shared.revision.subscribe();
    revision.mark_changed();
    let mut live = shared.live_rev.subscribe();
    live.mark_changed();
    let mut library = shared.library_rev.subscribe();
    library.mark_changed();
    // Keys this client is holding down, to let go of if it disappears.
    let mut holding: HashSet<u32> = HashSet::new();
    let mut layout = shared.layout.subscribe();
    layout.mark_changed();
    let mut preview = shared.preview.subscribe();

    let mut audio_tick = tokio::time::interval(Duration::from_millis(33));
    audio_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut status_tick = tokio::time::interval(Duration::from_secs(1));
    status_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // Results of commands that ran on another thread.
    let (result_tx, mut result_rx) = mpsc::channel::<String>(16);

    loop {
        let outgoing = tokio::select! {
            changed = revision.changed() => {
                if changed.is_err() { break; }
                let rev = *revision.borrow_and_update();
                Some(Message::Text(state_message(&shared, rev).into()))
            }
            changed = live.changed() => {
                if changed.is_err() { break; }
                live.mark_unchanged();
                Some(Message::Text(live_message(&shared).into()))
            }
            changed = library.changed() => {
                if changed.is_err() { break; }
                library.mark_unchanged();
                // Reading the folder touches the disk; keep it off this task.
                let shared = shared.clone();
                match tokio::task::spawn_blocking(move || library_message(&shared)).await {
                    Ok(text) => Some(Message::Text(text.into())),
                    Err(_) => None,
                }
            }
            changed = layout.changed() => {
                if changed.is_err() { break; }
                let bytes = layout.borrow_and_update().clone();
                (!bytes.is_empty()).then(|| Message::Binary(bytes))
            }
            changed = preview.changed() => {
                if changed.is_err() { break; }
                let bytes = preview.borrow_and_update().clone();
                (!bytes.is_empty()).then(|| Message::Binary(bytes))
            }
            _ = audio_tick.tick() => Some(Message::Text(audio_message(&shared).into())),
            _ = status_tick.tick() => Some(Message::Text(status_message(&shared).into())),
            Some(text) = result_rx.recv() => Some(Message::Text(text.into())),
            incoming = rx.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    handle(&shared, text.as_str(), &result_tx, &mut holding).map(|t| Message::Text(t.into()))
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => None,
            },
        };
        if let Some(message) = outgoing
            && tx.send(message).await.is_err()
        {
            break;
        }
    }
    // A key must not stay down because the window that held it closed.
    for binding in holding {
        shared.release(binding);
    }
}

/// Runs one message from a client. Returns a reply to send straight away,
/// if there is one.
fn handle(
    shared: &Arc<Shared>,
    text: &str,
    results: &mpsc::Sender<String>,
    holding: &mut HashSet<u32>,
) -> Option<String> {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return result_message(None, Err("that message was not valid JSON".into())),
    };
    let id = value.get("id").cloned();
    if value.get("type").and_then(Value::as_str) == Some("dismiss_notice") {
        *shared.notice.lock().unwrap() = None;
        return result_message(id.as_ref(), Ok(None));
    }
    let command: Command = match serde_json::from_value(value) {
        Ok(command) => command,
        Err(e) => return result_message(id.as_ref(), Err(format!("unknown or malformed command: {e}"))),
    };
    if let Command::Trigger { binding, down } = command {
        if down {
            holding.insert(binding);
        } else {
            holding.remove(&binding);
        }
    }
    if matches!(command, Command::AddDevice { .. }) {
        // Talks to the device over the network; keep it off this task so
        // the preview does not stall while it waits.
        let (shared, results) = (shared.clone(), results.clone());
        tokio::task::spawn_blocking(move || {
            if let Some(reply) = result_message(id.as_ref(), shared.execute(&command)) {
                let _ = results.blocking_send(reply);
            }
        });
        None
    } else {
        result_message(id.as_ref(), shared.execute(&command))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(host: Option<&str>, origin: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(host) = host {
            h.insert(header::HOST, host.parse().unwrap());
        }
        if let Some(origin) = origin {
            h.insert(header::ORIGIN, origin.parse().unwrap());
        }
        h
    }

    #[test]
    fn host_names_are_extracted() {
        assert_eq!(host_name("127.0.0.1:7420"), "127.0.0.1");
        assert_eq!(host_name("localhost"), "localhost");
        assert_eq!(host_name("LocalHost.:80"), "LocalHost");
        assert_eq!(host_name("[::1]:7420"), "::1");
        assert_eq!(host_name("evil.example:7420"), "evil.example");
    }

    #[test]
    fn the_served_page_and_plain_programs_may_connect() {
        let local = Access::default();
        assert!(allowed(
            &headers(Some("127.0.0.1:7420"), Some("http://127.0.0.1:7420")),
            &local
        ));
        assert!(allowed(
            &headers(Some("localhost:7420"), Some("http://localhost:7420")),
            &local
        ));
        assert!(allowed(&headers(Some("[::1]:7420"), Some("http://[::1]:7420")), &local));
        assert!(allowed(&headers(Some("127.0.0.1:7420"), None), &local));
    }

    #[test]
    fn other_web_pages_may_not_connect() {
        let local = Access::default();
        // A page on another site opening a socket to this machine.
        assert!(!allowed(
            &headers(Some("127.0.0.1:7420"), Some("https://evil.example")),
            &local
        ));
        assert!(!allowed(
            &headers(Some("127.0.0.1:7420"), Some("http://localhost:7420")),
            &local
        ));
        assert!(!allowed(
            &headers(Some("127.0.0.1:7420"), Some("http://127.0.0.1:9999")),
            &local
        ));
        assert!(!allowed(&headers(Some("127.0.0.1:7420"), Some("null")), &local));
        // DNS rebinding: the hostile name resolves here, so the origin matches the host.
        assert!(!allowed(
            &headers(Some("evil.example:7420"), Some("http://evil.example:7420")),
            &local
        ));
        assert!(!allowed(&headers(None, None), &local));
    }

    #[test]
    fn lan_mode_accepts_other_host_names_but_still_checks_origin() {
        let lan = Access {
            lan: true,
            dev_origin: None,
        };
        assert!(allowed(
            &headers(Some("192.168.1.20:7420"), Some("http://192.168.1.20:7420")),
            &lan
        ));
        assert!(!allowed(
            &headers(Some("192.168.1.20:7420"), Some("https://evil.example")),
            &lan
        ));
    }

    #[test]
    fn the_development_origin_is_allowed_only_when_configured() {
        let dev = Access {
            lan: false,
            dev_origin: Some("http://localhost:5173".into()),
        };
        let h = headers(Some("127.0.0.1:7420"), Some("http://localhost:5173"));
        assert!(allowed(&h, &dev));
        assert!(!allowed(&h, &Access::default()));
    }

    #[test]
    fn found_devices_are_matched_by_hardware_address_first() {
        let found = |mac: Option<&str>, address: &str| Discovered {
            key: "k".into(),
            name: "n".into(),
            address: address.into(),
            leds: 1,
            matrix: None,
            version: "v".into(),
            mac: mac.map(str::to_string),
        };
        let device = |mac: Option<&str>, address: &str| darkrgb_core::Device {
            mac: mac.map(str::to_string),
            ..darkrgb_core::Device::wled("d", address, 4048, 1)
        };
        // Two simulators on one IP address are different devices.
        assert!(!already_added(
            &found(Some("aa"), "127.0.0.1:8082"),
            &device(Some("bb"), "127.0.0.1")
        ));
        assert!(
            already_added(&found(Some("AA"), "10.0.0.9"), &device(Some("aa"), "10.0.0.5")),
            "same device, new IP"
        );
        assert!(already_added(&found(None, "10.0.0.5"), &device(Some("aa"), "10.0.0.5")));
        assert!(!already_added(&found(None, "10.0.0.6"), &device(None, "10.0.0.5")));
    }

    #[test]
    fn results_are_only_sent_when_useful() {
        assert_eq!(result_message(None, Ok(None)), None);
        let id = json!(4);
        let ok: Value = serde_json::from_str(&result_message(Some(&id), Ok(None)).unwrap()).unwrap();
        assert_eq!(ok, json!({"type":"result","id":4,"ok":true,"note":null}));
        let err: Value = serde_json::from_str(&result_message(None, Err("no".into())).unwrap()).unwrap();
        assert_eq!(err, json!({"type":"result","id":null,"ok":false,"error":"no"}));
    }

    #[test]
    fn hello_lists_effects_with_their_settings() {
        let hello: Value = serde_json::from_str(&hello()).unwrap();
        assert_eq!(hello["app"], APP_NAME);
        let effects = hello["effects"].as_array().unwrap();
        assert!(effects.len() >= 10);
        let plasma = effects.iter().find(|e| e["id"] == "plasma").unwrap();
        let speed = plasma["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "speed")
            .unwrap();
        assert_eq!(speed["kind"], "float");
        assert_eq!(speed["min"], 0.0);
        assert!(
            hello["palettes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["id"] == "rainbow")
        );
    }
}
