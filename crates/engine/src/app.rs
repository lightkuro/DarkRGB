//! State shared between the frame loop, the server and background tasks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use darkrgb_audio::{AudioCapture, InputDevice};
use darkrgb_core::{Action, AudioFeatures, Device, KeyMode, Launch, Show};
use darkrgb_output::scan::{self, LocalNetwork};
use darkrgb_output::{DeviceStats, wled};
use serde::Serialize;
use tokio::sync::watch;

use crate::commands::{self, Command};
use crate::live::{Live, Pending};

/// The name shown to users. Kept in one place so a rename is one edit.
pub const APP_NAME: &str = "darkRGB";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The CI build number, when this program was built there. Lets a bug
/// report say exactly which download it came from.
pub const BUILD: Option<&str> = option_env!("DARKRGB_BUILD");

/// The version as shown to users, such as `0.2.0 (build 12)`.
pub fn version_text() -> String {
    match BUILD {
        Some(build) if !build.is_empty() => format!("{VERSION} (build {build})"),
        _ => VERSION.to_string(),
    }
}

/// How the frame loop is doing, refreshed about once a second.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RenderStats {
    /// Frames rendered and sent per second.
    pub fps: f32,
    /// Average time to render and send one frame, in milliseconds.
    pub frame_ms: f32,
    /// Frames that finished after their deadline.
    pub late_frames: u64,
    /// True when the flash limit held the picture back in the last second.
    pub flash_limited: bool,
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

impl Discovered {
    /// `mac_hint` is used when the device did not report one itself.
    pub fn new(key: String, address: String, info: &wled::WledInfo, mac_hint: Option<String>) -> Self {
        Self {
            key,
            name: info.name.clone(),
            address,
            leds: info.led_count,
            matrix: info.matrix,
            version: info.version.clone(),
            mac: info.mac.clone().or(mac_hint),
        }
    }
}

