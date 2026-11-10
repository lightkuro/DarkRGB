//! End-to-end tests: the real frame loop and server against a simulated
//! WLED device, over real sockets on this machine.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use ols_core::Show;
use ols_core::params::ParamValue;
use ols_sim::{Sim, SimConfig};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::app::Shared;
use crate::commands::Command;
use crate::render_loop::{self, MSG_LAYOUT, MSG_PREVIEW};
use crate::server::{self, Access};

struct Engine {
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    render: Option<std::thread::JoinHandle<()>>,
}

impl Engine {
    fn start() -> Self {
        let shared = Shared::new(Show::default(), None, None);
        let stop = Arc::new(AtomicBool::new(false));
        let render = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::spawn(move || render_loop::run(shared, stop))
        };
        Self {
            shared,
            stop,
            render: Some(render),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(render) = self.render.take() {
            let _ = render.join();
        }
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Replaces the default layer with a solid colour, which is easy to check.
fn make_solid(shared: &Shared, colour: &str) {
    let layer = shared.show.lock().unwrap().layers[0].id;
    shared
        .execute(&Command::UpdateLayer {
            id: layer,
            effect: Some("solid".into()),
            opacity: None,
            blend: None,
            enabled: None,
            targets: None,
        })
        .unwrap();
    shared
        .execute(&Command::SetParam {
            layer,
            param: "colour".into(),
            value: ParamValue::Text(colour.into()),
        })
        .unwrap();
}

#[test]
fn a_device_added_by_address_receives_the_rendered_frames() {
    let sim = Sim::start(SimConfig {
        name: "Bench strip".into(),
        leds: 700,
        ..SimConfig::default()
    })
    .unwrap();
    let engine = Engine::start();
    make_solid(&engine.shared, "#ff8000");

    let note = engine
        .shared
        .execute(&Command::AddDevice {
            address: format!("127.0.0.1:{}", sim.http_port),
        })
        .unwrap();
    assert_eq!(note, None);
    {
        let show = engine.shared.show.lock().unwrap();
        assert_eq!(show.devices[0].name, "Bench strip");
        assert_eq!(show.devices[0].pixel_count, 700);
        assert_eq!(show.devices[0].port, sim.ddp_port);
        assert_eq!(show.fixtures.len(), 1);
    }

    wait_for("frames at the device", || sim.stats().frames > 30);
    // #ff8000 through the device's default gamma of 2.2:
    // 255 stays 255, 128 becomes (128/255)^2.2 * 255 = 56, 0 stays 0.
    let frame = sim.frame();
    assert_eq!(frame.len(), 2100);
    assert!(
        frame.chunks(3).all(|px| px == [255, 56, 0]),
        "first pixel {:?}",
        &frame[..3]
    );
    assert_eq!(sim.stats().rejected, 0);

    // Adding the same device again is refused.
    let again = engine.shared.execute(&Command::AddDevice {
        address: format!("127.0.0.1:{}", sim.http_port),
    });
    assert!(again.unwrap_err().contains("already in the show"));
}

#[test]
fn frames_arrive_at_a_steady_sixty_per_second() {
    let sim = Sim::start(SimConfig {
        leds: 300,
        ..SimConfig::default()
    })
    .unwrap();
    let engine = Engine::start();
    engine
        .shared
        .execute(&Command::AddDevice {
            address: format!("127.0.0.1:{}", sim.http_port),
        })
        .unwrap();
    wait_for("the stream to start", || sim.stats().frames > 5);
    sim.reset_stats();
    std::thread::sleep(Duration::from_secs(2));
    let stats = sim.stats();
    // The average must be right. The worst single gap only has to rule out
    // a stall: test machines are shared, and one late frame is not a bug.
    assert!(stats.fps > 55.0 && stats.fps < 65.0, "{stats:?}");
    assert!(
        stats.max_ms < 250.0,
        "a frame arrived {} ms after the previous one",
        stats.max_ms
    );
}

#[test]
fn blackout_and_disabling_a_device_stop_the_light() {
    let sim = Sim::start(SimConfig {
        leds: 10,
        ..SimConfig::default()
    })
    .unwrap();
    let engine = Engine::start();
    make_solid(&engine.shared, "#ffffff");
    engine
        .shared
        .execute(&Command::AddDevice {
            address: format!("127.0.0.1:{}", sim.http_port),
        })
        .unwrap();
    wait_for("white", || sim.frame().iter().all(|b| *b == 255));

    engine
        .shared
        .execute(&Command::SetMaster {
            brightness: None,
            blackout: Some(true),
        })
        .unwrap();
    wait_for("black", || sim.frame().iter().all(|b| *b == 0));

    engine
        .shared
        .execute(&Command::SetMaster {
            brightness: Some(0.5),
            blackout: Some(false),
        })
        .unwrap();
    // Half brightness through gamma 2.2 is (128/255)^2.2 * 255 = 56.
    wait_for("half brightness", || sim.frame().iter().all(|b| *b == 56));

    let device = engine.shared.show.lock().unwrap().devices[0].id;
    engine
        .shared
        .execute(&Command::UpdateDevice {
            id: device,
            name: None,
            enabled: Some(false),
            gamma: None,
            brightness: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let before = sim.stats().frames;
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(sim.stats().frames, before, "a disabled device must receive nothing");
}

#[test]
fn an_unreachable_address_is_reported_not_added() {
    let engine = Engine::start();
    // A port nothing listens on.
    let result = engine.shared.execute(&Command::AddDevice {
        address: "127.0.0.1:9".into(),
    });
    assert!(result.unwrap_err().contains("no answer"));
    assert!(engine.shared.show.lock().unwrap().devices.is_empty());
    assert!(
        engine
            .shared
            .execute(&Command::AddDevice {
                address: "not an address".into()
            })
            .is_err()
    );
}

async fn next_json(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    kind: &str,
) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let message = tokio::time::timeout_at(deadline, ws.next())
            .await
            .unwrap_or_else(|_| panic!("no {kind} message arrived"))
            .expect("socket closed")
            .expect("socket error");
        if let Message::Text(text) = message {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["type"] == kind {
                return value;
            }
        }
    }
}

async fn next_binary(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    kind: u8,
    min_len: usize,
) -> Vec<u8> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let message = tokio::time::timeout_at(deadline, ws.next())
            .await
            .unwrap_or_else(|_| panic!("no binary message of kind {kind} arrived"))
            .expect("socket closed")
            .expect("socket error");
        if let Message::Binary(bytes) = message
            && bytes[0] == kind
            && bytes.len() >= min_len
        {
            return bytes.to_vec();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_can_drive_the_show_over_the_websocket() {
    let sim = Sim::start(SimConfig {
        name: "Socket panel".into(),
        leds: 64,
        matrix: Some((8, 8)),
        ..SimConfig::default()
    })
    .unwrap();
    let engine = Engine::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = server::router(engine.shared.clone(), Access::default());
    tokio::spawn(async move { axum::serve(listener, router).await });

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .unwrap();

    let hello = next_json(&mut ws, "hello").await;
    assert_eq!(hello["frame_rate"], 60);
    let state = next_json(&mut ws, "state").await;
    assert_eq!(state["show"]["devices"], json!([]));
    assert_eq!(state["show"]["layers"][0]["effect"], "plasma");

    // Add the simulated device; the reply carries our id.
    let add = json!({ "id": 1, "type": "add_device", "address": format!("127.0.0.1:{}", sim.http_port) });
    ws.send(Message::Text(add.to_string().into())).await.unwrap();
    // The new state and the reply can arrive in either order.
    let (mut result, mut state) = (None, None);
    while result.is_none() || state.is_none() {
        let deadline = Duration::from_secs(5);
        let message = tokio::time::timeout(deadline, ws.next())
            .await
            .expect("no reply to add_device");
        if let Message::Text(text) = message.unwrap().unwrap() {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["type"] == "result" {
                result = Some(value);
            } else if value["type"] == "state" && value["show"]["devices"] != json!([]) {
                state = Some(value);
            }
        }
    }
    let (result, state) = (result.unwrap(), state.unwrap());
    assert_eq!(result, json!({ "type": "result", "id": 1, "ok": true, "note": null }));
    assert_eq!(state["show"]["devices"][0]["name"], "Socket panel");
    assert_eq!(
        state["show"]["fixtures"][0]["shape"],
        json!({ "type": "matrix", "cols": 8, "rows": 8, "serpentine": false })
    );

    // Positions for all 64 pixels, then preview colours for the same layout.
    let layout = next_binary(&mut ws, MSG_LAYOUT, 8 + 64 * 8).await;
    assert_eq!(layout.len(), 8 + 64 * 8);
    let serial = &layout[4..8];
    let preview = loop {
        let frame = next_binary(&mut ws, MSG_PREVIEW, 8 + 64 * 3).await;
        if &frame[4..8] == serial {
            break frame;
        }
    };
    assert_eq!(preview.len(), 8 + 64 * 3);
    assert!(preview[8..].iter().any(|b| *b > 0), "the default effect should be lit");

    // A bad command gets an error and changes nothing.
    ws.send(Message::Text(
        json!({ "id": 2, "type": "add_layer", "effect": "nope" })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let result = next_json(&mut ws, "result").await;
    assert_eq!(result["ok"], false);
    assert_eq!(result["id"], 2);
    assert_eq!(engine.shared.show.lock().unwrap().layers.len(), 1);

    // Audio and status messages keep flowing.
    let audio = next_json(&mut ws, "audio").await;
    assert_eq!(audio["bands"].as_array().unwrap().len(), ols_core::BANDS);
    let status = next_json(&mut ws, "status").await;
    assert!(status["render"]["fps"].is_number());
    assert!(status["audio"]["state"].is_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_web_page_from_another_site_is_refused() {
    let engine = Engine::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = server::router(engine.shared.clone(), Access::default());
    tokio::spawn(async move { axum::serve(listener, router).await });

    let mut request = format!("ws://127.0.0.1:{port}/ws").into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", "https://evil.example".parse().unwrap());
    let refused = tokio_tungstenite::connect_async(request).await;
    assert!(refused.is_err(), "a foreign origin must not be able to connect");

    let mut request = format!("ws://127.0.0.1:{port}/ws").into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", format!("http://127.0.0.1:{port}").parse().unwrap());
    assert!(tokio_tungstenite::connect_async(request).await.is_ok());
}
