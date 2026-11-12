//! State shared between the frame loop, the server and background tasks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use darkrgb_audio::{AudioCapture, InputDevice};
use darkrgb_core::{Action, AudioFeatures, Device, KeyMode, Kind, KnobTarget, Launch, Only, Show};
use darkrgb_output::scan::{self, LocalNetwork};
use darkrgb_output::{DeviceStats, wled};
use serde::Serialize;
use tokio::sync::watch;

use crate::commands::{self, Command};
use crate::library::{Library, MAX_PALETTES, MAX_PATTERNS, Pattern, Saved, SavedPalette, free_path};
use crate::live::{Live, Pending};
use crate::midi::{self, LearnTarget, MidiEvent, MidiStatus};

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

/// Earlier and later states of the show, for undo and redo.
#[derive(Default)]
pub struct History {
    undo: Vec<(Show, std::mem::Discriminant<Command>, Instant)>,
    redo: Vec<Show>,
}

impl History {
    /// How many steps back are kept.
    const DEPTH: usize = 100;
    /// A run of the same kind of change within this long is one step, so
    /// dragging a slider is undone in one go and not a frame at a time.
    const RUN: std::time::Duration = std::time::Duration::from_millis(900);

    /// Notes the show as it was before a change of this kind.
    pub fn remember(&mut self, before: Show, kind: std::mem::Discriminant<Command>) {
        let now = Instant::now();
        self.redo.clear();
        if let Some((_, last_kind, at)) = self.undo.last_mut()
            && *last_kind == kind
            && now.duration_since(*at) < Self::RUN
        {
            // Still the same drag: keep the state from before it began.
            *at = now;
            return;
        }
        self.undo.push((before, kind, now));
        if self.undo.len() > Self::DEPTH {
            self.undo.remove(0);
        }
    }

    /// Notes the show as it was before a change that no command made.
    pub fn remember_other(&mut self, before: Show) {
        self.redo.clear();
        self.undo.push((
            before,
            std::mem::discriminant(&Command::Undo),
            Instant::now() - Self::RUN,
        ));
        if self.undo.len() > Self::DEPTH {
            self.undo.remove(0);
        }
    }