/// Progress of a network scan, shown in the UI.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ScanStatus {
    pub running: bool,
    /// The network being scanned, such as `192.168.1.`.
    pub prefix: String,
    pub done: usize,
    pub total: usize,
    /// Devices this scan has found so far.
    pub found: usize,
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
    /// Local networks this computer is on, for suggesting addresses.
    pub networks: Mutex<Vec<LocalNetwork>>,
    pub scan: Mutex<ScanStatus>,
    /// Where the show is saved; `None` keeps it in memory only.
    pub save_path: Option<PathBuf>,
    /// Something the user should be told once, such as an unreadable file.
    pub notice: Mutex<Option<String>>,
    /// What is happening live: held keys, a tapped tempo, a waiting scene.
    pub live: Mutex<Live>,
    /// Bumped on every change to `live`.
    pub live_rev: watch::Sender<u64>,
    started: Instant,
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
            networks: Mutex::new(Vec::new()),
            scan: Mutex::new(ScanStatus::default()),
            save_path,
            notice: Mutex::new(notice),
            live: Mutex::new(Live::default()),
            live_rev: watch::Sender::new(1),
            started: Instant::now(),
        })
    }

    fn bump(&self) {
        self.revision.send_modify(|r| *r += 1);
    }

    fn bump_live(&self) {
        self.live_rev.send_modify(|r| *r += 1);
    }

    /// Seconds since the engine started: the clock the live state runs on.
    pub fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// The audio features with the tapped tempo and hand-fired beats in.
    pub fn features(&self) -> AudioFeatures {
        self.live.lock().unwrap().features(self.now(), self.audio.features())
    }

    /// Runs a command. Returns a note for the user when there is something
    /// worth telling them about a success.
    ///
    /// `AddDevice` talks to the network and can take a few seconds, so call
    /// this off the async runtime for that command.
    pub fn execute(self: &Arc<Self>, command: &Command) -> Result<Option<String>, String> {
        match command {
            Command::AddDevice { address } => return self.add_device(address),
            Command::ScanNetwork { prefix } => return self.scan_network(prefix),
            Command::Trigger { binding, down } => return self.trigger(*binding, *down),
            Command::RecallScene { id } => return self.recall_scene(*id),
            Command::Tap => {
                self.live.lock().unwrap().tap(self.now(), self.audio.features());
                self.bump_live();
                return Ok(None);
            }
            Command::Hit => {
                self.live.lock().unwrap().hit(self.now());
                self.bump_live();
                return Ok(None);
            }
            Command::AutoTempo => {
                self.live.lock().unwrap().tempo.clear();
                self.bump_live();
                return Ok(None);
            }
            Command::NewShow { keep_lights } => return self.new_show(*keep_lights),
            Command::ExportShow => return self.export_show(),
            _ => {}
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
        if let Command::SetAudio {
            source, sensitivity, ..
        } = command
        {
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

    /// A key went down or came up.
    fn trigger(self: &Arc<Self>, binding: u32, down: bool) -> Result<Option<String>, String> {
        let (action, mode) = {
            let show = self.show.lock().unwrap();
            let found = show.bindings.iter().find(|b| b.id == binding);
            match found {
                Some(b) => (b.action, b.mode),
                // Letting go of a key that was unset meanwhile must still
                // release it.
                None if !down => {
                    self.release(binding);
                    return Ok(None);
                }
                None => return Err("that key is no longer set up".into()),
            }
        };
        if action.is_switch() && mode == KeyMode::Hold {
            if self.live.lock().unwrap().hold(binding, down) {
                self.bump_live();
            }
            return Ok(None);
        }
        if !down {
            return Ok(None);
        }
        match action {
            Action::Scene { id } => self.recall_scene(id),
            Action::TapTempo => self.execute(&Command::Tap),
            Action::Hit => self.execute(&Command::Hit),
            Action::Layer { .. } | Action::LayerGroup { .. } | Action::LightGroup { .. } | Action::Blackout => {
                self.flip(action);
                Ok(None)
            }
        }
    }

    /// Flips the thing a key is tied to, for good. Nothing happens if it
    /// is not in what is playing now (an effect from another scene, say).
    fn flip(&self, action: Action) {
        let mut show = self.show.lock().unwrap();
        let flipped = match action {
            Action::Layer { id } => show.layer_mut(id).map(|l| l.enabled = !l.enabled),
            Action::LayerGroup { id } => show
                .layer_groups
                .iter_mut()
                .find(|g| g.id == id)
                .map(|g| g.enabled = !g.enabled),
            Action::LightGroup { id } => show
                .light_groups
                .iter_mut()
                .find(|g| g.id == id)
                .map(|g| g.enabled = !g.enabled),
            Action::Blackout => {
                show.master.blackout = !show.master.blackout;
                Some(())
            }
            Action::Scene { .. } | Action::TapTempo | Action::Hit => None,
        };
        drop(show);
        if flipped.is_some() {
            self.bump();
        }
    }

    /// Starts again: no effects but the default one, no scenes, no keys.
    /// The lights and their groups stay if asked.
    fn new_show(&self, keep_lights: bool) -> Result<Option<String>, String> {
        let mut next = Show::default();
        if keep_lights {
            let show = self.show.lock().unwrap();
            next.devices = show.devices.clone();
            next.fixtures = show.fixtures.clone();
            next.light_groups = show.light_groups.clone();
            next.audio = show.audio.clone();
            next.output = show.output.clone();
            next.master.limit = show.master.limit;
            // Ids are never reused, so the new show carries on counting.
            next.next_id = show.next_id.max(next.next_id);
            next.layers[0].id = next.alloc_id();
        }
        Ok(self.replace_show(next))
    }

    /// Replaces the whole show, keeping the one it replaces in a file
    /// beside the show file. Returns a note saying where that is.
    pub fn replace_show(&self, mut next: Show) -> Option<String> {
        next.repair();
        let previous = std::mem::replace(&mut *self.show.lock().unwrap(), next.clone());
        let kept = self.save_path.as_ref().and_then(|path| {
            let backup = path.with_file_name("show.previous.json");
            match save_show(&backup, &previous) {
                Ok(()) => Some(format!("The show as it was is kept in {}.", backup.display())),
                Err(e) => {
                    log::warn!("could not keep the previous show in {}: {e}", backup.display());
                    None
                }
            }
        });
        self.audio.set_source(&next.audio.source);
        self.audio.set_sensitivity(next.audio.sensitivity.clamp(0.25, 4.0));
        // Nothing held or waiting belongs to the new show.
        self.live.lock().unwrap().let_go();
        self.bump_live();
        self.bump();
        kept
    }

    /// Takes a show from outside the program (a file the user picked) in
    /// place of the current one, after the same checks a show file gets.
    pub fn import_show(&self, json: &str) -> Result<Option<String>, String> {
        let show: Show = serde_json::from_str(json).map_err(|e| format!("that file is not a darkRGB show ({e})"))?;
        show.validate().map_err(|e| format!("that show cannot be used: {e}"))?;
        if show.version > darkrgb_core::show::SHOW_VERSION {
            return Err("that show was saved by a newer version of darkRGB".into());
        }
        Ok(self.replace_show(show))
    }

    /// Writes a copy of the show to the Documents folder and says where.
    fn export_show(&self) -> Result<Option<String>, String> {
        let show = self.show.lock().unwrap().clone();
        let folder = dirs::document_dir()
            .or_else(dirs::home_dir)
            .ok_or("could not find a folder to save to")?
            .join(APP_NAME);
        let path = free_path(&folder, &show.name);
        save_show(&path, &show).map_err(|e| format!("could not write {}: {e}", path.display()))?;
        Ok(Some(format!("Saved a copy to {}", path.display())))
    }

    /// Lets go of a held key, however many times it was pressed. Used when
    /// the client that held it goes away.
    pub fn release(&self, binding: u32) {
        let mut live = self.live.lock().unwrap();
        let mut changed = false;
        while live.is_held(binding) {
            changed |= live.hold(binding, false);
        }
        drop(live);
        if changed {
            self.bump_live();
        }
    }

    /// Switches to a scene: at once, or on a coming beat when the show is
    /// set to wait for one and there is a tempo to wait for.
    fn recall_scene(&self, id: u32) -> Result<Option<String>, String> {
        let launch = {
            let show = self.show.lock().unwrap();
            if !show.scenes.iter().any(|s| s.id == id) {
                return Err("that scene no longer exists".into());
            }
            show.live.launch
        };
        let now = self.now();
        let waits = launch != Launch::Now && self.features().bpm > 0.0;
        if waits {
            self.live.lock().unwrap().pending = Some(Pending { scene: id, since: now });
            self.bump_live();
        } else {
            self.launch_scene(id);
        }
        Ok(None)
    }

    /// Switches to a scene now and forgets any scene that was waiting.
    pub fn launch_scene(&self, id: u32) {
        let changed = {
            let mut show = self.show.lock().unwrap();
            let before = show.clone();
            show.recall_scene(id) && *show != before
        };
        {
            let mut live = self.live.lock().unwrap();
            live.pending = None;
            if changed {
                live.scene_serial += 1;
            }
        }
        self.bump_live();
        if changed {
            self.bump();
        }
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
                mac: info.mac.clone(),
                version: Some(info.version.clone()),
                ..Device::wled(&info.name, &host, info.ddp_port, info.led_count)
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

    /// Starts asking every address under `prefix` whether it is a WLED
    /// device. Returns at once; progress is in `scan`, and devices that
    /// answer join the discovered list.
    pub fn scan_network(self: &Arc<Self>, prefix: &str) -> Result<Option<String>, String> {
        let octets = scan::parse_prefix(prefix)?;
        let hosts = scan::hosts_of(octets);
        {
            let mut status = self.scan.lock().unwrap();
            if status.running {
                return Err("a scan is already running".into());
            }
            *status = ScanStatus {
                running: true,
                prefix: format!("{}.{}.{}.", octets[0], octets[1], octets[2]),
                done: 0,
                total: hosts.len(),
                found: 0,
            };
        }
        let (on_found, on_progress, finished) = (self.clone(), self.clone(), self.clone());
        std::thread::Builder::new()
            .name("scan".into())
            .spawn(move || {
                scan::scan_hosts(
                    hosts,
                    80,
                    Duration::from_millis(900),
                    move |ip, info| {
                        on_found.note_discovered(Discovered::new(format!("scan:{ip}"), ip.to_string(), &info, None));
                        on_found.scan.lock().unwrap().found += 1;
                    },
                    move |done| on_progress.scan.lock().unwrap().done = done,
                );
                finished.scan.lock().unwrap().running = false;
            })
            .map_err(|e| format!("could not start the scan: {e}"))?;
        Ok(None)
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
        Ok(mut show) if show.version <= darkrgb_core::show::SHOW_VERSION => {
            show.repair();
            (show, None)
        }
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

/// A file name for a show that is safe on any system and not yet taken:
/// `Name.json`, then `Name (2).json`, and so on.
fn free_path(folder: &Path, show_name: &str) -> PathBuf {
    let mut stem: String = show_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    stem = stem.trim().chars().take(60).collect();
    if stem.is_empty() {
        stem = "show".into();
    }
    let mut path = folder.join(format!("{stem}.json"));
    let mut n = 2;
    while path.exists() {
        path = folder.join(format!("{stem} ({n}).json"));
        n += 1;
    }
    path
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
        let dir = std::env::temp_dir().join(format!("darkrgb-test-{}-{name}", std::process::id()));
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
        show.add_device(darkrgb_core::Device::wled("d", "127.0.0.1", 4048, 10), None);
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
    fn exported_files_get_safe_names_that_are_not_taken() {
        let dir = temp_dir("export");
        let first = free_path(&dir, "My/Show: one?");
        assert_eq!(first.file_name().unwrap(), "My_Show_ one_.json");
        std::fs::write(&first, "x").unwrap();
        let second = free_path(&dir, "My/Show: one?");
        assert_eq!(second.file_name().unwrap(), "My_Show_ one_ (2).json");
        assert_eq!(free_path(&dir, "  ").file_name().unwrap(), "show.json");
        assert_eq!(free_path(&dir, "../../etc").file_name().unwrap(), "______etc.json");
    }

    #[test]
    fn a_new_show_can_keep_the_lights_and_the_old_show_is_kept_beside_it() {
        let dir = temp_dir("newshow");
        let path = dir.join("show.json");
        let mut show = Show::default();
        show.add_device(darkrgb_core::Device::wled("d", "127.0.0.1", 4048, 10), None);
        let group = show.add_light_group("Ceiling");
        show.fixtures[0].group = Some(group);
        show.add_layer("solid");
        show.save_scene("One");
        let shared = Shared::new(show.clone(), Some(path.clone()), None);

        let note = shared.execute(&Command::NewShow { keep_lights: true }).unwrap();
        assert!(note.unwrap().contains("show.previous.json"));
        let (kept, _) = load_show(&path.with_file_name("show.previous.json"));
        assert_eq!(kept, show);
        {
            let now = shared.show.lock().unwrap();
            assert_eq!((now.devices.len(), now.light_groups.len()), (1, 1));
            assert_eq!(now.fixtures[0].group, Some(group));
            assert_eq!((now.layers.len(), now.scenes.len(), now.bindings.len()), (1, 0, 0));
            // No id is handed out twice.
            let mut ids: Vec<u32> = vec![now.devices[0].id, now.fixtures[0].id, group, now.layers[0].id];
            ids.dedup();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), 4);
        }

        shared.execute(&Command::NewShow { keep_lights: false }).unwrap();
        assert_eq!(*shared.show.lock().unwrap(), Show::default());
    }

    #[test]
    fn an_imported_show_is_checked_like_a_show_file() {
        let shared = Shared::new(Show::default(), None, None);
        assert!(
            shared
                .import_show("not json")
                .unwrap_err()
                .contains("not a darkRGB show")
        );
        assert!(
            shared
                .import_show(r#"{"version":999,"name":"future"}"#)
                .unwrap_err()
                .contains("newer version")
        );
        let absurd =
            r#"{"version":1,"name":"x","devices":[{"id":1,"name":"d","address":"a","pixel_count":4000000000}]}"#;
        assert!(shared.import_show(absurd).unwrap_err().contains("cannot be used"));
        assert_eq!(
            *shared.show.lock().unwrap(),
            Show::default(),
            "a refused file changes nothing"
        );

        // A good file replaces the show, and loose ends in it are mended.
        let good = r#"{"version":1,"name":"Imported","layers":[{"id":3,"effect":"solid","group":77}]}"#;
        assert_eq!(shared.import_show(good).unwrap(), None, "nowhere to keep the old show");
        let show = shared.show.lock().unwrap();
        assert_eq!(show.name, "Imported");
        assert_eq!(show.layers[0].group, None);
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
