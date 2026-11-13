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
            each: None,
            only: None,
            angle: None,
            spin: None,
            turns: None,
            mirror: None,
            repeat: None,
            kaleido: None,
            spread: None,
            hue: None,
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
            address: None,
            universe: None,
            order: None,
            max_milliamps: None,
            delay_ms: None,
            dither: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let before = sim.stats().frames;
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(sim.stats().frames, before, "a disabled device must receive nothing");
}

#[test]
fn stopping_the_engine_leaves_the_light_dark() {
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
    drop(engine);
    wait_for("dark after the engine has stopped", || {
        sim.frame().iter().all(|b| *b == 0)
    });
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
    let router = server::service(engine.shared.clone(), Access::default());
    tokio::spawn(async move { axum::serve(listener, router).await });

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .unwrap();

    let hello = next_json(&mut ws, "hello").await;
    assert_eq!(hello["frame_rates"], serde_json::json!([30, 40, 60, 120]));
    let state = next_json(&mut ws, "state").await;
    assert_eq!(state["show"]["devices"], json!([]));
    assert_eq!(state["show"]["layers"][0]["effect"], "plasma");

    // Add the simulated device; the reply carries our number.
    let add = json!({ "req": 1, "type": "add_device", "address": format!("127.0.0.1:{}", sim.http_port) });
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
    assert_eq!(result, json!({ "type": "result", "req": 1, "ok": true, "note": null }));
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
        json!({ "req": 2, "type": "add_layer", "effect": "nope" })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let result = next_json(&mut ws, "result").await;
    assert_eq!(result["ok"], false);
    assert_eq!(result["req"], 2);
    assert_eq!(engine.shared.show.lock().unwrap().layers.len(), 1);

    // A command that names a device by its id is answered under the
    // request's number, not the device's.
    let device = engine.shared.show.lock().unwrap().devices[0].id;
    ws.send(Message::Text(
        json!({ "req": 77, "type": "update_device", "id": device, "name": "Panel" })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let result = next_json(&mut ws, "result").await;
    assert_eq!((&result["req"], &result["ok"]), (&json!(77), &json!(true)));
    assert_eq!(engine.shared.show.lock().unwrap().devices[0].name, "Panel");
    // And without a number, a command that works is not answered at all.
    ws.send(Message::Text(
        json!({ "type": "update_device", "id": device, "name": "Socket panel" })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();

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
    let router = server::service(engine.shared.clone(), Access::default());
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

/// The colour of the first light, as the simulator last received it.
fn colour_at(sim: &Sim) -> [u8; 3] {
    let frame = sim.frame();
    if frame.len() < 3 {
        [1, 2, 3]
    } else {
        [frame[0], frame[1], frame[2]]
    }
}

#[test]
fn a_scene_can_be_changed_while_the_lights_play_another() {
    let (engine, sim) = engine_with_strip();
    let shared = &engine.shared;
    let colour = |hex: &str| {
        let layer = shared.show.lock().unwrap().layers[0].id;
        shared
            .execute(&Command::SetParam {
                layer,
                param: "colour".into(),
                value: ParamValue::Text(hex.into()),
            })
            .unwrap();
    };
    make_solid(shared, "#ff0000");
    shared.execute(&Command::SaveScene { name: "Red".into() }).unwrap();
    colour("#0000ff");
    shared.execute(&Command::SaveScene { name: "Blue".into() }).unwrap();
    let (red, blue) = {
        let show = shared.show.lock().unwrap();
        (show.scenes[0].id, show.scenes[1].id)
    };
    shared.execute(&Command::RecallScene { id: red }).unwrap();
    wait_for("red", || colour_at(&sim) == [255, 0, 0]);

    // Blue is opened to be changed. The lights stay red; the preview
    // shows blue.
    let mut preview = shared.preview.subscribe();
    shared.execute(&Command::EditScene { id: blue }).unwrap();
    let status = shared.live.lock().unwrap().status();
    assert_eq!((status.blind, status.editing), (true, Some(blue)));
    colour("#00ff00");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(colour_at(&sim), [255, 0, 0], "the lights were not touched");
    preview.mark_unchanged();
    wait_for("a preview frame", || preview.has_changed().unwrap_or(false));
    let frame = preview.borrow_and_update().clone();
    assert_eq!(frame[8..11], [0, 255, 0], "the preview shows what is being changed");
    // A scene asked for meanwhile goes to the lights, not into the panel.
    shared.launch_scene(red);
    assert_eq!(
        shared.show.lock().unwrap().layers[0].params["colour"],
        ParamValue::Text("#00ff00".into())
    );

    // Done: the change is kept in Blue, and the panel is back on Red.
    shared.execute(&Command::FinishEdit { play: false }).unwrap();
    {
        let show = shared.show.lock().unwrap();
        let kept = show.scenes.iter().find(|s| s.id == blue).unwrap();
        assert_eq!(kept.layers[0].params["colour"], ParamValue::Text("#00ff00".into()));
        assert_eq!(show.layers[0].params["colour"], ParamValue::Text("#ff0000".into()));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(colour_at(&sim), [255, 0, 0]);
    assert!(shared.execute(&Command::FinishEdit { play: false }).is_err());
    // Changed again and put on the lights this time.
    shared.execute(&Command::EditScene { id: blue }).unwrap();
    colour("#ffffff");
    shared.execute(&Command::FinishEdit { play: true }).unwrap();
    wait_for("white", || colour_at(&sim) == [255, 255, 255]);
    assert!(!shared.live.lock().unwrap().status().blind);
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
                key: Some(key.into()),
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
            each: None,
            only: None,
            angle: None,
            spin: None,
            turns: None,
            mirror: None,
            repeat: None,
            kaleido: None,
            spread: None,
            hue: None,
        })
        .unwrap();
    // With no sound and no beat, the pulse stays dark.
    wait_for("frames", || sim.stats().frames > 20);
    assert!(all(&sim, 0), "{:?}", sim.frame());
    shared
        .execute(&Command::Bind {
            key: Some("Space".into()),
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
            speed: None,
            auto: None,
            every_bars: None,
            shuffle: None,
            on_drops: None,
            midi_clock: None,
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
            speed: None,
            auto: None,
            every_bars: None,
            shuffle: None,
            on_drops: None,
            midi_clock: None,
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
            key: Some("KeyF".into()),
            label: "F".into(),
            action: Action::Layer { id: layer },
            mode: KeyMode::Hold,
        })
        .unwrap();
    let key = shared.show.lock().unwrap().bindings[0].id;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = server::service(shared.clone(), Access::default());
    tokio::spawn(async move { axum::serve(listener, router).await });
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
        .await
        .unwrap();

    // The first state and the first live message come in either order.
    let (mut state, mut live) = (None, None);
    while state.is_none() || live.is_none() {
        let message = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("the first messages never arrived");
        if let Message::Text(text) = message.unwrap().unwrap() {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["type"] == "state" {
                state = Some(value);
            } else if value["type"] == "live" {
                live = Some(value);
            }
        }
    }
    let (state, live) = (state.unwrap(), live.unwrap());
    assert_eq!(state["show"]["bindings"][0]["key"], "KeyF");
    assert_eq!(state["current_scene"], Value::Null);
    assert_eq!(live["held"], json!([]));
    assert_eq!(live["frozen"], false);

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

#[test]
fn a_performance_is_recorded_onto_the_timeline_and_plays_back() {
    use darkrgb_core::timeline::ClipKind;
    let engine = Engine::start();
    let shared = &engine.shared;
    let run = |command: Command| shared.execute(&command).unwrap();
    run(Command::SaveScene { name: "Verse".into() });
    run(Command::SaveScene { name: "Chorus".into() });
    run(Command::Bind {
        key: Some("KeyB".into()),
        label: String::new(),
        action: Action::Blackout,
        mode: KeyMode::Hold,
    });
    let layer = shared.show.lock().unwrap().layers[0].id;
    run(Command::Bind {
        key: Some("KeyQ".into()),
        label: String::new(),
        action: Action::Layer { id: layer },
        mode: KeyMode::Toggle,
    });
    let (verse, chorus, dark, switch) = {
        let show = shared.show.lock().unwrap();
        (
            show.scenes[0].id,
            show.scenes[1].id,
            show.binding_for(Action::Blackout).unwrap().id,
            show.binding_for(Action::Layer { id: layer }).unwrap().id,
        )
    };
    run(Command::SetTimeline {
        length: Some(30.0),
        bpm: None,
        offset: None,
        repeat: None,
    });
    let position = || shared.live.lock().unwrap().status().timeline.position;
    let clips = || shared.show.lock().unwrap().timeline.clips.clone();
    let lights_out = || {
        shared
            .live
            .lock()
            .unwrap()
            .overlay(&shared.show.lock().unwrap())
            .master
            .blackout
    };
    let press = |binding: u32, down: bool| run(Command::Trigger { binding, down });

    // Armed but not playing, nothing is written.
    run(Command::TimelineRecord { on: true });
    press(dark, true);
    press(dark, false);
    assert!(clips().is_empty());

    run(Command::TimelinePlay { from: Some(1.0) });
    wait_for("the timeline to run", || position() > 1.05);
    run(Command::RecallScene { id: verse });
    std::thread::sleep(Duration::from_millis(150));
    press(dark, true);
    std::thread::sleep(Duration::from_millis(300));
    assert!(lights_out(), "a pad being recorded still does what it does");
    press(dark, false);
    // A pad that switches is on from one press to the next.
    press(switch, true);
    press(switch, false);
    std::thread::sleep(Duration::from_millis(250));
    let enabled = || shared.live.lock().unwrap().overlay(&shared.show.lock().unwrap()).layers[0].enabled;
    assert!(!enabled(), "switched off, as the press asks");
    press(switch, true);
    press(switch, false);
    assert!(enabled());
    std::thread::sleep(Duration::from_millis(100));
    run(Command::RecallScene { id: chorus });
    // A pad still held when playback stops ends there.
    press(dark, true);
    std::thread::sleep(Duration::from_millis(120));
    run(Command::TimelineStop);
    press(dark, false);
    let stopped_at = position();

    let recorded = clips();
    let kinds: Vec<ClipKind> = recorded.iter().map(|c| c.what).collect();
    assert_eq!(
        kinds,
        [
            ClipKind::Scene { id: verse },
            ClipKind::Pad { id: dark },
            ClipKind::Pad { id: switch },
            ClipKind::Scene { id: chorus },
            ClipKind::Pad { id: dark },
        ]
    );
    assert!(
        recorded.windows(2).all(|w| w[0].start <= w[1].start),
        "in the order they were played"
    );
    assert!(
        (0.25..0.6).contains(&recorded[1].length),
        "held for 0.3 s: {}",
        recorded[1].length
    );
    assert!(
        (0.2..0.5).contains(&recorded[2].length),
        "on for 0.25 s: {}",
        recorded[2].length
    );
    assert!(
        (recorded[4].end() as f64 - stopped_at).abs() < 0.08,
        "ends where playback stopped"
    );
    assert!(
        !shared.live.lock().unwrap().status().timeline.recording,
        "stopping ends the recording"
    );
    // The show underneath was not changed by any of it.
    {
        let show = shared.show.lock().unwrap();
        assert!(show.layers[0].enabled && !show.master.blackout);
    }

    // Played back, the lights go out where the pad was held.
    let from = recorded[1].start as f64 - 0.2;
    run(Command::TimelinePlay { from: Some(from) });
    wait_for("playback", || position() > from + 0.02);
    assert!(!lights_out());
    wait_for("the recorded blackout", lights_out);
    assert!((position() - recorded[1].start as f64).abs() < 0.15);
    wait_for("it to end", || !lights_out());
    run(Command::TimelineStop);

    // What was recorded can be undone, a step at a time: first where
    // the last clip ended, then the clip itself.
    run(Command::Undo);
    assert_eq!(clips().len(), recorded.len());
    assert!(clips()[4].length < recorded[4].length);
    run(Command::Undo);
    assert_eq!(clips().len(), recorded.len() - 1);
    // Bad places are refused; a place past the end is brought back to it.
    assert!(shared.execute(&Command::TimelineSeek { position: f64::NAN }).is_err());
    run(Command::TimelineSeek { position: 1.0e9 });
    wait_for("the jump", || (position() - 30.0).abs() < 1e-6);
    // Played from the very end, it starts over.
    run(Command::TimelinePlay { from: None });
    wait_for("it to start over", || position() < 5.0 && position() > 0.0);
    run(Command::TimelineStop);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_song_is_kept_for_the_timeline_and_handed_back() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let engine = Engine::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = server::service(engine.shared.clone(), Access::default());
    tokio::spawn(async move { axum::serve(listener, router).await });
    let request = |method: &'static str, path: String, origin: bool, body: Vec<u8>| async move {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let origin = if origin {
            format!("Origin: http://127.0.0.1:{port}\r\n")
        } else {
            String::new()
        };
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{origin}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).await.unwrap();
        stream.write_all(&body).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let split = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .unwrap_or(response.len());
        let head = String::from_utf8_lossy(&response[..split]).to_string();
        (head, response[(split + 4).min(response.len())..].to_vec())
    };
    let sound: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();

    // Only the served page may send one, and only a sound file is taken.
    let (head, _) = request("POST", "/api/song?name=Night%20drive.mp3".into(), false, sound.clone()).await;
    assert!(head.contains(" 403 "), "{head}");
    let (head, body) = request("POST", "/api/song?name=notes.txt".into(), true, sound.clone()).await;
    assert!(head.contains(" 400 "), "{head}");
    assert!(String::from_utf8_lossy(&body).contains("sound file"));
    let (head, _) = request("POST", "/api/song?name=empty.mp3".into(), true, Vec::new()).await;
    assert!(head.contains(" 400 "), "{head}");
    assert_eq!(engine.shared.show.lock().unwrap().timeline.song, None);

    let (head, body) = request("POST", "/api/song?name=Night%20drive.MP3".into(), true, sound.clone()).await;
    assert!(head.contains(" 200 "), "{head} {}", String::from_utf8_lossy(&body));
    let song = engine
        .shared
        .show
        .lock()
        .unwrap()
        .timeline
        .song
        .clone()
        .expect("the show names its song");
    assert_eq!(song.name, "Night drive.MP3");
    assert!(
        song.file.ends_with(".mp3") && darkrgb_core::timeline::song_file_ok(&song.file),
        "{}",
        song.file
    );

    // Handed back byte for byte, as sound.
    let (head, body) = request("GET", format!("/api/song/{}", song.file), false, Vec::new()).await;
    assert!(
        head.contains(" 200 ") && head.to_lowercase().contains("content-type: audio/mpeg"),
        "{head}"
    );
    assert_eq!(body, sound);
    // Nothing else can be fetched this way.
    for bad in ["nothing.mp3", "..%2F..%2Fetc%2Fpasswd", "x.txt", ".mp3"] {
        let (head, _) = request("GET", format!("/api/song/{bad}"), false, Vec::new()).await;
        assert!(head.contains(" 404 "), "{bad}: {head}");
    }
    // The same song sent again is the same file, and undo lets it go.
    let (_, _) = request("POST", "/api/song?name=again.mp3".into(), true, sound.clone()).await;
    assert_eq!(
        engine.shared.show.lock().unwrap().timeline.song.as_ref().unwrap().file,
        song.file
    );
    engine.shared.execute(&Command::RemoveSong).unwrap();
    assert_eq!(engine.shared.show.lock().unwrap().timeline.song, None);
    let _ = std::fs::remove_file(engine.shared.songs_dir().join(&song.file));
}

/// Sends one request as a device elsewhere on the network would: under
/// the computer's network name, not `127.0.0.1`. Returns the status
/// line, the headers and the body.
async fn from_the_network(
    port: u16,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: &str,
) -> (String, String, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let cookie = cookie.map(|c| format!("Cookie: {c}\r\n")).unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: studio.local:{port}\r\nOrigin: http://studio.local:{port}\r\n{cookie}\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head.lines().next().unwrap_or_default().to_string();
    (status, head.to_string(), body.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn another_device_is_let_in_only_after_giving_the_code() {
    use crate::pairing::Pairing;
    let engine = Engine::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let access = Access {
        lan: true,
        pairing: Some(Arc::new(Pairing::with("048213", "a-long-random-token"))),
        ..Access::default()
    };
    tokio::spawn(async move { axum::serve(listener, server::service(engine.shared.clone(), access)).await });
    let socket = |cookie: Option<&str>, host: String| {
        let mut request = format!("ws://127.0.0.1:{port}/ws").into_client_request().unwrap();
        let headers = request.headers_mut();
        headers.insert("Host", host.parse().unwrap());
        headers.insert("Origin", format!("http://{host}").parse().unwrap());
        if let Some(cookie) = cookie {
            headers.insert("Cookie", cookie.parse().unwrap());
        }
        tokio_tungstenite::connect_async(request)
    };
    let network = format!("studio.local:{port}");

    // The page loads for anyone, and is told it has to ask for the code.
    let (status, _, body) = from_the_network(port, "GET", "/", None, "").await;
    assert!(status.contains("200") && body.contains("<html"), "{status}");
    let (_, _, body) = from_the_network(port, "GET", "/api/access", None, "").await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({ "paired": false })
    );
    // Until then: no show, no control, no importing.
    assert!(socket(None, network.clone()).await.is_err());
    assert!(socket(Some("darkrgb_pair=048213"), network.clone()).await.is_err());
    let (status, _, _) = from_the_network(port, "POST", "/api/import", None, r#"{"version":1,"name":"x"}"#).await;
    assert!(status.contains("403"), "{status}");

    // A wrong code is refused and says how many tries are left.
    let (status, head, body) = from_the_network(port, "POST", "/api/pair", None, r#"{"code":"111111"}"#).await;
    assert!(status.contains("403"), "{status}");
    assert!(!head.to_lowercase().contains("set-cookie"));
    assert!(body.contains("4 more tries"), "{body}");

    // The right one hands over a cookie scripts cannot read.
    let (status, head, body) = from_the_network(port, "POST", "/api/pair", None, r#"{"code":"048 213"}"#).await;
    assert!(status.contains("200"), "{status} {body}");
    let cookie = head
        .lines()
        .find_map(|l| {
            l.to_lowercase()
                .starts_with("set-cookie:")
                .then(|| l[11..].trim().to_string())
        })
        .expect("a cookie");
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
        "{cookie}"
    );
    let cookie = cookie.split(';').next().unwrap().to_string();
    let (_, _, body) = from_the_network(port, "GET", "/api/access", Some(&cookie), "").await;
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!({ "paired": true }));

    // With it the device gets the show, but not the code, and it cannot
    // change who is let in.
    let (mut ws, _) = socket(Some(&cookie), network.clone()).await.unwrap();
    next_json(&mut ws, "state").await;
    let status = next_json(&mut ws, "status").await;
    assert_eq!(
        (&status["remote"]["local"], &status["remote"]["code"]),
        (&json!(false), &Value::Null)
    );
    ws.send(Message::Text(r#"{"req":5,"type":"set_remote","enabled":false}"#.into()))
        .await
        .unwrap();
    let result = next_json(&mut ws, "result").await;
    assert_eq!(result["ok"], false);

    // This computer itself is never asked, and is shown the code.
    let (mut ws, _) = socket(None, format!("127.0.0.1:{port}")).await.unwrap();
    let status = next_json(&mut ws, "status").await;
    assert_eq!(
        (&status["remote"]["local"], &status["remote"]["code"]),
        (&json!(true), &json!("048213"))
    );

    // Five wrong codes and the device has to wait, even with the right
    // one. (Pairing above cleared the earlier wrong try.)
    for _ in 0..5 {
        from_the_network(port, "POST", "/api/pair", None, r#"{"code":"000000"}"#).await;
    }
    let (status, _, body) = from_the_network(port, "POST", "/api/pair", None, r#"{"code":"048213"}"#).await;
    assert!(status.contains("429"), "{status} {body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_show_file_can_be_imported_but_only_by_the_served_page() {
    let engine = Engine::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let router = server::service(engine.shared.clone(), Access::default());
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

#[test]
fn a_hundred_and_twenty_frames_a_second_reach_the_device() {
    let (engine, sim) = engine_with_strip();
    engine
        .shared
        .execute(&Command::SetOutput { frame_rate: Some(120) })
        .unwrap();
    wait_for("the stream to start", || sim.stats().frames > 5);
    std::thread::sleep(Duration::from_millis(200));
    sim.reset_stats();
    std::thread::sleep(Duration::from_secs(2));
    let stats = sim.stats();
    assert!(stats.fps > 110.0 && stats.fps < 128.0, "{stats:?}");
}

/// Counts how often the device went from dark to lit, sampling it for
/// `seconds`. A device shows what it last received, so this is what a
/// person standing in front of it would see.
fn flashes_seen(sim: &Sim, seconds: f32) -> usize {
    let started = Instant::now();
    let (mut count, mut was_lit) = (0, false);
    while started.elapsed().as_secs_f32() < seconds {
        let lit = sim.frame().first().is_some_and(|b| *b > 128);
        if lit && !was_lit {
            count += 1;
        }
        was_lit = lit;
        std::thread::sleep(Duration::from_millis(2));
    }
    count
}

#[test]
fn a_strobe_keeps_to_the_limit_by_itself_and_runs_free_with_it_off() {
    let (engine, sim) = engine_with_strip();
    let shared = &engine.shared;
    let layer = shared.show.lock().unwrap().layers[0].id;
    shared
        .execute(&Command::UpdateLayer {
            id: layer,
            effect: Some("strobe".into()),
            opacity: None,
            blend: None,
            enabled: None,
            targets: None,
            each: None,
            only: None,
            angle: None,
            spin: None,
            turns: None,
            mirror: None,
            repeat: None,
            kaleido: None,
            spread: None,
            hue: None,
        })
        .unwrap();
    shared
        .execute(&Command::SetParam {
            layer,
            param: "rate".into(),
            value: ParamValue::Number(10.0),
        })
        .unwrap();
    wait_for("frames", || sim.stats().frames > 30);

    // With the limit on, the strobe slows itself to just under three a
    // second: about eight in three seconds, all of them whole.
    std::thread::sleep(Duration::from_millis(1200));
    // A busy build machine may show a flash fewer; never more than three a second.
    let limited = flashes_seen(&sim, 3.0);
    assert!(
        (4..=9).contains(&limited),
        "{limited} flashes in three seconds with the limit on"
    );
    // The limit has nothing left to soften.
    std::thread::sleep(Duration::from_millis(1100));
    assert!(!engine.shared.stats.lock().unwrap().flash_limited);

    // One command switches it off, and the strobe runs at its own rate.
    shared
        .execute(&Command::SetFlash {
            limit: Some(0.0),
            strict: None,
            decided: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let free = flashes_seen(&sim, 3.0);
    assert!(
        (18..=32).contains(&free),
        "{free} flashes in three seconds with the limit off"
    );
    assert!(
        shared.show.lock().unwrap().flash.decided,
        "switching it off is a choice"
    );
}

#[test]
fn flashing_that_no_effect_announced_is_still_limited_at_the_device() {
    // A white layer switched on and off by hand, ten times a second: no
    // effect here says "I flash", so only the limit on the finished
    // picture can catch it.
    let (engine, sim) = engine_with_strip();
    let shared = engine.shared.clone();
    let layer = shared.show.lock().unwrap().layers[0].id;
    shared
        .execute(&Command::UpdateLayer {
            id: layer,
            effect: Some("solid".into()),
            opacity: None,
            blend: None,
            enabled: None,
            targets: None,
            each: None,
            only: None,
            angle: None,
            spin: None,
            turns: None,
            mirror: None,
            repeat: None,
            kaleido: None,
            spread: None,
            hue: None,
        })
        .unwrap();
    shared
        .execute(&Command::SetParam {
            layer,
            param: "colour".into(),
            value: ParamValue::Text("#ffffff".into()),
        })
        .unwrap();
    wait_for("frames", || sim.stats().frames > 30);

    let stop = Arc::new(AtomicBool::new(false));
    let flicker = {
        let (shared, stop) = (shared.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut on = false;
            while !stop.load(Ordering::Relaxed) {
                on = !on;
                let _ = shared.execute(&Command::UpdateLayer {
                    id: layer,
                    effect: None,
                    opacity: None,
                    blend: None,
                    enabled: Some(on),
                    targets: None,
                    each: None,
                    only: None,
                    angle: None,
                    spin: None,
                    turns: None,
                    mirror: None,
                    repeat: None,
                    kaleido: None,
                    spread: None,
                    hue: None,
                });
                std::thread::sleep(Duration::from_millis(50));
            }
        })
    };
    let limited = flashes_seen(&sim, 3.0);
    assert!(
        (3..=10).contains(&limited),
        "{limited} flashes in three seconds with the limit on"
    );
    wait_for("the limit to be reported", || {
        engine.shared.stats.lock().unwrap().flash_limited
    });

    // With the limit off, all ten a second get through.
    shared
        .execute(&Command::SetFlash {
            limit: Some(0.0),
            strict: None,
            decided: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let free = flashes_seen(&sim, 3.0);
    stop.store(true, Ordering::Relaxed);
    flicker.join().unwrap();
    assert!(
        (15..=32).contains(&free),
        "{free} flashes in three seconds with the limit off"
    );
}

#[test]
fn a_fired_pad_sets_off_its_effect_and_a_held_scene_comes_and_goes() {
    use crate::commands::QuickPad;
    let (engine, sim) = engine_with_strip();
    let shared = &engine.shared;
    make_solid(shared, "#000000");
    wait_for("dark", || all(&sim, 0));
    let press = |binding, down| shared.execute(&Command::Trigger { binding, down });

    // A ready-made pulse: dark until fired, whatever the music does.
    shared.execute(&Command::AddQuickPad { kind: QuickPad::Pulse }).unwrap();
    let pulse = shared.show.lock().unwrap().bindings[0].id;
    std::thread::sleep(Duration::from_millis(150));
    assert!(all(&sim, 0));
    press(pulse, true).unwrap();
    wait_for("the pulse", || sim.frame().iter().any(|b| *b > 60));
    press(pulse, false).unwrap();
    wait_for("the pulse to fade", || all(&sim, 0));

    // A ready-made flash: white while held.
    shared.execute(&Command::AddQuickPad { kind: QuickPad::Flash }).unwrap();
    let flash = shared.show.lock().unwrap().bindings[1].id;
    press(flash, true).unwrap();
    wait_for("white while held", || all(&sim, 255));
    press(flash, false).unwrap();
    wait_for("dark again", || all(&sim, 0));

    // A scene on a held key shows only while the key is down.
    shared.execute(&Command::SaveScene { name: "Dark".into() }).unwrap();
    make_solid(shared, "#ffffff");
    shared.execute(&Command::SaveScene { name: "Bright".into() }).unwrap();
    wait_for("white", || all(&sim, 255));
    let dark = shared.show.lock().unwrap().scenes[0].id;
    shared
        .execute(&Command::Bind {
            key: Some("KeyQ".into()),
            label: "Q".into(),
            action: Action::Scene { id: dark },
            mode: KeyMode::Hold,
        })
        .unwrap();
    let key = shared
        .show
        .lock()
        .unwrap()
        .binding_for(Action::Scene { id: dark })
        .unwrap()
        .id;
    let before = shared.show.lock().unwrap().clone();
    press(key, true).unwrap();
    wait_for("the held scene", || all(&sim, 0));
    assert_eq!(*shared.show.lock().unwrap(), before, "holding a scene saves nothing");
    press(key, false).unwrap();
    wait_for("back to what was playing", || all(&sim, 255));
}

#[test]
fn next_scene_previous_scene_freeze_and_speed() {
    let (engine, sim) = engine_with_strip();
    let shared = &engine.shared;
    for (name, colour) in [("Red", "#ff0000"), ("Green", "#00ff00"), ("Blue", "#0000ff")] {
        make_solid(shared, colour);
        shared.execute(&Command::SaveScene { name: name.into() }).unwrap();
    }
    let pad = |action| {
        shared
            .execute(&Command::Bind {
                key: None,
                label: String::new(),
                action,
                mode: KeyMode::Toggle,
            })
            .unwrap();
        shared.show.lock().unwrap().binding_for(action).unwrap().id
    };
    let press = |binding| {
        shared.execute(&Command::Trigger { binding, down: true }).unwrap();
        shared.execute(&Command::Trigger { binding, down: false }).unwrap();
    };
    let colour = || {
        let f = sim.frame();
        (f[0] > 128, f[1] > 128, f[2] > 128)
    };
    let (next, previous) = (pad(Action::NextScene), pad(Action::PrevScene));
    wait_for("blue", || colour() == (false, false, true));
    press(next);
    wait_for("round to red", || colour() == (true, false, false));
    press(next);
    wait_for("green", || colour() == (false, true, false));
    press(previous);
    press(previous);
    wait_for("back round to blue", || colour() == (false, false, true));

    // Freeze: a moving effect stops moving, and blackout still works.
    let layer = shared.show.lock().unwrap().layers[0].id;
    shared
        .execute(&Command::UpdateLayer {
            id: layer,
            effect: Some("plasma".into()),
            opacity: None,
            blend: None,
            enabled: None,
            targets: None,
            each: None,
            only: None,
            angle: None,
            spin: None,
            turns: None,
            mirror: None,
            repeat: None,
            kaleido: None,
            spread: None,
            hue: None,
        })
        .unwrap();
    let moving = |sim: &Sim| {
        let first = sim.frame();
        std::thread::sleep(Duration::from_millis(250));
        sim.frame() != first
    };
    wait_for("plasma to move", || moving(&sim));
    let freeze = pad(Action::Freeze);
    press(freeze);
    assert!(shared.live.lock().unwrap().status().frozen);
    std::thread::sleep(Duration::from_millis(100));
    assert!(!moving(&sim), "the picture should stand still while frozen");
    shared
        .execute(&Command::SetMaster {
            brightness: None,
            blackout: Some(true),
            limit: None,
        })
        .unwrap();
    wait_for("blackout while frozen", || all(&sim, 0));
    shared
        .execute(&Command::SetMaster {
            brightness: None,
            blackout: Some(false),
            limit: None,
        })
        .unwrap();
    press(freeze);
    wait_for("moving again", || moving(&sim));

    // Speed is kept within its range.
    shared
        .execute(&Command::SetLive {
            launch: None,
            fade: None,
            speed: Some(0.01),
            auto: None,
            every_bars: None,
            shuffle: None,
            on_drops: None,
            midi_clock: None,
        })
        .unwrap();
    assert_eq!(shared.show.lock().unwrap().live.speed, 0.25);
}

#[test]
fn autopilot_changes_scene_by_itself_and_a_press_restarts_its_count() {
    let engine = Engine::start();
    let shared = &engine.shared;
    for (name, colour) in [("Red", "#ff0000"), ("Green", "#00ff00"), ("Blue", "#0000ff")] {
        make_solid(shared, colour);
        shared.execute(&Command::SaveScene { name: name.into() }).unwrap();
    }
    let ids: Vec<u32> = shared.show.lock().unwrap().scenes.iter().map(|s| s.id).collect();
    let current = || shared.show.lock().unwrap().current_scene();
    assert_eq!(current(), Some(ids[2]));

    // No music in the test, so autopilot keeps time by the clock: one bar is two seconds.
    let started = Instant::now();
    shared
        .execute(&Command::SetLive {
            launch: None,
            fade: None,
            speed: None,
            auto: Some(true),
            every_bars: Some(1),
            shuffle: None,
            on_drops: None,
            midi_clock: None,
        })
        .unwrap();
    wait_for("the first change", || current() == Some(ids[0]));
    let first = started.elapsed();
    assert!(
        first > Duration::from_millis(1700) && first < Duration::from_millis(2600),
        "the first change came after {first:?}"
    );

    // Choosing a scene by hand is respected, and its bar is counted from the press.
    shared.execute(&Command::RecallScene { id: ids[2] }).unwrap();
    let pressed = Instant::now();
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(
        current(),
        Some(ids[2]),
        "autopilot must not take over right after a press"
    );
    wait_for("autopilot to carry on", || current() == Some(ids[0]));
    assert!(pressed.elapsed() > Duration::from_millis(1700));

    // A scene marked to skip is skipped.
    shared
        .execute(&Command::SetSceneAuto {
            id: ids[1],
            auto: false,
        })
        .unwrap();
    wait_for("the next change", || current() != Some(ids[0]));
    assert_eq!(current(), Some(ids[2]), "green is skipped");

    shared
        .execute(&Command::SetLive {
            launch: None,
            fade: None,
            speed: None,
            auto: Some(false),
            every_bars: None,
            shuffle: None,
            on_drops: None,
            midi_clock: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(current(), Some(ids[2]), "off means it stays");
}
