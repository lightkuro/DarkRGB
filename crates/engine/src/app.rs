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
use crate::library::{Library, free_path};
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
    /// Where the show is saved; `None` keeps it in memory only. It changes
    /// when another show from the library is opened.
    pub save_path: Mutex<Option<PathBuf>>,
    /// The folder of shows, unless the program was pointed at one file.
    pub library: Option<Library>,
    /// Bumped when the list of shows may have changed.
    pub library_rev: watch::Sender<u64>,
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
        Self::with_library(show, save_path, notice, None)
    }

    pub fn with_library(
        show: Show,
        save_path: Option<PathBuf>,
        notice: Option<String>,
        library: Option<Library>,
    ) -> Arc<Self> {
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
            save_path: Mutex::new(save_path),
            library,
            library_rev: watch::Sender::new(1),
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
            Command::Perform { action } => return self.perform(*action),
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
            Command::NewShow { name, keep_lights } => return self.new_show(name.as_deref(), *keep_lights),
            Command::ExportShow => return self.export_show(),
            Command::OpenShow { file } => return self.open_show(file),
            Command::DuplicateShow { file } => return self.duplicate_show(file),
            Command::DeleteShow { file } => return self.delete_show(file),
            Command::RenameShowFile { file, name } => return self.rename_show_file(file, name),
            Command::RefreshLibrary => {
                self.library_rev.send_modify(|r| *r += 1);
                return Ok(None);
            }
            _ => {}
        }
        {
            let mut show = self.show.lock().unwrap();
            // `apply` changes nothing if the command is refused.
            let before = show.clone();
            commands::apply(&mut show, command)?;
            if *show == before {
                return Ok(None);
            }
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
        if let Command::RenameShow { .. } = command {
            self.rename_file();
        }
        Ok(None)
    }

    /// Writes the show to its file now. A failure is reported in the UI.
    pub fn save(&self) {
        let Some(path) = self.save_path.lock().unwrap().clone() else {
            return;
        };
        let show = self.show.lock().unwrap().clone();
        if let Err(e) = save_show(&path, &show) {
            log::error!("could not save the show to {}: {e}", path.display());
            *self.notice.lock().unwrap() = Some(format!(
                "Your changes are not being saved: could not write {} ({e}).",
                path.display()
            ));
        }
    }

    /// The shows in the library and which one is open, for the UI.
    pub fn library_status(&self) -> serde_json::Value {
        let current = self
            .save_path
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|p| p.file_name().and_then(|f| f.to_str()).map(str::to_string));
        match &self.library {
            Some(library) => {
                let mut shows = library.list();
                // The open show is listed as it is now, not as last saved.
                if let Some(entry) = shows.iter_mut().find(|e| Some(&e.file) == current.as_ref()) {
                    let show = self.show.lock().unwrap();
                    entry.name = show.name.clone();
                    entry.lights = show.devices.len();
                    entry.effects = show.layers.len();
                    entry.scenes = show.scenes.len();
                    entry.readable = true;
                }
                serde_json::json!({
                    "available": true,
                    "current": current,
                    "folder": library.shows_dir().display().to_string(),
                    "shows": shows,
                })
            }
            None => serde_json::json!({ "available": false, "current": current, "folder": null, "shows": [] }),
        }
    }

    fn library(&self) -> Result<&Library, String> {
        self.library
            .as_ref()
            .ok_or_else(|| "shows are not kept in a library when the program is started with one show file".to_string())
    }

    /// Makes `next`, kept in `path`, the open show. The show that was
    /// open is saved first. Settings that belong to this computer and
    /// room, not to a show, stay as they are.
    fn switch_to(&self, path: PathBuf, mut next: Show) {
        self.save();
        next.repair();
        {
            let mut show = self.show.lock().unwrap();
            next.output = show.output.clone();
            next.audio = show.audio.clone();
            next.master.limit = show.master.limit;
            *show = next;
        }
        if let Some(library) = &self.library {
            library.set_current(&path);
        }
        *self.save_path.lock().unwrap() = Some(path);
        // Nothing held or waiting belongs to the new show.
        self.live.lock().unwrap().let_go();
        self.save();
        self.bump_live();
        self.bump();
        self.library_rev.send_modify(|r| *r += 1);
    }

    fn open_show(&self, file: &str) -> Result<Option<String>, String> {
        let path = self.library()?.path_of(file)?;
        if self.save_path.lock().unwrap().as_ref() == Some(&path) {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("could not read that show: {e}"))?;
        let show = checked_show(&text)?;
        self.switch_to(path, show);
        Ok(None)
    }

    fn duplicate_show(&self, file: &str) -> Result<Option<String>, String> {
        let library = self.library()?;
        let path = library.path_of(file)?;
        // The open show is copied as it is now, not as it was last saved.
        let mut copy = if self.save_path.lock().unwrap().as_ref() == Some(&path) {
            self.show.lock().unwrap().clone()
        } else {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("could not read that show: {e}"))?;
            checked_show(&text)?
        };
        copy.name = format!("{} copy", copy.name).chars().take(80).collect();
        let to = library.new_path(&copy.name);
        save_show(&to, &copy).map_err(|e| format!("could not write {}: {e}", to.display()))?;
        self.library_rev.send_modify(|r| *r += 1);
        Ok(None)
    }

    fn delete_show(&self, file: &str) -> Result<Option<String>, String> {
        let library = self.library()?;
        let path = library.path_of(file)?;
        if self.save_path.lock().unwrap().as_ref() == Some(&path) {
            return Err("that show is open; open another one first".into());
        }
        let to = library
            .to_trash(&path)
            .map_err(|e| format!("could not move that show to the trash: {e}"))?;
        log::info!("moved {} to {}", path.display(), to.display());
        self.library_rev.send_modify(|r| *r += 1);
        // The window that lists the shows says where the trash folder is.
        let name = to.file_stem().and_then(|s| s.to_str()).unwrap_or("The show");
        Ok(Some(format!("{name} was moved to the trash folder.")))
    }

    /// After a show is renamed, its file takes the new name too.
    /// Renames any show in the library. The open one goes the usual way,
    /// so the change is seen at once.
    fn rename_show_file(self: &Arc<Self>, file: &str, name: &str) -> Result<Option<String>, String> {
        let library = self.library()?;
        let path = library.path_of(file)?;
        if self.save_path.lock().unwrap().as_ref() == Some(&path) {
            return self.execute(&Command::RenameShow { name: name.into() });
        }
        let name = commands::name(name, "The show name")?;
        let text = std::fs::read_to_string(&path).map_err(|e| format!("could not read that show: {e}"))?;
        let mut show = checked_show(&text)?;
        if show.name == name {
            return Ok(None);
        }
        show.name = name;
        // Written under the new name first, so a failure loses nothing.
        let to = library.new_path(&show.name);
        save_show(&to, &show).map_err(|e| format!("could not write {}: {e}", to.display()))?;
        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!("could not remove {} after renaming it: {e}", path.display());
        }
        self.library_rev.send_modify(|r| *r += 1);
        Ok(None)
    }

    fn rename_file(&self) {
        let Some(library) = &self.library else {
            return;
        };
        let name = self.show.lock().unwrap().name.clone();
        let mut path = self.save_path.lock().unwrap();
        let Some(from) = path.clone() else {
            return;
        };
        // Already named after the show: nothing to do.
        let stem = from.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let wanted = free_path(&library.shows_dir(), &name);
        let wanted_stem = wanted.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if stem == wanted_stem || wanted_stem.strip_suffix(" (2)") == Some(stem) {
            return;
        }
        if !from.exists() {
            // Never saved yet: just save under the new name from now on.
            *path = Some(wanted.clone());
            library.set_current(&wanted);
        } else if std::fs::rename(&from, &wanted).is_ok() {
            *path = Some(wanted.clone());
            library.set_current(&wanted);
        }
        drop(path);
        self.library_rev.send_modify(|r| *r += 1);
    }

    /// A pad or its key went down or came up.
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
        if mode == KeyMode::Hold && action.can_hold() {
            if self.live.lock().unwrap().hold(binding, down) {
                self.bump_live();
            }
            return Ok(None);
        }
        if !down {
            return Ok(None);
        }
        if mode == KeyMode::Fire && action.can_fire() {
            // Set off the reaction of the effect, or of every effect in the folder.
            let layers: Vec<u32> = {
                let show = self.show.lock().unwrap();
                match action {
                    Action::Layer { id } => vec![id],
                    Action::LayerGroup { id } => show
                        .layers
                        .iter()
                        .filter(|l| l.group == Some(id))
                        .map(|l| l.id)
                        .collect(),
                    _ => Vec::new(),
                }
            };
            self.live.lock().unwrap().fire(&layers, self.now());
            self.bump_live();
            return Ok(None);
        }
        self.perform(action)
    }

    /// What a plain press does for an action.
    fn perform(self: &Arc<Self>, action: Action) -> Result<Option<String>, String> {
        match action {
            Action::Scene { id } => self.recall_scene(id),
            Action::NextScene | Action::PrevScene => {
                let step = if action == Action::NextScene { 1 } else { -1 };
                // Count from a scene that is still waiting for its beat, so
                // quick presses step on instead of asking for the same one.
                let (waiting, last) = {
                    let live = self.live.lock().unwrap();
                    (live.pending.map(|p| p.scene), live.last_scene)
                };
                let next = {
                    let show = self.show.lock().unwrap();
                    let count = show.scenes.len() as i32;
                    match waiting.and_then(|id| show.scenes.iter().position(|s| s.id == id)) {
                        Some(at) => Some(show.scenes[(at as i32 + step).rem_euclid(count) as usize].id),
                        None => show.scene_after(last, step),
                    }
                };
                match next {
                    Some(id) => self.recall_scene(id),
                    None => Ok(None),
                }
            }
            Action::TapTempo => self.execute(&Command::Tap),
            Action::Hit => self.execute(&Command::Hit),
            Action::Freeze => {
                let mut live = self.live.lock().unwrap();
                live.frozen = !live.frozen;
                drop(live);
                self.bump_live();
                Ok(None)
            }
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
            Action::Scene { .. }
            | Action::TapTempo
            | Action::Hit
            | Action::NextScene
            | Action::PrevScene
            | Action::Freeze => None,
        };
        drop(show);
        if flipped.is_some() {
            self.bump();
        }
    }

    /// A new show: no effects but the default one, no scenes, no keys.
    /// The lights and their groups come along if asked. With a library it
    /// is a new file and the show that was open stays as it is; without
    /// one it replaces the open show, which is kept in a file beside it.
    fn new_show(&self, name: Option<&str>, keep_lights: bool) -> Result<Option<String>, String> {
        let mut next = Show::default();
        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
            next.name = commands::name(name, "The show name")?;
        }
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
        match &self.library {
            Some(library) => {
                let path = library.new_path(&next.name);
                self.switch_to(path, next);
                Ok(None)
            }
            None => Ok(self.replace_show(next)),
        }
    }

    /// Replaces the whole show, keeping the one it replaces in a file
    /// beside the show file. Returns a note saying where that is.
    pub fn replace_show(&self, mut next: Show) -> Option<String> {
        next.repair();
        let previous = std::mem::replace(&mut *self.show.lock().unwrap(), next.clone());
        let kept = self.save_path.lock().unwrap().as_ref().and_then(|path| {
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

    /// Takes a show from outside the program (a file the user picked),
    /// after the same checks a show file gets. With a library it becomes
    /// a new show there and is opened; the show that was open is untouched.
    pub fn import_show(&self, json: &str) -> Result<Option<String>, String> {
        let show = checked_show(json)?;
        match &self.library {
            Some(library) => {
                let path = library.new_path(&show.name);
                self.switch_to(path, show);
                Ok(None)
            }
            None => Ok(self.replace_show(show)),
        }
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
            live.last_scene = Some(id);
            live.scene_changed();
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

/// Reads a show from text that came from outside the program, with the
/// checks that make it safe to use.
fn checked_show(json: &str) -> Result<Show, String> {
    let show: Show = serde_json::from_str(json).map_err(|e| format!("that file is not a darkRGB show ({e})"))?;
    show.validate().map_err(|e| format!("that show cannot be used: {e}"))?;
    if show.version > darkrgb_core::show::SHOW_VERSION {
        return Err("that show was saved by a newer version of darkRGB".into());
    }
    Ok(show)
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
    use darkrgb_core::Device;

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

        let note = shared
            .execute(&Command::NewShow {
                name: None,
                keep_lights: true,
            })
            .unwrap();
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

        shared
            .execute(&Command::NewShow {
                name: None,
                keep_lights: false,
            })
            .unwrap();
        assert_eq!(*shared.show.lock().unwrap(), Show::default());
    }

    /// An app with a library of its own in a temporary folder.
    fn with_library(name: &str) -> (Arc<Shared>, Library) {
        let library = Library::new(temp_dir(name));
        let path = library.start();
        let (show, notice) = load_show(&path);
        (
            Shared::with_library(show, Some(path), notice, Some(library.clone())),
            library,
        )
    }

    fn files(library: &Library) -> Vec<String> {
        let mut files: Vec<String> = library.list().into_iter().map(|e| e.file).collect();
        files.sort();
        files
    }

    #[test]
    fn shows_in_the_library_can_be_made_opened_copied_renamed_and_deleted() {
        let (shared, library) = with_library("shows");
        let run = |command: Command| shared.execute(&command);
        let current = |shared: &Shared| {
            let path = shared.save_path.lock().unwrap().clone().unwrap();
            path.file_name().unwrap().to_str().unwrap().to_string()
        };

        // Naming the first show names its file.
        run(Command::RenameShow { name: "First".into() }).unwrap();
        assert_eq!(current(&shared), "First.json");
        shared
            .show
            .lock()
            .unwrap()
            .add_device(Device::wled("d", "127.0.0.1", 4048, 10), None);
        // A setting that belongs to the computer, and one that belongs to the show.
        run(Command::SetOutput { frame_rate: Some(40) }).unwrap();
        run(Command::SetFlash {
            limit: Some(0.0),
            strict: None,
            decided: None,
        })
        .unwrap();

        // A new show is a new file. The first is saved as it was, not replaced.
        run(Command::NewShow {
            name: Some("Second".into()),
            keep_lights: false,
        })
        .unwrap();
        assert_eq!(current(&shared), "Second.json");
        assert_eq!(files(&library), ["First.json", "Second.json"]);
        {
            let show = shared.show.lock().unwrap();
            assert_eq!((show.name.as_str(), show.devices.len()), ("Second", 0));
            assert_eq!(show.output.frame_rate, 40, "this computer's settings came along");
            assert_eq!(
                (show.flash.limit, show.flash.decided),
                (3.0, false),
                "the flash choice belongs to each show"
            );
        }
        let (first, _) = load_show(&library.path_of("First.json").unwrap());
        assert_eq!(first.devices.len(), 1);

        let status = shared.library_status();
        assert_eq!(status["current"], "Second.json");
        assert_eq!(status["shows"].as_array().unwrap().len(), 2);
        // The open show is listed as it is this moment, saved or not.
        shared.show.lock().unwrap().name = "Second, renamed".into();
        let status = shared.library_status();
        let open = status["shows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["file"] == "Second.json")
            .unwrap();
        assert_eq!(open["name"], "Second, renamed");
        shared.show.lock().unwrap().name = "Second".into();

        // Opening the first brings its lights back and keeps the computer's settings.
        run(Command::SetOutput { frame_rate: Some(30) }).unwrap();
        run(Command::OpenShow {
            file: "First.json".into(),
        })
        .unwrap();
        {
            let show = shared.show.lock().unwrap();
            assert_eq!((show.name.as_str(), show.devices.len()), ("First", 1));
            assert_eq!(show.output.frame_rate, 30);
            assert_eq!(show.flash.limit, 0.0, "and the first show's own flash choice is back");
        }
        assert_eq!(
            library.start().file_name().unwrap(),
            "First.json",
            "remembered for next time"
        );

        // A copy, under a name of its own.
        run(Command::DuplicateShow {
            file: "First.json".into(),
        })
        .unwrap();
        assert_eq!(files(&library), ["First copy.json", "First.json", "Second.json"]);
        let (copy, _) = load_show(&library.path_of("First copy.json").unwrap());
        assert_eq!((copy.name.as_str(), copy.devices.len()), ("First copy", 1));

        // The open show cannot be deleted; another goes to the trash, not away.
        let delete = |file: &str| run(Command::DeleteShow { file: file.into() });
        assert!(delete("First.json").unwrap_err().contains("open"));
        let note = delete("Second.json").unwrap().unwrap();
        assert_eq!(note, "Second was moved to the trash folder.");
        assert_eq!(files(&library), ["First copy.json", "First.json"]);
        let trashed = library.shows_dir().parent().unwrap().join("trash").join("Second.json");
        assert!(trashed.is_file(), "the file is kept, in the trash");

        // Names that are not plain show files are refused.
        for bad in ["../First.json", "First", "/etc/passwd", "Second.json"] {
            assert!(run(Command::OpenShow { file: bad.into() }).is_err(), "{bad}");
            assert!(delete(bad).is_err(), "{bad}");
        }

        // Renaming the open show renames its file.
        run(Command::RenameShow { name: "Main".into() }).unwrap();
        assert_eq!(current(&shared), "Main.json");
        assert_eq!(files(&library), ["First copy.json", "Main.json"]);

        // A show that is not open can be renamed too, file and all.
        run(Command::RenameShowFile {
            file: "First copy.json".into(),
            name: "Spare".into(),
        })
        .unwrap();
        assert_eq!(files(&library), ["Main.json", "Spare.json"]);
        let (spare, _) = load_show(&library.path_of("Spare.json").unwrap());
        assert_eq!((spare.name.as_str(), spare.devices.len()), ("Spare", 1));
        assert_eq!(current(&shared), "Main.json", "the open show is untouched");
        // Asked of the open show, it is the same as renaming it.
        run(Command::RenameShowFile {
            file: "Main.json".into(),
            name: "Main set".into(),
        })
        .unwrap();
        assert_eq!(current(&shared), "Main set.json");
        assert_eq!(shared.show.lock().unwrap().name, "Main set");
        assert!(
            run(Command::RenameShowFile {
                file: "Spare.json".into(),
                name: "  ".into(),
            })
            .is_err()
        );
        run(Command::RenameShowFile {
            file: "Main set.json".into(),
            name: "Main".into(),
        })
        .unwrap();
        run(Command::RenameShowFile {
            file: "Spare.json".into(),
            name: "First copy".into(),
        })
        .unwrap();

        // A show brought in from a file joins the library; nothing is replaced.
        shared.import_show(r#"{"version":1,"name":"From a friend"}"#).unwrap();
        assert_eq!(current(&shared), "From a friend.json");
        assert_eq!(files(&library), ["First copy.json", "From a friend.json", "Main.json"]);
        assert_eq!(
            shared.show.lock().unwrap().output.frame_rate,
            30,
            "the file's own settings are not taken"
        );
        let (main, _) = load_show(&library.path_of("Main.json").unwrap());
        assert_eq!(main.devices.len(), 1);
    }

    #[test]
    fn without_a_library_the_library_commands_say_so() {
        let shared = Shared::new(Show::default(), None, None);
        let error = shared
            .execute(&Command::OpenShow { file: "A.json".into() })
            .unwrap_err();
        assert!(error.contains("library"), "{error}");
        assert_eq!(shared.library_status()["available"], false);
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
