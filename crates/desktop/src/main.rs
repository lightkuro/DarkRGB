//! darkRGB as a desktop app: one program, one window.
//!
//! The engine runs inside this process and serves the UI on this computer
//! only. The window is a web view pointed at it. The page gets no special
//! access to the system: it talks to the engine over the same WebSocket a
//! browser would use.

// A windowed program on Windows must not open a terminal alongside itself.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use darkrgb_engine::{APP_NAME, Engine, Options, version_text};
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

/// A windowed program has no terminal to print to, so the log goes to a
/// file next to the saved show. Runs append to it (a second copy being
/// started must not wipe the first one's log); it starts afresh once it
/// has grown past a megabyte.
fn start_logging() {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder
        .filter_module("pulseaudio", log::LevelFilter::Off)
        .filter_module("mdns_sd", log::LevelFilter::Error)
        // The windowing libraries narrate every message on the session bus.
        .filter_module("zbus", log::LevelFilter::Warn)
        .filter_module("tracing", log::LevelFilter::Warn)
        .format_timestamp_secs();
    let file = dirs::config_dir().and_then(|dir| {
        let dir = dir.join(APP_NAME);
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("darkrgb.log");
        let grown = std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_000_000);
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(!grown)
            .truncate(grown)
            .open(path)
            .ok()
    });
    if let Some(file) = file {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }
    let _ = builder.try_init();
}

/// A page shown in place of the UI when the engine cannot start.
fn problem_page(message: &str) -> String {
    let escaped = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "data:text/html,<!doctype html><meta charset=utf-8><title>{APP_NAME}</title>\
         <body style=\"font:15px system-ui;background:%23090a0d;color:%23e8eaf0;margin:3rem;max-width:36rem\">\
         <h1 style=\"font-size:20px\">{APP_NAME} could not start</h1><p>{escaped}</p>"
    )
}

fn main() {
    start_logging();
    log::info!("{APP_NAME} {}", version_text());

    // The engine is started once the single-instance check has passed, so
    // a second copy never streams to the lights alongside the first.
    let engine: &'static Mutex<Option<Engine>> = Box::leak(Box::new(Mutex::new(None)));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Started a second time: bring the window that is already open forward.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(move |app| {
            let started = Engine::start(Options {
                // If another program holds the usual port, any free one will do.
                any_port_if_busy: true,
                ..Options::default()
            });
            let url = match started {
                Ok(running) => {
                    let url = running.url.clone();
                    *engine.lock().unwrap() = Some(running);
                    url
                }
                Err(error) => {
                    log::error!("could not start: {error:#}");
                    problem_page(&format!("{error:#}"))
                }
            };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.parse()?))
                .title(APP_NAME)
                .inner_size(1440.0, 900.0)
                .min_inner_size(900.0, 600.0)
                // Lets the page handle its own dragging, such as reordering effects.
                .disable_drag_drop_handler()
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("could not create the application window");

    app.run(move |_app, event| {
        if let RunEvent::Exit = event {
            // Stops the lights' stream and saves the show.
            if let Some(running) = engine.lock().unwrap().take() {
                running.shutdown();
            }
        }
    });
}
