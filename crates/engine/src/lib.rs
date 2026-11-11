//! The darkRGB engine: frame loop, audio, device output, and the local
//! server the UI talks to.
//!
//! Two programs use it. `darkrgb` (in `crates/desktop`) shows the UI in its
//! own window. `darkrgb-cli` (this crate's binary) runs the same engine
//! from a terminal, opens the UI in a browser, and has the diagnostics.

pub mod app;
pub mod commands;
pub mod diagnostics;
#[cfg(test)]
mod e2e_tests;
pub mod library;
pub mod live;
pub mod render_loop;
pub mod server;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use darkrgb_output::discovery::{Discovery, Event};
use darkrgb_output::wled;
use tokio::sync::Notify;

pub use app::{APP_NAME, BUILD, VERSION, version_text};
use app::{Discovered, Shared};

/// The port the UI is served on unless told otherwise. Fixed, so that
/// settings the UI keeps in the browser survive from one run to the next.
pub const DEFAULT_PORT: u16 = 7420;

/// How to start the engine.
#[derive(Clone, Debug)]
pub struct Options {
    pub port: u16,
    /// If `port` is taken by another program, use any free port instead of
    /// failing.
    pub any_port_if_busy: bool,
    /// Let other devices on the network open the UI. No password yet.
    pub lan: bool,
    /// Show file to load and save. `None` uses the default location.
    pub show: Option<PathBuf>,
    /// Keep the show in memory only.
    pub no_save: bool,
    /// Extra origin allowed to connect, for the UI's development server.
    pub dev_origin: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            any_port_if_busy: false,
            lan: false,
            show: None,
            no_save: false,
            dev_origin: None,
        }
    }
}

/// A running engine. Call [`Engine::shutdown`] to stop it and save.
pub struct Engine {
    pub shared: Arc<Shared>,
    /// Where the UI is served, as this computer reaches it.
    pub url: String,
    pub port: u16,
    stop: Arc<AtomicBool>,
    quit: Arc<Notify>,
    render: Option<std::thread::JoinHandle<()>>,
    runtime: Option<tokio::runtime::Runtime>,
    _discovery: Option<Discovery>,
}

/// The folder the library of shows lives in. A show saved under the
/// project's first name is carried over once.
fn library_dir() -> Option<PathBuf> {
    let config = dirs::config_dir()?;
    let dir = config.join(APP_NAME);
    let old = config.join("OpenLightShow").join("show.json");
    // Only when nothing of ours exists yet; the library then moves it in.
    if !dir.exists() && old.exists() {
        let _ = std::fs::create_dir_all(&dir);
        match std::fs::copy(&old, dir.join("show.json")) {
            Ok(_) => log::info!("carried the show over from {}", old.display()),
            Err(e) => log::warn!("could not carry the show over from {}: {e}", old.display()),
        }
    }
    Some(dir)
}

