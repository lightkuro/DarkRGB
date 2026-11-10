//! darkRGB: the engine and the server for its UI.

mod app;
mod commands;
mod diagnostics;
#[cfg(test)]
mod e2e_tests;
mod render_loop;
mod server;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use darkrgb_output::discovery::{Discovery, Event};
use darkrgb_output::wled;

use app::{APP_NAME, Discovered, Shared, VERSION};

#[derive(Parser)]
#[command(name = "darkrgb-cli", version, about = "LED show controller for WLED and friends")]
struct Cli {
    #[command(subcommand)]
    command: Option<Tool>,

    /// Port for the UI, on this computer.
    #[arg(long, default_value_t = 7420)]
    port: u16,

    /// Let other devices on the network open the UI (a phone as a remote).
    /// There is no password yet: anyone on the network can control the lights.
    #[arg(long)]
    lan: bool,

    /// Do not open the UI in a browser at start.
    #[arg(long)]
    no_open: bool,

    /// Show file to load and save. Defaults to a file in your config folder.
    #[arg(long, value_name = "FILE")]
    show: Option<PathBuf>,

    /// Keep the show in memory only; nothing is read from or written to disk.
    #[arg(long, conflicts_with = "show")]
    no_save: bool,

    /// Extra origin allowed to connect, for the UI's development server.
    #[arg(long, hide = true, value_name = "ORIGIN")]
    dev_origin: Option<String>,
}

#[derive(Subcommand)]
enum Tool {
    /// Check that system audio is captured with no setup.
    AudioTest {
        /// How long to listen.
        #[arg(long, default_value_t = 15)]
        seconds: u64,
        /// `system`, or the id of an input from the app's audio menu.
        #[arg(long, default_value = "system")]
        source: String,
    },
    /// Stream a test pattern to one device and report how steady it was.
    DdpTest {
        /// The device's IP address or host name.
        address: String,
        #[arg(long, default_value_t = 60)]
        fps: u32,
        #[arg(long, default_value_t = 20)]
        seconds: u64,
    },
    /// List WLED devices found on the network.
    Discover {
        #[arg(long, default_value_t = 5)]
        seconds: u64,
    },
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        // These two report routine disconnects and network chatter as errors.
        .filter_module("pulseaudio", log::LevelFilter::Off)
        .filter_module("mdns_sd", log::LevelFilter::Error)
        .format_timestamp_secs()
        .init();

    let cli = Cli::parse();
    if let Some(tool) = cli.command {
        let ok = match tool {
            Tool::AudioTest { seconds, source } => diagnostics::audio_test(&source, seconds.clamp(2, 600)),
            Tool::DdpTest { address, fps, seconds } => diagnostics::ddp_test(&address, fps, seconds.clamp(1, 3600)),
            Tool::Discover { seconds } => diagnostics::discover(seconds.clamp(1, 120)),
        };
        std::process::exit(if ok { 0 } else { 1 });
    }
    run(cli)
}

fn default_show_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(APP_NAME).join("show.json"))
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let save_path = if cli.no_save {
        None
    } else {
        cli.show.clone().or_else(default_show_path)
    };
    let (show, notice) = match &save_path {
        Some(path) => app::load_show(path),
        None => (darkrgb_core::Show::default(), None),
    };
    if let Some(notice) = &notice {
        log::warn!("{notice}");
    }
    let shared = Shared::new(show, save_path, notice);

    let stop = Arc::new(AtomicBool::new(false));
    let render = {
        let (shared, stop) = (shared.clone(), stop.clone());
        std::thread::Builder::new()
            .name("render".into())
            .spawn(move || render_loop::run(shared, stop))
            .context("could not start the render thread")?
    };

    // Discovery is a convenience. Without it devices are added by address.
    let _discovery = start_discovery(shared.clone())
        .inspect_err(|e| log::warn!("device discovery is off: {e}"))
        .ok();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("could not start the async runtime")?;
    let served = runtime.block_on(serve(cli, shared.clone()));

    stop.store(true, Ordering::Relaxed);
    let _ = render.join();
    save_now(&shared);
    served
}

async fn serve(cli: Cli, shared: Arc<Shared>) -> anyhow::Result<()> {
    tokio::spawn(save_when_changed(shared.clone()));
    tokio::spawn(refresh_inputs(shared.clone()));

    let ip = if cli.lan {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::LOCALHOST
    };
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((ip, cli.port)))
        .await
        .with_context(|| {
            format!(
                "port {} is in use. Is {APP_NAME} already running? Close it, or pick another port with --port.",
                cli.port
            )
        })?;
    let url = format!("http://127.0.0.1:{}", cli.port);
    println!("{APP_NAME} {VERSION} is running. Open {url}");
    if cli.lan {
        println!(
            "Other devices on your network can open it too, at this computer's address, port {}.",
            cli.port
        );
        println!("There is no password yet: anyone on the network can control the lights.");
    }
    println!("Press Ctrl+C to stop.");
    if !cli.no_open
        && let Err(e) = open::that(&url)
    {
        log::info!("could not open a browser ({e}); open {url} yourself");
    }

    let access = server::Access {
        lan: cli.lan,
        dev_origin: cli.dev_origin,
    };
    axum::serve(listener, server::router(shared, access))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("the server stopped unexpectedly")
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
    let Some(path) = &shared.save_path else {
        return;
    };
    let show = shared.show.lock().unwrap().clone();
    if let Err(e) = app::save_show(path, &show) {
        log::error!("could not save the show to {}: {e}", path.display());
        *shared.notice.lock().unwrap() = Some(format!(
            "Your changes are not being saved: could not write {} ({e}).",
            path.display()
        ));
    }
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
                            shared.note_discovered(Discovered {
                                key: found.key,
                                name: info.name,
                                address: if found.http_port == 80 {
                                    host.clone()
                                } else {
                                    format!("{host}:{}", found.http_port)
                                },
                                leds: info.led_count,
                                matrix: info.matrix,
                                version: info.version,
                                mac: info.mac.or(found.mac),
                            });
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