    /// One step back or forward from `current`. `None` at either end.
    fn step(&mut self, back: bool, current: Show) -> Option<Show> {
        if back {
            let (show, _, _) = self.undo.pop()?;
            self.redo.push(current);
            Some(show)
        } else {
            let show = self.redo.pop()?;
            // Old enough that the next change starts a step of its own.
            self.undo.push((
                current,
                std::mem::discriminant(&Command::Redo),
                Instant::now() - Self::RUN,
            ));
            Some(show)
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Whether there is anything to undo, and to redo.
    pub fn can(&self) -> (bool, bool) {
        (!self.undo.is_empty(), !self.redo.is_empty())
    }
}

/// Whether other devices on the network may open the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Remote {
    /// The server is listening on the network now.
    pub on: bool,
    /// What the user chose in Settings, for the next start.
    pub wanted: bool,
    /// The port the UI is served on.
    pub port: u16,
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
    /// Effects, looks and palettes kept to use in any show.
    pub saved: Mutex<Saved>,
    /// Earlier states of the show, for undo and redo.
    pub history: Mutex<History>,
    /// MIDI controllers: which are heard, and what is being learnt.
    pub midi: Mutex<MidiStatus>,
    /// Where knobs were last turned to, waiting for the next frame. A
    /// knob sends far more often than a frame is drawn.
    pub knob_moves: Mutex<Vec<(KnobTarget, f32)>>,
    /// Something the user should be told once, such as an unreadable file.
    pub notice: Mutex<Option<String>>,
    /// Whether other devices on the network may connect.
    pub remote: Mutex<Remote>,
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
        let saved = library.as_ref().map(Library::load_saved).unwrap_or_default();
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
            saved: Mutex::new(saved),
            history: Mutex::new(History::default()),
            midi: Mutex::new(MidiStatus::default()),
            knob_moves: Mutex::new(Vec::new()),
            notice: Mutex::new(notice),
            remote: Mutex::new(Remote::default()),
            live: Mutex::new(Live::default()),
            live_rev: watch::Sender::new(1),
            started: Instant::now(),
        })
    }

    fn bump(&self) {
        self.revision.send_modify(|r| *r += 1);
    }

    pub(crate) fn bump_live(&self) {
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
            Command::NewShow {
                name,
                keep_lights,
                kind,
            } => return self.new_show(name.as_deref(), *keep_lights, *kind),
            Command::ExportShow => return self.export_show(),
            Command::SetRemote { enabled } => return self.set_remote(*enabled),
            Command::OpenShow { file } => return self.open_show(file),
            Command::DuplicateShow { file } => return self.duplicate_show(file),
            Command::DeleteShow { file } => return self.delete_show(file),
            Command::RenameShowFile { file, name } => return self.rename_show_file(file, name),
            Command::RefreshLibrary => {
                self.library_rev.send_modify(|r| *r += 1);
                return Ok(None);
            }
            Command::Undo => return self.step_history(true),
            Command::Redo => return self.step_history(false),
            Command::MidiLearn { target } => {
                if let Some(LearnTarget::Pad { binding }) = target
                    && !self.show.lock().unwrap().bindings.iter().any(|b| b.id == *binding)
                {
                    return Err("that pad is no longer set up".into());
                }
                self.midi.lock().unwrap().learning = *target;
                return Ok(None);
            }
            Command::SavePattern { layer, name } => return self.save_pattern(Some(*layer), name.as_deref()),
            Command::SaveLook { name } => return self.save_pattern(None, Some(name)),
            Command::UsePattern { id, group } => return self.use_pattern(*id, *group),
            Command::RenamePattern { id, name } => {
                let name = commands::name(name, "The name")?;
                return self.change_saved(|saved| {
                    saved
                        .patterns
                        .iter_mut()
                        .find(|p| p.id == *id)
                        .map(|p| p.name = name)
                        .ok_or_else(|| "that pattern is no longer there".to_string())
                });
            }
            Command::DeletePattern { id } => {
                return self.change_saved(|saved| {
                    let before = saved.patterns.len();
                    saved.patterns.retain(|p| p.id != *id);
                    (saved.patterns.len() < before)
                        .then_some(())
                        .ok_or_else(|| "that pattern is no longer there".to_string())
                });
            }
            Command::SavePalette { name, value } => {
                let name = commands::name(name, "The name")?;
                let own = darkrgb_core::color::parse_custom(value)
                    .ok_or("that is not a palette of your own: it needs 2 to 8 colours")?;
                let value = darkrgb_core::color::write_custom(&own);
                return self.change_saved(|saved| {
                    // The same name again replaces the colours kept under it.
                    if let Some(existing) = saved.palettes.iter_mut().find(|p| p.name == name) {
                        existing.value = value;
                        return Ok(());
                    }
                    if saved.palettes.len() >= MAX_PALETTES {
                        return Err(format!("at most {MAX_PALETTES} palettes can be kept"));
                    }
                    let id = saved.alloc_id();
                    saved.palettes.push(SavedPalette { id, name, value });
                    Ok(())
                });
            }
            Command::DeletePalette { id } => {
                return self.change_saved(|saved| {
                    let before = saved.palettes.len();
                    saved.palettes.retain(|p| p.id != *id);
                    (saved.palettes.len() < before)
                        .then_some(())
                        .ok_or_else(|| "that palette is no longer there".to_string())
                });
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
            if commands::undoable(command) {
                self.history
                    .lock()
                    .unwrap()
                    .remember(before, std::mem::discriminant(command));
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
                let saved = self.saved.lock().unwrap();
                let patterns: Vec<serde_json::Value> = saved
                    .patterns
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "name": p.name,
                            "look": p.look,
                            // Enough for the UI to describe it and draw a swatch.
                            "layers": p.layers.iter().map(|l| serde_json::json!({
                                "effect": l.effect,
                                "name": l.name,
                                "params": l.params,
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                serde_json::json!({
                    "available": true,
                    "current": current,
                    "folder": library.shows_dir().display().to_string(),
                    "shows": shows,
                    "patterns": patterns,
                    "palettes": saved.palettes,
                })
            }
            None => serde_json::json!({
                "available": false,
                "current": current,
                "folder": null,
                "shows": [],
                "patterns": [],
                "palettes": [],
            }),
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
        // Undo does not reach back into another show.
        self.history.lock().unwrap().clear();
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
    /// Takes back the last change (`back`), or puts it back. What is being
    /// played stays as it is: brightness, blackout, faders, speed and this
    /// computer's settings are not part of what undo moves.
    fn step_history(&self, back: bool) -> Result<Option<String>, String> {
        {
            let mut show = self.show.lock().unwrap();
            let mut history = self.history.lock().unwrap();
            let Some(mut other) = history.step(back, show.clone()) else {
                return Err(if back {
                    "there is nothing to undo".into()
                } else {
                    "there is nothing to redo".into()
                });
            };
            other.master = show.master.clone();
            other.output = show.output.clone();
            other.audio = show.audio.clone();
            other.live.faders = show.live.faders;
            other.live.speed = show.live.speed;
            other.repair();
            *show = other;
        }
        // Nothing held or waiting is sure to exist in the other state.
        self.live.lock().unwrap().let_go();
        self.bump_live();
        self.bump();
        if self.show.lock().unwrap().name != self.saved_name() {
            self.rename_file();
        }
        Ok(None)
    }

    /// The name the open show's file was given, to tell when undo has
    /// changed the show's name.
    fn saved_name(&self) -> String {
        self.save_path
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
            .unwrap_or_default()
    }

    /// One message from a MIDI controller. While something is being
    /// learnt, the message is tied to it; otherwise a note plays its pad
    /// and a knob is noted for the next frame.
    pub fn midi_message(self: &Arc<Self>, bytes: &[u8]) {
        let Some(event) = midi::parse(bytes) else {
            return;
        };
        let learning = {
            let mut status = self.midi.lock().unwrap();
            status.last = Some(match event {
                MidiEvent::Note(note, down) => format!(
                    "{} {} on channel {}",
                    midi::note_name(note.note),
                    if down { "down" } else { "up" },
                    note.channel + 1
                ),
                MidiEvent::Control { channel, cc, value } => {
                    format!("knob {cc} on channel {} at {}%", channel + 1, (value * 100.0).round())
                }
            });
            status.learning
        };
        match (event, learning) {
            (MidiEvent::Note(note, true), Some(LearnTarget::Pad { binding })) => {
                self.midi.lock().unwrap().learning = None;
                let _ = self.execute(&Command::SetPadMidi {
                    id: binding,
                    midi: Some(note),
                });
            }
            (MidiEvent::Control { channel, cc, .. }, Some(LearnTarget::Knob { target })) => {
                self.midi.lock().unwrap().learning = None;
                let _ = self.execute(&Command::SetKnob { target, channel, cc });
            }
            // Something else is being learnt: this is not for playing.
            (_, Some(_)) => {}
            (MidiEvent::Note(note, down), None) => {
                // The pad with this note on the page that is showing. A note
                // let go still reaches a pad whose page has been left.
                let pad = {
                    let show = self.show.lock().unwrap();
                    let page = show.live.page;
                    let with_note = |b: &&darkrgb_core::Binding| b.midi == Some(note);
                    show.bindings
                        .iter()
                        .filter(with_note)
                        .find(|b| b.showing(page))
                        .or_else(|| show.bindings.iter().filter(with_note).find(|_| !down))
                        .map(|b| b.id)
                };
                if let Some(pad) = pad {
                    let _ = self.trigger(pad, down);
                }
            }
            (MidiEvent::Control { channel, cc, value }, None) => {
                let target = self
                    .show
                    .lock()
                    .unwrap()
                    .live
                    .knobs
                    .iter()
                    .find(|k| (k.channel, k.cc) == (channel, cc))
                    .map(|k| k.target);
                if let Some(target) = target {
                    let mut moves = self.knob_moves.lock().unwrap();
                    moves.retain(|(t, _)| *t != target);
                    moves.push((target, value));
                }
            }
        }
    }

    /// Applies where the knobs were last turned to. Called once a frame.
    pub fn apply_knobs(self: &Arc<Self>) {
        let moves = std::mem::take(&mut *self.knob_moves.lock().unwrap());
        for (target, value) in moves {
            let command = match target {
                KnobTarget::Brightness => Command::SetMaster {
                    brightness: Some(value),
                    blackout: None,
                    limit: None,
                },
                KnobTarget::Fader { n } => Command::SetFader { n, value },
                KnobTarget::Speed => Command::SetLive {
                    launch: None,
                    fade: None,
                    speed: Some(midi::doubling(value)),
                    auto: None,
                    every_bars: None,
                    shuffle: None,
                    on_drops: None,
                },
                KnobTarget::Sensitivity => Command::SetAudio {
                    source: None,
                    sensitivity: Some(midi::doubling(value)),
                    delay_ms: None,
                    dynamics: None,
                },
            };
            let _ = self.execute(&command);
        }
    }

    /// Changes what is kept, writes it to disk and tells the clients.
    /// Nothing is changed if `change` refuses or the file cannot be written.
    fn change_saved(&self, change: impl FnOnce(&mut Saved) -> Result<(), String>) -> Result<Option<String>, String> {
        let library = self.library()?;
        let mut saved = self.saved.lock().unwrap();
        let mut next = saved.clone();
        change(&mut next)?;
        library
            .store_saved(&next)
            .map_err(|e| format!("could not write the patterns file: {e}"))?;
        *saved = next;
        drop(saved);
        self.library_rev.send_modify(|r| *r += 1);
        Ok(None)
    }

    /// Keeps one effect (`layer`), or the whole look that is playing, to
    /// use in any show. Lights are not kept, only kinds of light, because
    /// another show has other lights.
    fn save_pattern(&self, layer: Option<u32>, name: Option<&str>) -> Result<Option<String>, String> {
        let (layers, fallback) = {
            let show = self.show.lock().unwrap();
            let portable = |layer: &darkrgb_core::Layer| {
                let mut kept = layer.clone();
                // Playing on strips only, or matrices only, is kept as that rule.
                if kept.only == Only::Any && !kept.targets.is_empty() {
                    let hit: Vec<_> = show.fixtures.iter().filter(|f| Show::layer_hits(layer, f)).collect();
                    if !hit.is_empty() && hit.iter().all(|f| Only::Strips.allows(&f.shape)) {
                        kept.only = Only::Strips;
                    } else if !hit.is_empty() && hit.iter().all(|f| Only::Matrices.allows(&f.shape)) {
                        kept.only = Only::Matrices;
                    }
                }
                kept.targets.clear();
                kept.group = None;
                kept.id = 0;
                kept
            };
            match layer {
                Some(id) => {
                    let layer = show
                        .layers
                        .iter()
                        .find(|l| l.id == id)
                        .ok_or("that effect no longer exists")?;
                    let effect =
                        darkrgb_core::effects::find(&layer.effect).map_or(layer.effect.as_str(), |d| d.info.name);
                    let mut kept = portable(layer);
                    kept.enabled = true;
                    (vec![kept], layer.name.clone().unwrap_or_else(|| effect.to_string()))
                }
                None => {
                    let (layers, _) = show.scene_part();
                    if layers.is_empty() {
                        return Err("there are no effects to keep".into());
                    }
                    (layers.iter().map(portable).collect(), "Look".to_string())
                }
            }
        };
        let look = layer.is_none();
        let name = commands::name(name.filter(|n| !n.trim().is_empty()).unwrap_or(&fallback), "The name")?;
        let kept = name.clone();
        self.change_saved(|saved| {
            // The same name again, for the same kind of thing, replaces it.
            if let Some(existing) = saved.patterns.iter_mut().find(|p| p.name == name && p.look == look) {
                existing.layers = layers;
                return Ok(());
            }
            if saved.patterns.len() >= MAX_PATTERNS {
                return Err(format!("at most {MAX_PATTERNS} patterns can be kept"));
            }
            let id = saved.alloc_id();
            saved.patterns.push(Pattern { id, name, look, layers });
            Ok(())
        })?;
        Ok(Some(if look {
            format!("Kept the look \"{kept}\". In any show, it is offered when you add an effect.")
        } else {
            format!("Kept as \"{kept}\". In any show, it is offered when you add an effect.")
        }))
    }

    /// Uses something kept: see `Command::UsePattern`.
    fn use_pattern(self: &Arc<Self>, id: u32, group: Option<u32>) -> Result<Option<String>, String> {
        self.library()?;
        let pattern = self
            .saved
            .lock()
            .unwrap()
            .patterns
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or("that pattern is no longer there")?;
        self.execute(&Command::Insert {
            layers: pattern.layers,
            group: if pattern.look { None } else { group },
            scene: pattern.look.then_some(pattern.name),
        })
    }

    /// Remembers whether other devices may connect. The server listens
    /// where it was told to when it started, so this is for next time.
    fn set_remote(&self, enabled: bool) -> Result<Option<String>, String> {
        let library = self.library()?;
        library
            .set_remote(enabled)
            .map_err(|e| format!("could not save that choice: {e}"))?;
        let mut remote = self.remote.lock().unwrap();
        remote.wanted = enabled;
        Ok((remote.on != enabled).then(|| {
            if enabled {
                format!("Other devices can connect the next time {APP_NAME} starts.")
            } else {
                format!("Other devices are shut out the next time {APP_NAME} starts.")
            }
        }))
    }

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
            Action::Layer { .. }
            | Action::LayerGroup { .. }
            | Action::LightGroup { .. }
            | Action::Blackout
            | Action::Colours { .. } => {
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
            // The same pad again gives the effects their own colours back.
            Action::Colours { id } => show.live.swatches.iter().any(|s| s.id == id).then(|| {
                show.live.colours = if show.live.colours == Some(id) { None } else { Some(id) };
            }),
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
    fn new_show(&self, name: Option<&str>, keep_lights: bool, kind: Option<Kind>) -> Result<Option<String>, String> {
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
        if let Some(kind) = kind {
            commands::set_up(&mut next, kind)?;
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
        self.history.lock().unwrap().clear();
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
        let before = show.clone();
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
        self.history.lock().unwrap().remember_other(before);
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
                kind: None,
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
                kind: None,
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
            kind: None,
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

        // A show started for a dance floor comes set up for one, and the
        // lights can come along.
        run(Command::NewShow {
            name: Some("Friday night".into()),
            keep_lights: true,
            kind: Some(Kind::Club),
        })
        .unwrap();
        {
            let show = shared.show.lock().unwrap();
            assert_eq!((show.kind, show.devices.len()), (Kind::Club, 1));
            assert!(!show.flash.on() && show.flash.decided, "strobes are expected there");
            assert_eq!(show.scenes.len(), Kind::Club.looks().len());
            assert!(show.live.auto.enabled);
            assert!(show.bindings.iter().any(|b| b.action == Action::Blackout));
            assert_eq!(show.output.frame_rate, 30, "the computer's settings still come along");
        }
        run(Command::OpenShow {
            file: "Main.json".into(),
        })
        .unwrap();
        delete("Friday night.json").unwrap();

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
    fn effects_looks_and_palettes_can_be_kept_and_used_in_another_show() {
        use darkrgb_core::params::ParamValue;
        use darkrgb_core::{Link, Source};
        let (shared, library) = with_library("patterns");
        let run = |command: Command| shared.execute(&command);
        // A strip and a matrix, and an effect made the user's own: its
        // name, its colours, a slider that follows the bass, and only on the strip.
        let (layer, strip) = {
            let mut show = shared.show.lock().unwrap();
            show.add_device(Device::wled("strip", "127.0.0.1", 4048, 10), None);
            show.add_device(Device::wled("matrix", "127.0.0.2", 4048, 16), Some((4, 4)));
            (show.layers[0].id, show.fixtures[0].id)
        };
        run(Command::SetParam {
            layer,
            param: "palette".into(),
            value: ParamValue::Text("#ff0000,#00ff00|loop".into()),
        })
        .unwrap();
        run(Command::UpdateLayer {
            id: layer,
            effect: None,
            opacity: Some(0.5),
            blend: None,
            enabled: None,
            targets: Some(vec![strip]),
            each: None,
            only: None,
            angle: None,
            spin: None,
            turns: None,
        })
        .unwrap();
        run(Command::SetLink {
            layer,
            param: "speed".into(),
            source: Source::Bass,
            amount: 0.3,
        })
        .unwrap();
        run(Command::RenameLayer {
            id: layer,
            name: "Mine".into(),
        })
        .unwrap();

        let note = run(Command::SavePattern { layer, name: None }).unwrap().unwrap();
        assert!(note.contains("Mine"), "{note}");
        // The whole look, of two effects.
        run(Command::AddLayer {
            effect: "sparkle".into(),
            group: None,
        })
        .unwrap();
        run(Command::SaveLook { name: "Evening".into() }).unwrap();
        // A palette, tidied as it is kept.
        run(Command::SavePalette {
            name: "Flag".into(),
            value: "#FF0000,#ffffff".into(),
        })
        .unwrap();
        assert!(
            run(Command::SavePalette {
                name: "Bad".into(),
                value: "rainbow".into()
            })
            .is_err()
        );

        let status = shared.library_status();
        let patterns = status["patterns"].as_array().unwrap();
        assert_eq!(patterns.len(), 2);
        assert_eq!(
            (&patterns[0]["name"], &patterns[0]["look"]),
            (&"Mine".into(), &false.into())
        );
        assert_eq!(patterns[1]["layers"].as_array().unwrap().len(), 2);
        assert_eq!(status["palettes"][0]["value"], "#ff0000,#ffffff");
        // It is on disk: reading the file gives what is in memory.
        assert_eq!(library.load_saved(), *shared.saved.lock().unwrap());
        let (effect_id, look_id) = {
            let saved = shared.saved.lock().unwrap();
            (saved.patterns[0].id, saved.patterns[1].id)
        };
        // Keeping under the same name again replaces, and does not add.
        run(Command::SavePattern { layer, name: None }).unwrap();
        assert_eq!(shared.saved.lock().unwrap().patterns.len(), 2);

        // Another show, with other lights.
        run(Command::NewShow {
            name: Some("Other".into()),
            keep_lights: false,
            kind: None,
        })
        .unwrap();
        shared
            .show
            .lock()
            .unwrap()
            .add_device(Device::wled("other strip", "127.0.0.3", 4048, 20), None);
        run(Command::UsePattern {
            id: effect_id,
            group: None,
        })
        .unwrap();
        {
            let show = shared.show.lock().unwrap();
            assert_eq!(show.layers.len(), 2);
            let used = show.layers.last().unwrap();
            assert_eq!(used.name.as_deref(), Some("Mine"));
            assert_eq!(used.opacity, 0.5);
            assert_eq!(
                used.params["palette"],
                ParamValue::Text("#ff0000,#00ff00|loop".into()),
                "the colours came along inside the setting"
            );
            assert_eq!(
                used.links,
                vec![Link {
                    param: "speed".into(),
                    source: Source::Bass,
                    amount: 0.3
                }]
            );
            assert!(used.targets.is_empty(), "no lights are named");
            assert_eq!(used.only, Only::Strips, "but it still keeps to strips");
            assert!(show.validate().is_ok());
        }
        // A look becomes a scene, and is switched to. Twice gives two scenes.
        run(Command::UsePattern {
            id: look_id,
            group: None,
        })
        .unwrap();
        run(Command::UsePattern {
            id: look_id,
            group: None,
        })
        .unwrap();
        {
            let show = shared.show.lock().unwrap();
            let names: Vec<&str> = show.scenes.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, ["Evening", "Evening 2"]);
            assert_eq!(show.current_scene(), Some(show.scenes[1].id));
            assert_eq!(show.layers.len(), 2);
            let mut mended = show.clone();
            mended.repair();
            assert_eq!(mended, *show);
        }

        // Renamed and deleted.
        run(Command::RenamePattern {
            id: effect_id,
            name: "Renamed".into(),
        })
        .unwrap();
        assert_eq!(shared.saved.lock().unwrap().patterns[0].name, "Renamed");
        run(Command::DeletePattern { id: look_id }).unwrap();
        assert!(run(Command::DeletePattern { id: look_id }).is_err());
        assert!(
            run(Command::UsePattern {
                id: look_id,
                group: None
            })
            .is_err()
        );
        let palette = shared.saved.lock().unwrap().palettes[0].id;
        run(Command::DeletePalette { id: palette }).unwrap();
        assert_eq!(library.load_saved(), *shared.saved.lock().unwrap());

        // A patterns file edited by hand cannot put nonsense into a show.
        {
            let mut saved = shared.saved.lock().unwrap();
            let pattern = &mut saved.patterns[0];
            pattern.layers[0].params.insert("speed".into(), ParamValue::Number(1e9));
            pattern.layers[0]
                .params
                .insert("made_up".into(), ParamValue::Bool(true));
            pattern.layers[0].opacity = f32::NAN;
            pattern.layers[0].targets = vec![4242];
        }
        run(Command::UsePattern {
            id: effect_id,
            group: None,
        })
        .unwrap();
        {
            let show = shared.show.lock().unwrap();
            let used = show.layers.last().unwrap();
            assert_eq!(used.opacity, 1.0);
            assert!(!used.params.contains_key("made_up"));
            assert!(matches!(used.params["speed"], ParamValue::Number(v) if v <= 2.0));
            assert!(used.targets.is_empty());
        }
        shared.saved.lock().unwrap().patterns[0].layers[0].effect = "from_the_future".into();
        let error = run(Command::UsePattern {
            id: effect_id,
            group: None,
        })
        .unwrap_err();
        assert!(error.contains("from_the_future"), "{error}");
    }

    #[test]
    fn a_midi_controller_can_be_taught_and_then_plays_pads_and_moves_sliders() {
        let shared = Shared::new(Show::default(), None, None);
        let (layer, pad) = {
            let mut show = shared.show.lock().unwrap();
            let layer = show.layers[0].id;
            (
                layer,
                show.add_pad(None, "", Action::Layer { id: layer }, KeyMode::Hold),
            )
        };
        let other = shared
            .show
            .lock()
            .unwrap()
            .add_pad(None, "", Action::Blackout, KeyMode::Toggle);
        let learn = |target| shared.execute(&Command::MidiLearn { target }).unwrap();
        let note_of = |id: u32| {
            shared
                .show
                .lock()
                .unwrap()
                .bindings
                .iter()
                .find(|b| b.id == id)
                .unwrap()
                .midi
        };

        // Untaught, a controller does nothing but show that it is heard.
        shared.midi_message(&[0x90, 36, 100]);
        assert!(!shared.live.lock().unwrap().is_held(pad));
        assert_eq!(
            shared.midi.lock().unwrap().last.as_deref(),
            Some("C2 down on channel 1")
        );

        // Taught: the next pad pressed is tied to the pad. A knob moved meanwhile is not.
        learn(Some(LearnTarget::Pad { binding: pad }));
        shared.midi_message(&[0xB0, 7, 64]);
        assert!(shared.midi.lock().unwrap().learning.is_some());
        shared.midi_message(&[0x90, 36, 100]);
        assert_eq!(note_of(pad), Some(darkrgb_core::MidiNote { channel: 0, note: 36 }));
        assert!(shared.midi.lock().unwrap().learning.is_none());
        assert!(
            !shared.live.lock().unwrap().is_held(pad),
            "the press that taught it does not play it"
        );

        // Now it plays: held while down, let go on note-off or a note-on with no velocity.
        shared.midi_message(&[0x90, 36, 100]);
        assert!(shared.live.lock().unwrap().is_held(pad));
        shared.midi_message(&[0x80, 36, 0]);
        assert!(!shared.live.lock().unwrap().is_held(pad));
        shared.midi_message(&[0x90, 36, 100]);
        shared.midi_message(&[0x90, 36, 0]);
        assert!(!shared.live.lock().unwrap().is_held(pad));
        // Another channel is another pad.
        shared.midi_message(&[0x91, 36, 100]);
        assert!(!shared.live.lock().unwrap().is_held(pad));

        // The same note taught to another pad moves there.
        learn(Some(LearnTarget::Pad { binding: other }));
        shared.midi_message(&[0x90, 36, 100]);
        assert_eq!((note_of(pad), note_of(other).map(|n| n.note)), (None, Some(36)));
        shared.midi_message(&[0x90, 36, 100]);
        assert!(
            shared.show.lock().unwrap().master.blackout,
            "the note now switches blackout"
        );
        shared.midi_message(&[0x90, 36, 0]);
        shared.midi_message(&[0x90, 36, 100]);
        assert!(!shared.show.lock().unwrap().master.blackout);

        // Knobs: taught the same way, applied once a frame, the latest position winning.
        learn(Some(LearnTarget::Knob {
            target: KnobTarget::Brightness,
        }));
        shared.midi_message(&[0xB0, 7, 0]);
        learn(Some(LearnTarget::Knob {
            target: KnobTarget::Fader { n: 2 },
        }));
        shared.midi_message(&[0xB0, 8, 0]);
        learn(Some(LearnTarget::Knob {
            target: KnobTarget::Speed,
        }));
        shared.midi_message(&[0xB0, 9, 0]);
        assert_eq!(shared.show.lock().unwrap().live.knobs.len(), 3);
        for value in [10, 50, 127, 64] {
            shared.midi_message(&[0xB0, 7, value]);
        }
        shared.midi_message(&[0xB0, 8, 127]);
        shared.midi_message(&[0xB0, 9, 127]);
        assert_eq!(
            shared.show.lock().unwrap().master.brightness,
            1.0,
            "nothing moves until the frame"
        );
        shared.apply_knobs();
        {
            let show = shared.show.lock().unwrap();
            assert!((show.master.brightness - 64.0 / 127.0).abs() < 1e-6);
            assert_eq!(show.live.faders[1], 1.0);
            assert_eq!(show.live.speed, 4.0, "full turn is four times the speed");
        }
        // Cancelling, and pads that are gone.
        learn(Some(LearnTarget::Pad { binding: pad }));
        learn(None);
        shared.midi_message(&[0x90, 40, 100]);
        assert_eq!(note_of(pad), None);
        assert!(
            shared
                .execute(&Command::MidiLearn {
                    target: Some(LearnTarget::Pad { binding: 9999 })
                })
                .is_err()
        );
        // Nonsense from a controller is ignored.
        shared.midi_message(&[]);
        shared.midi_message(&[0xF8]);
        shared.midi_message(&[0x90, 200, 5]);
        let _ = layer;
    }

    #[test]
    fn changes_can_be_taken_back_and_put_back() {
        let shared = Shared::new(Show::default(), None, None);
        let run = |command: Command| shared.execute(&command);
        let layers = || shared.show.lock().unwrap().layers.len();
        let add = || {
            run(Command::AddLayer {
                effect: "solid".into(),
                group: None,
            })
            .unwrap()
        };
        assert!(run(Command::Undo).is_err(), "nothing to undo yet");
        assert_eq!(shared.history.lock().unwrap().can(), (false, false));

        add();
        // Different kinds of change are separate steps.
        run(Command::RenameShow { name: "Mine".into() }).unwrap();
        assert_eq!((layers(), shared.history.lock().unwrap().can()), (2, (true, false)));
        run(Command::Undo).unwrap();
        assert_eq!(shared.show.lock().unwrap().name, "Untitled show");
        assert_eq!(layers(), 2);
        run(Command::Undo).unwrap();
        assert_eq!(layers(), 1);
        assert!(run(Command::Undo).is_err());
        // And back again.
        run(Command::Redo).unwrap();
        assert_eq!(layers(), 2);
        run(Command::Redo).unwrap();
        assert_eq!(shared.show.lock().unwrap().name, "Mine");
        assert!(run(Command::Redo).is_err());

        // A run of the same kind of change, such as a slider being dragged, is one step.
        let layer = shared.show.lock().unwrap().layers[0].id;
        for speed in [0.3, 0.6, 0.9, 1.2] {
            run(Command::SetParam {
                layer,
                param: "speed".into(),
                value: darkrgb_core::params::ParamValue::Number(speed),
            })
            .unwrap();
        }
        run(Command::Undo).unwrap();
        assert!(
            !shared.show.lock().unwrap().layers[0].params.contains_key("speed"),
            "the whole drag is taken back at once"
        );
        // A new change after an undo forgets what could have been redone.
        add();
        assert!(run(Command::Redo).is_err());

        // Playing the lights is not undone, and does not fill the memory.
        let steps = shared.history.lock().unwrap().undo.len();
        run(Command::SetMaster {
            brightness: Some(0.4),
            blackout: Some(true),
            limit: None,
        })
        .unwrap();
        run(Command::SetFader { n: 1, value: 0.7 }).unwrap();
        assert_eq!(shared.history.lock().unwrap().undo.len(), steps);
        run(Command::Undo).unwrap();
        {
            let show = shared.show.lock().unwrap();
            assert_eq!((show.master.brightness, show.master.blackout), (0.4, true));
            assert_eq!(show.live.faders[0], 0.7);
        }
        // A change that did nothing is not a step.
        let steps = shared.history.lock().unwrap().undo.len();
        run(Command::RenameShow { name: "Mine".into() }).unwrap();
        assert_eq!(shared.history.lock().unwrap().undo.len(), steps);
        // Another show starts with a clean memory.
        shared.replace_show(Show::default());
        assert_eq!(shared.history.lock().unwrap().can(), (false, false));
    }

    #[test]
    fn without_a_library_the_library_commands_say_so() {
        let shared = Shared::new(Show::default(), None, None);
        let error = shared
            .execute(&Command::OpenShow { file: "A.json".into() })
            .unwrap_err();
        assert!(error.contains("library"), "{error}");
        assert_eq!(shared.library_status()["available"], false);
        let layer = shared.show.lock().unwrap().layers[0].id;
        assert!(shared.execute(&Command::SavePattern { layer, name: None }).is_err());
        assert!(shared.execute(&Command::UsePattern { id: 1, group: None }).is_err());
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
