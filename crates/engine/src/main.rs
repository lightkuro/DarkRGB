//! `darkrgb-cli`: darkRGB from a terminal.
//!
//! Runs the same engine as the app, opens the UI in a browser, and has the
//! checks that need a terminal to print to.

use std::path::PathBuf;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use darkrgb_engine::{APP_NAME, DEFAULT_PORT, Engine, Options, diagnostics, version_text};

#[derive(Parser)]
#[command(
    name = "darkrgb-cli",
    about = "darkRGB from a terminal: browser UI, checks, and headless use"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Tool>,

    /// Port for the UI, on this computer.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Let other devices on the network open the UI (a phone as a remote).
    /// Each has to give the six-digit code printed when the program starts.
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
    /// List MIDI controllers and print what they send, to check that one is heard.
    MidiTest {
        #[arg(long, default_value_t = 20)]
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

    // The version includes the build number, which is only known at run time.
    let version: &'static str = Box::leak(version_text().into_boxed_str());
    let cli = Cli::from_arg_matches(&Cli::command().version(version).get_matches())?;
    if let Some(tool) = cli.command {
        let ok = match tool {
            Tool::AudioTest { seconds, source } => diagnostics::audio_test(&source, seconds.clamp(2, 600)),
            Tool::DdpTest { address, fps, seconds } => diagnostics::ddp_test(&address, fps, seconds.clamp(1, 3600)),
            Tool::Discover { seconds } => diagnostics::discover(seconds.clamp(1, 120)),
            Tool::MidiTest { seconds } => diagnostics::midi_test(seconds.clamp(1, 600)),
        };
        std::process::exit(if ok { 0 } else { 1 });
    }

    let engine = Engine::start(Options {
        port: cli.port,
        any_port_if_busy: false,
        lan: cli.lan,
        show: cli.show,
        no_save: cli.no_save,
        dev_origin: cli.dev_origin,
    })?;

    println!("{APP_NAME} {} is running. Open {}", version_text(), engine.url);
    if let Some(code) = &engine.pairing_code {
        println!(
            "Other devices on your network can open it too, at this computer's address, port {}.",
            engine.port
        );
        println!("They will be asked for this code: {} {}", &code[..3], &code[3..]);
    }
    println!("Press Ctrl+C to stop.");
    if !cli.no_open
        && let Err(e) = open::that(&engine.url)
    {
        log::info!("could not open a browser ({e}); open {} yourself", engine.url);
    }

    // Wait for Ctrl+C on a small runtime of our own; the engine has its own.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let _ = tokio::signal::ctrl_c().await;
        });
    engine.shutdown();
    Ok(())
}
