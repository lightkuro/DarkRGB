//! End-to-end tests: the real frame loop and server against a simulated
//! WLED device, over real sockets on this machine.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use darkrgb_core::params::ParamValue;
use darkrgb_core::{Action, KeyMode, Launch, Show};
use darkrgb_sim::{Sim, SimConfig};
use futures_util::{SinkExt, StreamExt};
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

fn wait_for(what: &str, done: impl FnMut() -> bool) {
    wait_up_to(Duration::from_secs(5), what, done);
}

fn wait_up_to(limit: Duration, what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < limit, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Replaces the default layer with a solid colour, which is easy to check.
fn make_solid(shared: &Arc<Shared>, colour: &str) {
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
            limit: None,
        })
        .unwrap();
    wait_for("black", || sim.frame().iter().all(|b| *b == 0));

    engine
        .shared
        .execute(&Command::SetMaster {
            brightness: Some(0.5),
            blackout: Some(false),
            limit: None,
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
    assert_eq!(hello["frame_rates"], serde_json::json!([30, 40, 60]));
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
    assert_eq!(audio["bands"].as_array().unwrap().len(), darkrgb_core::BANDS);
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

#[test]
fn a_scan_finds_devices_that_answer_and_skips_the_rest() {
    use std::net::Ipv4Addr;
    use std::sync::Mutex;

    let sim = Sim::start(SimConfig {
        name: "Found by scan".into(),
        leds: 42,
        ..SimConfig::default()
    })
    .unwrap();
    let found = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Mutex::new(Vec::new()));
    let (f, p) = (found.clone(), progress.clone());
    // 127.0.0.1 answers on the simulator's port; 127.0.0.2 has nothing there.
    darkrgb_output::scan::scan_hosts(
        vec![Ipv4Addr::new(127, 0, 0, 2), Ipv4Addr::LOCALHOST],
        sim.http_port,
        Duration::from_millis(800),
        move |ip, info| f.lock().unwrap().push((ip, info.name, info.led_count)),
        move |done| p.lock().unwrap().push(done),
    );
    assert_eq!(
        *found.lock().unwrap(),
        [(Ipv4Addr::LOCALHOST, "Found by scan".to_string(), 42)]
    );
    let mut progress = progress.lock().unwrap().clone();
    progress.sort_unstable();
    assert_eq!(progress, [1, 2]);
}

#[test]
fn a_scan_is_refused_outside_local_networks_and_while_one_runs() {
    let engine = Engine::start();
    let scan = |prefix: &str| engine.shared.execute(&Command::ScanNetwork { prefix: prefix.into() });
    assert!(scan("8.8.8.").unwrap_err().contains("only local networks"));
    assert!(scan("nonsense").is_err());
    assert!(!engine.shared.scan.lock().unwrap().running);
    // A link-local network nobody is on: starts, and a second one is refused meanwhile.
    scan("169.254.77.").unwrap();
    {
        let status = engine.shared.scan.lock().unwrap().clone();
        assert!(status.running);
        assert_eq!((status.prefix.as_str(), status.total), ("169.254.77.", 254));
    }
    assert!(scan("169.254.78.").unwrap_err().contains("already running"));
    // Where the network drops packets for these addresses instead of
    // refusing them, every address waits out its full timeout: 254
    // addresses, 32 at a time, 0.9 s each is a little over 7 s.
    wait_up_to(Duration::from_secs(30), "the scan to finish", || {
        !engine.shared.scan.lock().unwrap().running
    });
    assert_eq!(engine.shared.scan.lock().unwrap().done, 254);
}

#[test]
fn demo_lights_are_previewed_but_never_sent_anywhere() {
    let sim = Sim::start(SimConfig {
        leds: 10,
        ..SimConfig::default()
    })
    .unwrap();
    let engine = Engine::start();
    engine.shared.execute(&Command::SetDemo { enabled: true }).unwrap();
    engine
        .shared
        .execute(&Command::AddDevice {
            address: format!("127.0.0.1:{}", sim.http_port),
        })
        .unwrap();
    wait_for("frames at the real device", || sim.stats().frames > 10);
    // The real device gets its ten LEDs and nothing more.
    assert_eq!(sim.stats().rejected, 0);
    wait_for("send statistics", || {
        !engine.shared.stats.lock().unwrap().devices.is_empty()
    });
    let stats = engine.shared.stats.lock().unwrap().clone();
    let show = engine.shared.show.lock().unwrap().clone();
    assert_eq!(show.devices.len(), 4);
    // Only the real device has send statistics at all.
    let real = show.devices.iter().find(|d| !d.demo).unwrap().id;
    assert_eq!(stats.devices.keys().copied().collect::<Vec<_>>(), [real]);
    assert_eq!(show.pixel_count(), 120 + 90 + 256 + 10);
}

/// An engine streaming to a small simulated strip.
fn engine_with_strip() -> (Engine, Sim) {
    let sim = Sim::start(SimConfig {
        leds: 8,
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
    (engine, sim)
}

fn all(sim: &Sim, value: u8) -> bool {
    let frame = sim.frame();
    !frame.is_empty() && frame.iter().all(|b| *b == value)
}

#[test]
fn a_held_key_flashes_and_a_toggled_key_stays() {
    let (engine, sim) = engine_with_strip();
    let shared = &engine.shared;
    make_solid(shared, "#ffffff");
    wait_for("white", || all(&sim, 255));
    let layer = shared.show.lock().unwrap().layers[0].id;
    let bind = |key: &str, action, mode| {
        shared
            .execute(&Command::Bind {
                key: key.into(),
                label: String::new(),
                action,
                mode,
            })
            .unwrap();
        shared.show.lock().unwrap().binding_for(action).unwrap().id
    };
    let press = |binding, down| shared.execute(&Command::Trigger { binding, down });

    // Held: the effect is cut only while the key is down, and the saved
    // show never changes.
    let cut = bind("KeyA", Action::Layer { id: layer }, KeyMode::Hold);
    let saved = shared.show.lock().unwrap().clone();
    press(cut, true).unwrap();
    wait_for("dark while held", || all(&sim, 0));
    assert_eq!(*shared.show.lock().unwrap(), saved);
    assert_eq!(shared.live.lock().unwrap().status().held, [cut]);
    press(cut, false).unwrap();
    wait_for("white again", || all(&sim, 255));

    // Toggled: each press flips it; letting go does nothing.
    let dark = bind("KeyB", Action::Blackout, KeyMode::Toggle);
    press(dark, true).unwrap();
    wait_for("blackout", || all(&sim, 0));
    press(dark, false).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(all(&sim, 0) && shared.show.lock().unwrap().master.blackout);
    press(dark, true).unwrap();
    wait_for("lights back", || all(&sim, 255));

    // A key that was removed while held is still let go of.
    press(cut, true).unwrap();
    shared.execute(&Command::Unbind { id: cut }).unwrap();
    assert!(press(cut, true).is_err());
    press(cut, false).unwrap();
    assert!(shared.live.lock().unwrap().status().held.is_empty());
}

#[test]
fn a_beat_fired_by_hand_sets_off_effects_that_wait_for_one() {
    let (engine, sim) = engine_with_strip();
    let shared = &engine.shared;
    let layer = shared.show.lock().unwrap().layers[0].id;
    shared
        .execute(&Command::UpdateLayer {
            id: layer,
            effect: Some("beat_pulse".into()),
            opacity: None,
            blend: None,
            enabled: None,
            targets: None,
        })
        .unwrap();
    // With no sound and no beat, the pulse stays dark.
    wait_for("frames", || sim.stats().frames > 20);
    assert!(all(&sim, 0), "{:?}", sim.frame());
    shared
        .execute(&Command::Bind {
            key: "Space".into(),
            label: String::new(),
            action: Action::Hit,
            mode: KeyMode::Toggle,
        })
        .unwrap();
    let key = shared.show.lock().unwrap().bindings[0].id;
    shared
        .execute(&Command::Trigger {
            binding: key,
            down: true,
        })
        .unwrap();
    wait_for("the flash", || sim.frame().iter().any(|b| *b > 60));
    wait_for("the flash to fade", || all(&sim, 0));
}

#[test]
fn a_scene_waits_for_the_beat_when_asked_to() {
    let engine = Engine::start();
    let shared = &engine.shared;
    make_solid(shared, "#ffffff");
    shared.execute(&Command::SaveScene { name: "White".into() }).unwrap();
    make_solid(shared, "#000000");
    shared.execute(&Command::SaveScene { name: "Black".into() }).unwrap();
    let (white, black) = {
        let show = shared.show.lock().unwrap();
        (show.scenes[0].id, show.scenes[1].id)
    };
    let current = || shared.show.lock().unwrap().current_scene();
    assert_eq!(current(), Some(black));
    shared
        .execute(&Command::SetLive {
            launch: Some(Launch::Beat),
            fade: None,
        })
        .unwrap();

    // With no tempo there is no beat to wait for: the scene starts at once.
    shared.execute(&Command::RecallScene { id: white }).unwrap();
    assert_eq!(current(), Some(white));

    // Tap a tempo of about 150 beats a minute, then ask for the other scene.
    shared.execute(&Command::Tap).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    shared.execute(&Command::Tap).unwrap();
    let bpm = shared.live.lock().unwrap().status().tapped_bpm;
    assert!((120.0..=165.0).contains(&bpm), "tapped {bpm}");
    let asked = Instant::now();
    shared.execute(&Command::RecallScene { id: black }).unwrap();
    assert_eq!(shared.live.lock().unwrap().status().pending_scene, Some(black));
    assert_eq!(current(), Some(white), "still the old scene until the beat");
    wait_for("the beat", || current() == Some(black));
    let waited = asked.elapsed();
    assert!(
        waited > Duration::from_millis(150) && waited < Duration::from_millis(900),
        "waited {waited:?}"
    );
    assert_eq!(shared.live.lock().unwrap().status().pending_scene, None);

    // Back to the music's own tempo.
    shared.execute(&Command::AutoTempo).unwrap();
    assert_eq!(shared.live.lock().unwrap().status().tapped_bpm, 0.0);
    assert!(shared.execute(&Command::RecallScene { id: 9999 }).is_err());
}

#[test]
fn a_scene_change_can_fade_in() {
    let (engine, sim) = engine_with_strip();
    let shared = &engine.shared;
    make_solid(shared, "#000000");
    shared.execute(&Command::SaveScene { name: "Dark".into() }).unwrap();
    make_solid(shared, "#ffffff");
    shared.execute(&Command::SaveScene { name: "Bright".into() }).unwrap();
    wait_for("white", || all(&sim, 255));
    let dark = shared.show.lock().unwrap().scenes[0].id;
    shared
        .execute(&Command::SetLive {
            launch: None,
            fade: Some(0.6),
        })
        .unwrap();

    let started = Instant::now();
    shared.execute(&Command::RecallScene { id: dark }).unwrap();
    let mut in_between = false;
    wait_for("dark", || {
        let frame = sim.frame();
        in_between |= frame.iter().all(|b| *b > 10 && *b < 245);
        all(&sim, 0)
    });
    assert!(in_between, "the change should pass through the shades in between");
    let took = started.elapsed();
    assert!(
        took > Duration::from_millis(350) && took < Duration::from_millis(1500),
        "the fade took {took:?}"
    );
}

#[test]
fn the_frame_rate_can_be_lowered() {
    let (engine, sim) = engine_with_strip();
    engine
        .shared
        .execute(&Command::SetOutput { frame_rate: Some(30) })
        .unwrap();
    wait_for("the stream to start", || sim.stats().frames > 5);
    std::thread::sleep(Duration::from_millis(200));
    sim.reset_stats();
    std::thread::sleep(Duration::from_secs(2));
    let stats = sim.stats();
    assert!(stats.fps > 27.0 && stats.fps < 33.0, "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_held_by_a_client_that_disappears_is_let_go() {
    let engine = Engine::start();
    let shared = engine.shared.clone();
    let layer = shared.show.lock().unwrap().layers[0].id;
    shared
        .execute(&Command::Bind {
            key: "KeyF".into(),
            label: "F".into(),
            action: Action::Layer { id: layer },
            mode: KeyMode::Hold,
        })
        .unwrap();
    let key = shared.show.lock().unwrap().bindings[0].id;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = server::router(shared.clone(), Access::default());
    tokio::spawn(async move { axum::serve(listener, router).await });
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .unwrap();

    let state = next_json(&mut ws, "state").await;
    assert_eq!(state["show"]["bindings"][0]["key"], "KeyF");
    assert_eq!(state["current_scene"], Value::Null);
    let live = next_json(&mut ws, "live").await;
    assert_eq!(live["held"], json!([]));

    let press = json!({ "type": "trigger", "binding": key, "down": true });
    ws.send(Message::Text(press.to_string().into())).await.unwrap();
    let live = next_json(&mut ws, "live").await;
    assert_eq!(live["held"], json!([key]));

    // The window closes with the key still down.
    drop(ws);
    let deadline = Instant::now() + Duration::from_secs(5);
    while shared.live.lock().unwrap().is_held(key) {
        assert!(Instant::now() < deadline, "the key stayed held after its client left");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Sends one HTTP request by hand and returns the status line and body.
async fn post(port: u16, origin: Option<&str>, body: &str) -> (String, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let origin = origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default();
    let request = format!(
        "POST /api/import HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{origin}Content-Type: text/plain\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response.lines().next().unwrap_or_default().to_string();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
    (status, body)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_show_file_can_be_imported_but_only_by_the_served_page() {
    let engine = Engine::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = server::router(engine.shared.clone(), Access::default());
    tokio::spawn(async move { axum::serve(listener, router).await });
    let own = format!("http://127.0.0.1:{port}");
    let good = r#"{"version":1,"name":"From a file"}"#;

    // Another site's page, or no page at all, is refused and changes nothing.
    let (status, _) = post(port, Some("http://evil.example"), good).await;
    assert!(status.contains("403"), "{status}");
    let (status, _) = post(port, None, good).await;
    assert!(status.contains("403"), "{status}");
    assert_eq!(engine.shared.show.lock().unwrap().name, "Untitled show");

    // A file that is not a show is refused with a reason.
    let (status, body) = post(port, Some(&own), "{ nope").await;
    assert!(status.contains("400"), "{status}");
    let reply: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(reply["ok"], false);
    assert!(reply["error"].as_str().unwrap().contains("not a darkRGB show"));

    let (status, body) = post(port, Some(&own), good).await;
    assert!(status.contains("200"), "{status} {body}");
    assert_eq!(engine.shared.show.lock().unwrap().name, "From a file");

    // Far too large to be a show.
    let huge = "x".repeat(9 * 1024 * 1024);
    let (status, _) = post(port, Some(&own), &huge).await;
    assert!(status.contains("413"), "{status}");
}
