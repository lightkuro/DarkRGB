//! State shared between the frame loop, the server and background tasks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use ols_audio::{AudioCapture, InputDevice};
use ols_core::{Device, Show};
use ols_output::{DeviceStats, wled};
use serde::Serialize;
use tokio::sync::watch;

use crate::commands::{self, Command};

/// The name shown to users. Kept in one place so a rename is one edit.
pub const APP_NAME: &str = "OpenLightShow";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How the frame loop is doing, refreshed about once a second.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RenderStats {
    /// Frames rendered and sent per second.
    pub fps: f32,
    /// Average time to render and send one frame, in milliseconds.
    pub frame_ms: f32,
    /// Frames that finished after their deadline.
    pub late_frames: u64,
    pub devices: HashMap<u32, DeviceStats>,
}

/// A WLED device seen on the network.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Discovered {
    /// mDNS instance name, used to notice when the device goes away.
    #[serde(skip)]
    pub key: String,
    pub name: String,
    /// What to pass to `add_device`.
    pub address: String,
    pub leds: u32,
    pub matrix: Option<(u32, u32)>,
    pub version: String,
    pub mac: Option<String>,
}

pub struct Shared {
    pub show: Mutex<Show>,
    /// Bumped on every change to the show. Clients and the saver watch it.
    pub revision: watch::Sender<u64>,
    /// Latest preview frame, already framed for the WebSocket.
    pub preview: watch::Sender<Bytes>,
    /// Latest pixel positions, already framed for the WebSocket.
    pub layout: watch::Sender<Bytes>,
    pub audio: AudioCapture,
    pub stats: Mutex<RenderStats>,
    pub discovered: Mutex<Vec<Discovered>>,
    pub inputs: Mutex<Vec<InputDevice>>,
    /// Where the show is saved; `None` keeps it in memory only.
    pub save_path: Option<PathBuf>,
    /// Something the user should be told once, such as an unreadable file.
    pub notice: Mutex<Option<String>>,
}

impl Shared {
    pub fn new(show: Show, save_path: Option<PathBuf>, notice: Option<String>) -> Arc<Self> {
        let audio = AudioCapture::start(&show.audio.source, show.audio.sensitivity);
        Arc::new(Self {
            show: Mutex::new(show),
            revision: watch::Sender::new(1),
            preview: watch::Sender::new(Bytes::new()),
            layout: watch::Sender::new(Bytes::new()),
            audio,
            stats: Mutex::new(RenderStats::default()),
            discovered: Mutex::new(Vec::new()),
            inputs: Mutex::new(Vec::new()),
            save_path,
            notice: Mutex::new(notice),
        })
    }

    fn bump(&self) {
        self.revision.send_modify(|r| *r += 1);
    }

    /// Runs a command. Returns a note for the user when there is something
    /// worth telling them about a success.
    ///
    /// `AddDevice` talks to the network and can take a few seconds, so call
    /// this off the async runtime for that command.
    pub fn execute(&self, command: &Command) -> Result<Option<String>, String> {
        if let Command::AddDevice { address } = command {
            return self.add_device(address);
        }
        {
            let mut show = self.show.lock().unwrap();
            // Work on a copy so a rejected command cannot leave the show
            // half changed.
            let mut next = show.clone();
            commands::apply(&mut next, command)?;
            if *show == next {
                return Ok(None);
            }
            *show = next;
        }
        if let Command::SetAudio { source, sensitivity } = command {
            if let Some(source) = source {
                self.audio.set_source(source);
            }
            if let Some(sensitivity) = sensitivity {
                self.audio.set_sensitivity(sensitivity.clamp(0.25, 4.0));
            }
        }
        self.bump();
        Ok(None)
    }

    fn add_device(&self, address: &str) -> Result<Option<String>, String> {
        let (host, http_port) = wled::parse_address(address)?;
        let info = wled::fetch_info(&host, http_port, Duration::from_secs(4))?;

        let mut show = self.show.lock().unwrap();
        let duplicate = show.devices.iter().any(|d| {
            let same_mac = info.mac.is_some() && d.mac == info.mac;
            same_mac || (d.address == host && d.port == info.ddp_port)
        });
        if duplicate {
            return Err(format!("{} is already in the show", info.name));
        }
        show.add_device(
            Device {
                id: 0,
                name: info.name.clone(),
                address: host,
                port: info.ddp_port,
                pixel_count: info.led_count,
                enabled: true,
                gamma: 2.2,
                brightness: 1.0,
                mac: info.mac.clone(),
                version: Some(info.version.clone()),
            },
            info.matrix,
        );
        drop(show);
        self.bump();

        Ok(info.live_source.filter(|_| info.live).map(|source| {
            format!(
                "{} is already receiving a stream ({source}). Two senders at once will flicker.",
                info.name
            )
        }))
    }