impl Engine {
    /// Starts everything and returns once the UI can be opened at `url`.
    pub fn start(options: Options) -> anyhow::Result<Self> {
        // A show file named on the command line is used by itself. Otherwise
        // shows live in the library, and the one open last is opened again.
        let library = match (&options.show, options.no_save) {
            (None, false) => library_dir().map(library::Library::new),
            _ => None,
        };
        let save_path = if options.no_save {
            None
        } else {
            options.show.clone().or_else(|| library.as_ref().map(|l| l.start()))
        };
        let (show, notice) = match &save_path {
            Some(path) => app::load_show(path),
            None => (darkrgb_core::Show::default(), None),
        };
        if let Some(notice) = &notice {
            log::warn!("{notice}");
        }
        if let (Some(library), Some(path)) = (&library, &save_path) {
            library.set_current(path);
        }
        let shared = Shared::with_library(show, save_path, notice, library);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .context("could not start the async runtime")?;

        // Bind before anything else starts, so a taken port is reported
        // without side effects.
        let ip = if options.lan {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        let listener = runtime.block_on(async {
            match tokio::net::TcpListener::bind(SocketAddr::from((ip, options.port))).await {
                Ok(listener) => Ok(listener),
                Err(_) if options.any_port_if_busy => tokio::net::TcpListener::bind(SocketAddr::from((ip, 0))).await,
                Err(e) => Err(e),
            }
        });
        let listener = listener.with_context(|| {
            format!(
                "port {} is in use. Is {APP_NAME} already running? Close it, or pick another port.",
                options.port
            )
        })?;
        let port = listener.local_addr().context("no local address")?.port();

        let stop = Arc::new(AtomicBool::new(false));
        let render = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("render".into())
                .spawn(move || render_loop::run(shared, stop))
                .context("could not start the render thread")?
        };

        // Discovery is a convenience. Without it devices are added by address.
        let discovery = start_discovery(shared.clone())
            .inspect_err(|e| log::warn!("device discovery is off: {e}"))
            .ok();

        let quit = Arc::new(Notify::new());
        runtime.spawn(save_when_changed(shared.clone()));
        runtime.spawn(refresh_inputs(shared.clone()));
        runtime.spawn(refresh_networks(shared.clone()));
        {
            let access = server::Access {
                lan: options.lan,
                dev_origin: options.dev_origin.clone(),
            };
            let (router, quit) = (server::router(shared.clone(), access), quit.clone());
            runtime.spawn(async move {
                let served = axum::serve(listener, router)
                    .with_graceful_shutdown(async move { quit.notified().await })
                    .await;
                if let Err(e) = served {
                    log::error!("the server stopped unexpectedly: {e}");
                }
            });
        }

        Ok(Self {
            shared,
            url: format!("http://127.0.0.1:{port}"),
            port,
            stop,
            quit,
            render: Some(render),
            runtime: Some(runtime),
            _discovery: discovery,
        })
    }

    /// Stops the server and the frame loop, then saves the show.
    pub fn shutdown(mut self) {
        self.stop_all();
    }

    fn stop_all(&mut self) {
        self.quit.notify_waiters();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(2));
        }
        self.stop.store(true, Ordering::Relaxed);
        if let Some(render) = self.render.take() {
            let _ = render.join();
            save_now(&self.shared);
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Saves a moment after the last change, so dragging a slider is one write.
async fn save_when_changed(shared: Arc<Shared>) {
    let mut revision = shared.revision.subscribe();
    while revision.changed().await.is_ok() {
        loop {
            tokio::time::sleep(Duration::from_millis(800)).await;
            if !revision.has_changed().unwrap_or(false) {
                break;
            }
            revision.mark_unchanged();
        }
        let shared = shared.clone();
        let _ = tokio::task::spawn_blocking(move || save_now(&shared)).await;
    }
}

fn save_now(shared: &Shared) {
    shared.save();
}

/// Keeps the list of audio inputs fresh for the UI's menu.
async fn refresh_inputs(shared: Arc<Shared>) {
    loop {
        if let Ok(inputs) = tokio::task::spawn_blocking(darkrgb_audio::list_inputs).await {
            *shared.inputs.lock().unwrap() = inputs;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Keeps track of which local networks this computer is on, so the UI can
/// suggest the start of a device's address.
async fn refresh_networks(shared: Arc<Shared>) {
    loop {
        if let Ok(networks) = tokio::task::spawn_blocking(darkrgb_output::scan::local_networks).await {
            *shared.networks.lock().unwrap() = networks;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

fn start_discovery(shared: Arc<Shared>) -> Result<Discovery, String> {
    Discovery::start(move |event| match event {
        Event::Found(found) => {
            // Ask the device about itself off the discovery thread.
            let shared = shared.clone();
            std::thread::spawn(move || {
                // Use the first advertised address that actually answers.
                for host in &found.hosts {
                    match wled::fetch_info(host, found.http_port, Duration::from_secs(3)) {
                        Ok(info) => {
                            let address = if found.http_port == 80 {
                                host.clone()
                            } else {
                                format!("{host}:{}", found.http_port)
                            };
                            shared.note_discovered(Discovered::new(found.key, address, &info, found.mac));
                            return;
                        }
                        Err(e) => log::debug!("{host} advertised itself but did not answer: {e}"),
                    }
                }
            });
        }
        Event::Lost { key } => shared.forget_discovered(&key),
    })
}