    /// Records a device found on the network, replacing an older sighting.
    pub fn note_discovered(&self, found: Discovered) {
        let mut list = self.discovered.lock().unwrap();
        list.retain(|d| d.key != found.key && d.address != found.address);
        list.push(found);
        list.sort_by(|a, b| a.name.cmp(&b.name).then(a.address.cmp(&b.address)));
    }

    pub fn forget_discovered(&self, key: &str) {
        self.discovered.lock().unwrap().retain(|d| d.key != key);
    }
}

/// Loads a show file. A missing file is a fresh start; an unreadable one is
/// set aside as `.bak` and reported, never silently overwritten.
pub fn load_show(path: &Path) -> (Show, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Show::default(), None),
        Err(e) => {
            return (
                Show::default(),
                Some(format!(
                    "Could not read {}: {e}. Started with an empty show.",
                    path.display()
                )),
            );
        }
    };
    // A file that parses but holds impossible values is as unusable as one
    // that does not parse.
    let parsed = serde_json::from_str::<Show>(&text)
        .map_err(|e| e.to_string())
        .and_then(|show| show.validate().map(|()| show));
    match parsed {
        Ok(show) if show.version <= ols_core::show::SHOW_VERSION => (show, None),
        Ok(show) => {
            let backup = set_aside(path);
            (
                Show::default(),
                Some(format!(
                    "{} was saved by a newer version (format {}). It was kept as {backup} and an empty show was started.",
                    path.display(),
                    show.version
                )),
            )
        }
        Err(e) => {
            let backup = set_aside(path);
            (
                Show::default(),
                Some(format!(
                    "{} could not be understood ({e}). It was kept as {backup} and an empty show was started.",
                    path.display()
                )),
            )
        }
    }
}

fn set_aside(path: &Path) -> String {
    let backup = path.with_extension("json.bak");
    match std::fs::rename(path, &backup) {
        Ok(()) => backup.display().to_string(),
        Err(_) => "(could not be renamed)".to_string(),
    }
}

/// Writes the show next to its final place, then swaps it in, so a crash
/// mid-write cannot leave a truncated file.
pub fn save_show(path: &Path, show: &Show) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(show).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ols-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_is_a_fresh_start() {
        let dir = temp_dir("missing");
        let (show, notice) = load_show(&dir.join("show.json"));
        assert_eq!(show, Show::default());
        assert!(notice.is_none());
    }

    #[test]
    fn shows_round_trip_through_disk() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("nested").join("show.json");
        let mut show = Show {
            name: "Saved".into(),
            ..Show::default()
        };
        show.add_layer("solid");
        save_show(&path, &show).unwrap();
        let (loaded, notice) = load_show(&path);
        assert_eq!(loaded, show);
        assert!(notice.is_none());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_broken_file_is_kept_and_reported() {
        let dir = temp_dir("broken");
        let path = dir.join("show.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        let (show, notice) = load_show(&path);
        assert_eq!(show, Show::default());
        assert!(notice.unwrap().contains("kept as"));
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(path.with_extension("json.bak")).unwrap(),
            "{ this is not json"
        );
    }

    #[test]
    fn a_file_with_impossible_sizes_is_kept_and_reported() {
        let dir = temp_dir("absurd");
        let path = dir.join("show.json");
        let mut show = Show::default();
        show.add_device(
            ols_core::Device {
                id: 0,
                name: "d".into(),
                address: "127.0.0.1".into(),
                port: 4048,
                pixel_count: 10,
                enabled: true,
                gamma: 2.2,
                brightness: 1.0,
                mac: None,
                version: None,
            },
            None,
        );
        let json = serde_json::to_string(&show)
            .unwrap()
            .replace(r#""count":10"#, r#""count":4000000000"#);
        assert!(json.contains("4000000000"));
        std::fs::write(&path, json).unwrap();
        let (loaded, notice) = load_show(&path);
        assert_eq!(loaded, Show::default());
        assert!(notice.unwrap().contains("too large"));
        assert!(path.with_extension("json.bak").exists());
    }

    #[test]
    fn a_file_from_a_newer_version_is_not_overwritten() {
        let dir = temp_dir("newer");
        let path = dir.join("show.json");
        std::fs::write(&path, r#"{"version":999,"name":"future"}"#).unwrap();
        let (show, notice) = load_show(&path);
        assert_eq!(show, Show::default());
        assert!(notice.unwrap().contains("newer version"));
        assert!(path.with_extension("json.bak").exists());
    }
}
