//! The show file: everything that is saved and restored.

use serde::{Deserialize, Serialize};

use crate::audio::{AudioFeatures, Band};
use crate::effects;
use crate::layout::{Device, Fixture, MAX_PIXELS, Shape};
use crate::params::Params;

pub const SHOW_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    /// Replaces what is underneath, mixed by opacity.
    #[default]
    Normal,
    /// Adds light to what is underneath.
    Add,
    /// Keeps the brighter of the two.
    Lighten,
    /// Darkens what is underneath; black hides it.
    Multiply,
}

/// What a layer hears, and how strongly it responds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    /// The part of the spectrum the layer listens to.
    #[serde(default)]
    pub band: Band,
    /// Multiplier on what it hears, 0.25..4.
    #[serde(default = "one")]
    pub gain: f32,
    /// How much the layer's brightness follows the sound, 0..1. Works on
    /// any effect, including ones that ignore sound themselves.
    #[serde(default)]
    pub pulse: f32,
    /// The layer ignores the music and reacts only when fired by hand.
    #[serde(default)]
    pub manual: bool,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            band: Band::All,
            gain: 1.0,
            pulse: 0.0,
            manual: false,
        }
    }
}

/// A folder of effects. Its layers sit next to each other in the stack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerGroup {
    pub id: u32,
    pub name: String,
    /// Off hides every effect inside, whatever their own switches say.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Multiplies the strength of every effect inside, 0..1.
    #[serde(default = "one")]
    pub opacity: f32,
    /// Pinned folders stay on top of the stack and are left alone by scene
    /// changes: they are not saved in scenes and not replaced by them. It
    /// is where things played by hand live, such as a flash.
    #[serde(default)]
    pub pinned: bool,
}

/// A named set of lights, such as "Ceiling". An effect can play on a
/// whole group, and the group can be dimmed or switched off as one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightGroup {
    pub id: u32,
    pub name: String,
    /// Off makes the group's lights dark.
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "one")]
    pub brightness: f32,
}

/// The kind of light an effect can be kept to, whichever lights the show
/// has now or gets later. A spectrum wants a matrix; a chase wants a strip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Only {
    #[default]
    Any,
    Strips,
    Matrices,
}

impl Only {
    pub fn allows(self, shape: &Shape) -> bool {
        match self {
            Only::Any => true,
            Only::Strips => matches!(shape, Shape::Strip { .. }),
            Only::Matrices => !matches!(shape, Shape::Strip { .. }),
        }
    }
}

/// Who a show is for. Chosen when the show is started, it decides what the
/// show begins with: its starter scenes, its pads, how it treats flashing.
/// Nothing is locked by it afterwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A desk or a room, mostly for whoever lives there.
    Room,
    /// A house, a garden or a window: a show for people passing by.
    Display,
    /// A dance floor or a stage, with someone playing the lights.
    Club,
    /// Nothing assumed.
    #[default]
    Custom,
}

/// Something a setting can follow, so that it moves by itself. Every
/// source runs from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// How loud the part of the sound is that the effect hears.
    Level,
    Bass,
    Mid,
    Treble,
    /// Jumps on every beat and falls away.
    Beat,
    /// Jumps on every hit in the part of the sound the effect hears.
    Hit,
    /// How big this part of the song is: low in a breakdown, full in a drop.
    Energy,
    /// A slow swing out and back, once every so many beats.
    Wave {
        beats: f32,
    },
    /// One of the faders in the Live view, numbered from 1.
    Fader {
        n: u8,
    },
}

/// How many faders the Live view has for links to follow.
pub const FADERS: usize = 4;
/// Most links one effect can have.
pub const MAX_LINKS: usize = 8;
/// The name a link uses for the layer's own strength, which is not one of
/// the effect's settings.
pub const LINK_STRENGTH: &str = "opacity";
/// The name a link uses for the angle a layer's pattern is turned to.
pub const LINK_ANGLE: &str = "angle";

impl Source {
    /// Where the source is now, 0..1. `heard` is the sound as the layer
    /// hears it, `whole` the sound as it is.
    pub fn value(self, heard: &AudioFeatures, whole: &AudioFeatures, faders: &[f32; FADERS], time: f64) -> f32 {
        let v = match self {
            Source::Level => heard.level,
            Source::Bass => whole.bass,
            Source::Mid => whole.mid,
            Source::Treble => whole.treble,
            Source::Beat => heard.beat,
            Source::Hit => heard.onset,
            // With nothing playing there is no song to be big.
            Source::Energy => {
                if whole.active {
                    whole.intensity
                } else {
                    0.0
                }
            }
            Source::Wave { beats } => {
                let period = if beats.is_finite() {
                    beats.clamp(0.25, 64.0)
                } else {
                    8.0
                } as f64;
                // In time with the music when there is a tempo; otherwise
                // as if it were playing at 120.
                let turns = if whole.bpm > 0.0 {
                    (whole.beat_count as f64 + whole.beat_phase.clamp(0.0, 1.0) as f64) / period
                } else {
                    time * 2.0 / period
                };
                (0.5 - 0.5 * (turns * std::f64::consts::TAU).cos()) as f32
            }
            Source::Fader { n } => faders.get((n as usize).wrapping_sub(1)).copied().unwrap_or(0.0),
        };
        if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
    }
}

/// Ties one setting of an effect to a source: the setting moves from
/// where its slider is, by `amount` of its range when the source is full.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// The setting's id, or [`LINK_STRENGTH`].
    pub param: String,
    pub source: Source,
    /// -1..1. Negative moves the setting down as the source rises.
    pub amount: f32,
}

/// One effect running on some or all fixtures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: u32,
    /// A label the user gave it. Without one, the effect's name is shown.
    #[serde(default)]
    pub name: Option<String>,
    /// Effect id from the registry.
    pub effect: String,
    #[serde(default)]
    pub sound: Sound,
    #[serde(default)]
    pub params: Params,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub blend: Blend,
    /// What this layer draws on: ids of fixtures and of light groups.
    /// Empty means all fixtures.
    #[serde(default)]
    pub targets: Vec<u32>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The folder this layer is filed in, if any.
    #[serde(default)]
    pub group: Option<u32>,
    /// Draw the effect on each light by itself, in the light's own frame,
    /// instead of once across all of them. A fire then burns up every
    /// matrix, however the lights are laid out.
    #[serde(default)]
    pub each: bool,
    /// Keep to one kind of light, among whatever `targets` allows.
    #[serde(default)]
    pub only: Only,
    /// Settings that move by themselves, following the music or a fader.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    /// How far the pattern is turned, in degrees clockwise. A sweep that
    /// crosses from left to right at 0 comes down from the top at 90.
    #[serde(default, skip_serializing_if = "is_nought")]
    pub angle: f32,
    /// How fast the pattern keeps turning, in degrees a second.
    #[serde(default, skip_serializing_if = "is_nought")]
    pub spin: f32,
    /// Drawn on each light by itself, the pattern normally stays the right
    /// way up in the room however a matrix is turned on the stage (a
    /// square hung as a diamond still shows its words level). With this
    /// on, the pattern turns with the light instead.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub turns: bool,
}

fn is_nought(v: &f32) -> bool {
    *v == 0.0
}

/// The parts of a song a scene suits, for autopilot to choose by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    #[default]
    Any,
    /// For breakdowns and quiet stretches.
    Calm,
    /// For drops and the loud parts.
    Big,
}

/// A saved set of layers that can be recalled in one click.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: u32,
    pub name: String,
    pub layers: Vec<Layer>,
    /// The folders those layers were filed in.
    #[serde(default)]
    pub groups: Vec<LayerGroup>,
    /// Whether autopilot may switch to this scene.
    #[serde(default = "yes")]
    pub auto: bool,
    /// The parts of a song autopilot uses it for.
    #[serde(default)]
    pub mood: Mood,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Master {
    pub brightness: f32,
    pub blackout: bool,
    /// What the brightness slider's 100% means, 0.05..1. A ceiling for
    /// power supplies and eyes that the slider cannot go past.
    #[serde(default = "one")]
    pub limit: f32,
}

impl Default for Master {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            blackout: false,
            limit: 1.0,
        }
    }
}

/// What a key does when pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Switches to a scene.
    Scene {
        id: u32,
    },
    /// Switches an effect on or off.
    Layer {
        id: u32,
    },
    /// Switches a folder of effects on or off.
    LayerGroup {
        id: u32,
    },
    /// Switches a group of lights on or off.
    LightGroup {
        id: u32,
    },
    Blackout,
    /// Tapped in time with the music, sets the tempo by hand.
    TapTempo,
    /// Fires a beat by hand, so effects that wait for one go off now.
    Hit,
    /// Switches to the scene after the one that is playing.
    NextScene,
    /// Switches to the scene before the one that is playing.
    PrevScene,
    /// Stops the picture where it is.
    Freeze,
    /// Gives everything that is playing the colours of a swatch.
    Colours {
        id: u32,
    },
}

impl Action {
    /// Actions that have a state to flip.
    pub fn is_switch(&self) -> bool {
        matches!(
            self,
            Action::Layer { .. }
                | Action::LayerGroup { .. }
                | Action::LightGroup { .. }
                | Action::Blackout
                | Action::Freeze
                | Action::Colours { .. }
        )
    }

    /// Whether "only while the key is down" means something: for anything
    /// that switches, and for a scene, which is shown while held.
    pub fn can_hold(&self) -> bool {
        self.is_switch() || matches!(self, Action::Scene { .. })
    }

    /// Whether the thing has a reaction that a press can set off.
    pub fn can_fire(&self) -> bool {
        matches!(self, Action::Layer { .. } | Action::LayerGroup { .. })
    }

    /// The nearest behaviour to `mode` that this action supports.
    pub fn allowed(&self, mode: KeyMode) -> KeyMode {
        match mode {
            KeyMode::Hold if self.can_hold() => KeyMode::Hold,
            KeyMode::Fire if self.can_fire() => KeyMode::Fire,
            _ => KeyMode::Toggle,
        }
    }
}

/// What a press of a pad or its key does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyMode {
    /// Each press does it: flips a switch, changes a scene.
    #[default]
    Toggle,
    /// Flipped, or shown, only while the key is down.
    Hold,
    /// Sets off the effect's reaction, as if a beat had hit.
    Fire,
}

/// A pad in the live view, tied to something in the show, usually with a
/// key on the keyboard. The order of the list is the order of the pads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub id: u32,
    /// Where the key sits on the keyboard, as browsers name it: `KeyA`,
    /// `Digit1`, `Space`. The same key on any keyboard layout. A pad
    /// without a key is pressed by mouse or touch.
    #[serde(default)]
    pub key: Option<String>,
    /// What is printed on that key on the user's keyboard, for display.
    #[serde(default)]
    pub label: String,
    pub action: Action,
    #[serde(default)]
    pub mode: KeyMode,
    /// The pad or key on a MIDI controller that plays this pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi: Option<MidiNote>,
    /// The page of pads it is on, 1 to [`PAGES`]; 0 is every page. A key
    /// or a controller's pad plays it only while its page is showing, so
    /// the same key can do something else on another page.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub page: u8,
}

/// How many pages of pads there are.
pub const PAGES: u8 = 4;

fn first_page() -> u8 {
    1
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

impl Binding {
    /// Whether two pads can be showing at the same time, so that a key
    /// could not tell them apart.
    pub fn shares_a_page_with(&self, page: u8) -> bool {
        self.page == 0 || page == 0 || self.page == page
    }

    /// Whether the pad is live on the page that is showing.
    pub fn showing(&self, page: u8) -> bool {
        self.page == 0 || self.page == page
    }
}

/// A pad or key on a MIDI controller: its channel (0..15) and note (0..127).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiNote {
    pub channel: u8,
    pub note: u8,
}

/// What a knob or fader on a MIDI controller can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KnobTarget {
    Brightness,
    Speed,
    Sensitivity,
    /// One of the Live view's faders, numbered from 1.
    Fader {
        n: u8,
    },
}

/// Ties a knob or fader on a MIDI controller (a channel and a controller
/// number) to something it moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knob {
    pub channel: u8,
    pub cc: u8,
    pub target: KnobTarget,
}

/// When a scene change takes effect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Launch {
    #[default]
    Now,
    /// On the next beat of the music.
    Beat,
    /// On the next fourth beat.
    Bar,
}

/// Settings for performing live.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveSettings {
    #[serde(default)]
    pub launch: Launch,
    /// Seconds a scene change takes to fade in, 0..10. Zero cuts.
    #[serde(default)]
    pub fade: f32,
    /// How fast everything moves, 0.25..4. One is normal.
    #[serde(default = "one")]
    pub speed: f32,
    #[serde(default)]
    pub auto: Autopilot,
    /// Where the faders of the Live view stand, 0..1. Links follow them.
    #[serde(default)]
    pub faders: [f32; FADERS],
    #[serde(default)]
    pub idle: Idle,
    /// Knobs and faders of a MIDI controller, and what each moves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knobs: Vec<Knob>,
    /// The page of pads that is showing, 1 to [`PAGES`].
    #[serde(default = "first_page")]
    pub page: u8,
    /// Colours that a pad can give to everything that is playing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub swatches: Vec<Swatch>,
    /// The swatch in force, if any: every palette is replaced by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colours: Option<u32>,
}

/// A palette with a pad of its own. While it is in force, every effect
/// that takes its colours from a palette takes them from this one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub id: u32,
    pub name: String,
    /// A palette setting's value: a built-in palette's id, or the user's
    /// own colours written out.
    pub value: String,
}

/// What the lights do when the music stops: after a while they change to
/// a scene chosen for it, and come back when the music does.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Idle {
    /// The scene to wait in. `None` stays on whatever is playing.
    #[serde(default)]
    pub scene: Option<u32>,
    /// Seconds of silence before changing to it, 3..600.
    #[serde(default = "default_idle_after")]
    pub after: f32,
}

fn default_idle_after() -> f32 {
    20.0
}

impl Default for Idle {
    fn default() -> Self {
        Self {
            scene: None,
            after: default_idle_after(),
        }
    }
}

impl Default for LiveSettings {
    fn default() -> Self {
        Self {
            launch: Launch::Now,
            fade: 0.0,
            speed: 1.0,
            auto: Autopilot::default(),
            faders: [0.0; FADERS],
            idle: Idle::default(),
            knobs: Vec::new(),
            page: 1,
            swatches: Vec::new(),
            colours: None,
        }
    }
}

/// Changes scene by itself, in time with the music.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Autopilot {
    #[serde(default)]
    pub enabled: bool,
    /// Bars (of four beats) each scene plays for, 1..64.
    #[serde(default = "default_bars")]
    pub every_bars: u32,
    /// Pick the next scene at random instead of in order.
    #[serde(default)]
    pub shuffle: bool,
    /// Also change the moment the song moves to a new section.
    #[serde(default = "yes")]
    pub on_drops: bool,
}

fn default_bars() -> u32 {
    8
}

impl Default for Autopilot {
    fn default() -> Self {
        Self {
            enabled: false,
            every_bars: default_bars(),
            shuffle: false,
            on_drops: true,
        }
    }
}

/// Frame rates the engine can run at. 120 only helps short strips: the
/// common LED types take about 30 microseconds per LED to refresh.
pub const FRAME_RATES: [u32; 4] = [30, 40, 60, 120];

/// Flashes a second allowed by default: the accepted threshold for
/// photosensitive epilepsy.
pub const DEFAULT_FLASH_LIMIT: f32 = 3.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputSettings {
    /// Frames rendered and sent each second.
    pub frame_rate: u32,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self { frame_rate: 60 }
    }
}

/// Hours the lights keep, every day: on at one time, off at another. For
/// a display that should not shine all night. Outside the hours the
/// lights are dark and the show itself is left as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    #[serde(default)]
    pub enabled: bool,
    /// When the lights come on, in minutes after midnight.
    #[serde(default = "default_on")]
    pub on: u16,
    /// When they go off. Earlier than `on` means past midnight.
    #[serde(default = "default_off")]
    pub off: u16,
}

fn default_on() -> u16 {
    17 * 60
}

fn default_off() -> u16 {
    23 * 60
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            on: default_on(),
            off: default_off(),
        }
    }
}

impl Schedule {
    /// Whether the lights should be on at this time of day, given in
    /// minutes after midnight. Always, when there is no schedule; never,
    /// when on and off are the same time.
    pub fn lit_at(&self, minute: u16) -> bool {
        if !self.enabled {
            return true;
        }
        let minute = minute % 1440;
        if self.on <= self.off {
            (self.on..self.off).contains(&minute)
        } else {
            // Through midnight.
            minute >= self.on || minute < self.off
        }
    }
}

/// How a show treats fast flashing. It belongs to the show and not to the
/// computer, because the answer depends on who will see the lights: a club
/// night and a display on the front lawn want different things.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlashSettings {
    /// Most flashes of the whole picture allowed in a second. Zero
    /// switches the limit off. See `safety.rs`.
    #[serde(default = "default_flash_limit")]
    pub limit: f32,
    /// Count any change of a tenth of full brightness as a flash, as the
    /// guidance for screens does, instead of only hard flashes.
    #[serde(default)]
    pub strict: bool,
    /// False until whoever makes the show has chosen. Until then the limit
    /// is on, and the UI asks the first time something flashes fast.
    #[serde(default)]
    pub decided: bool,
}

impl FlashSettings {
    /// With the limit switched off, for shows whose makers want strobes.
    pub fn off() -> Self {
        Self {
            limit: 0.0,
            strict: false,
            decided: true,
        }
    }

    pub fn on(&self) -> bool {
        self.limit > 0.0
    }

    /// The share of full brightness that counts as a flash.
    pub fn share(&self) -> f32 {
        if self.strict {
            crate::safety::STRICT
        } else {
            crate::safety::STANDARD
        }
    }

    /// Flashes a second an effect may make by itself and still be let
    /// through whole: a little under the limit, so that a steady strobe
    /// never lands a fourth flash inside one second by a frame's jitter.
    /// Infinite when there is no limit.
    pub fn cap(&self) -> f32 {
        if self.on() { self.limit * 0.9 } else { f32::INFINITY }
    }
}

fn default_flash_limit() -> f32 {
    DEFAULT_FLASH_LIMIT
}

impl Default for FlashSettings {
    fn default() -> Self {
        Self {
            limit: DEFAULT_FLASH_LIMIT,
            strict: false,
            decided: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioSettings {
    /// `system` follows whatever the computer is playing. Anything else is
    /// the id of a specific input device.
    pub source: String,
    /// Multiplier on top of automatic gain, 0.25..4.
    pub sensitivity: f32,
    /// Milliseconds the lights wait before following the sound, 0..1000.
    /// For speakers that play late, such as Bluetooth ones.
    #[serde(default)]
    pub delay_ms: f32,
    /// How much the lights follow the shape of the song, 0..1: quiet
    /// parts dimmer and calmer, big parts at full strength. Zero treats
    /// every part of a song alike.
    #[serde(default = "default_dynamics")]
    pub dynamics: f32,
}

/// A mild amount: enough to feel a breakdown, not enough to go dark in one.
pub const DEFAULT_DYNAMICS: f32 = 0.35;

fn default_dynamics() -> f32 {
    DEFAULT_DYNAMICS
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            source: "system".into(),
            sensitivity: 1.0,
            delay_ms: 0.0,
            dynamics: DEFAULT_DYNAMICS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Show {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub devices: Vec<Device>,
    #[serde(default)]
    pub fixtures: Vec<Fixture>,
    #[serde(default)]
    pub light_groups: Vec<LightGroup>,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub layer_groups: Vec<LayerGroup>,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub master: Master,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub live: LiveSettings,
    #[serde(default)]
    pub output: OutputSettings,
    #[serde(default)]
    pub flash: FlashSettings,
    #[serde(default)]
    pub schedule: Schedule,
    /// Next id to hand out. Ids are never reused.
    #[serde(default = "first_id")]
    pub next_id: u32,
}

fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}
fn first_id() -> u32 {
    1
}

/// Whether an effect has a slider called `param` that a link can move.
/// The layer's strength always can be.
pub fn linkable(effect: &str, param: &str) -> bool {
    param == LINK_STRENGTH
        || param == LINK_ANGLE
        || effects::find(effect).is_some_and(|def| {
            def.info
                .params
                .iter()
                .any(|p| p.id == param && matches!(p.kind, crate::params::ParamKind::Float { .. }))
        })
}

/// Drops links to settings the layer's effect does not have, keeps one per
/// setting, and brings their numbers into range.
pub fn mend_links(layer: &mut Layer) {
    if layer.links.is_empty() {
        return;
    }
    let effect = layer.effect.clone();
    let mut seen: Vec<String> = Vec::new();
    layer.links.retain(|link| {
        let keep = linkable(&effect, &link.param) && !seen.contains(&link.param);
        seen.push(link.param.clone());
        keep
    });
    layer.links.truncate(MAX_LINKS);
    for link in &mut layer.links {
        link.amount = if link.amount.is_finite() {
            link.amount.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        match &mut link.source {
            Source::Wave { beats } => {
                *beats = if beats.is_finite() {
                    beats.clamp(0.25, 64.0)
                } else {
                    8.0
                }
            }
            Source::Fader { n } => *n = (*n).clamp(1, FADERS as u8),
            _ => {}
        }
    }
}

/// See [`Show::tidy_layers`].
fn tidy(layers: &mut Vec<Layer>, groups: &[LayerGroup]) {
    for layer in layers.iter_mut() {
        if layer.group.is_some_and(|g| !groups.iter().any(|x| x.id == g)) {
            layer.group = None;
        }
    }
    let mut ordered: Vec<Layer> = Vec::with_capacity(layers.len());
    let mut rest = std::mem::take(layers);
    while !rest.is_empty() {
        let first = rest.remove(0);
        let group = first.group;
        ordered.push(first);
        if group.is_some() {
            let (same, other): (Vec<Layer>, Vec<Layer>) = rest.into_iter().partition(|l| l.group == group);
            ordered.extend(same);
            rest = other;
        }
    }
    // Pinned folders sit above everything else, in the order they were in.
    let pinned = |l: &Layer| l.group.is_some_and(|g| groups.iter().any(|x| x.id == g && x.pinned));
    let (top, below): (Vec<Layer>, Vec<Layer>) = ordered.into_iter().partition(pinned);
    *layers = below;
    layers.extend(top);
}

impl Default for Show {
    fn default() -> Self {
        let mut show = Show {
            version: SHOW_VERSION,
            name: "Untitled show".into(),
            kind: Kind::Custom,
            devices: Vec::new(),
            fixtures: Vec::new(),
            light_groups: Vec::new(),
            layers: Vec::new(),
            layer_groups: Vec::new(),
            scenes: Vec::new(),
            bindings: Vec::new(),
            master: Master::default(),
            audio: AudioSettings::default(),
            live: LiveSettings::default(),
            output: OutputSettings::default(),
            flash: FlashSettings::default(),
            schedule: Schedule::default(),
            next_id: 1,
        };
        show.add_layer(effects::DEFAULT_EFFECT);
        show
    }
}

impl Show {
    pub fn alloc_id(&mut self) -> u32 {
        // Guard against a hand-edited file whose counter is behind its ids.
        let used = self
            .devices
            .iter()
            .map(|d| d.id)
            .chain(self.fixtures.iter().map(|f| f.id))
            .chain(self.layers.iter().map(|l| l.id))
            .chain(self.scenes.iter().map(|s| s.id))
            .chain(self.scenes.iter().flat_map(|s| s.layers.iter().map(|l| l.id)))
            .chain(self.scenes.iter().flat_map(|s| s.groups.iter().map(|g| g.id)))
            .chain(self.layer_groups.iter().map(|g| g.id))
            .chain(self.light_groups.iter().map(|g| g.id))
            .chain(self.bindings.iter().map(|b| b.id))
            .max()
            .unwrap_or(0);
        let id = self.next_id.max(used + 1);
        self.next_id = id + 1;
        id
    }

    pub fn device(&self, id: u32) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn layer_mut(&mut self, id: u32) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn fixture_mut(&mut self, id: u32) -> Option<&mut Fixture> {
        self.fixtures.iter_mut().find(|f| f.id == id)
    }

    /// Adds a layer on top with the effect's defaults and returns its id.
    /// The first layer replaces; later ones add light, which is what
    /// stacking effects on LEDs usually wants.
    pub fn add_layer(&mut self, effect: &str) -> u32 {
        self.add_layer_in(effect, None)
    }

    /// Adds a layer on top of a folder's layers, or on top of everything
    /// when no folder is given or the folder is empty.
    pub fn add_layer_in(&mut self, effect: &str, group: Option<u32>) -> u32 {
        let id = self.alloc_id();
        let blend = if self.layers.is_empty() {
            Blend::Normal
        } else {
            Blend::Add
        };
        let group = group.filter(|g| self.layer_groups.iter().any(|x| x.id == *g));
        // On top of its folder; otherwise on top of everything that is not
        // pinned, since pinned folders stay above the rest.
        let top = self
            .layers
            .iter()
            .position(|l| self.is_pinned(l))
            .unwrap_or(self.layers.len());
        let pinned_folder = group.is_some_and(|g| self.layer_groups.iter().any(|x| x.id == g && x.pinned));
        let at = group
            .and_then(|g| self.layers.iter().rposition(|l| l.group == Some(g)))
            .map_or(if pinned_folder { self.layers.len() } else { top }, |last| last + 1);
        self.layers.insert(
            at,
            Layer {
                id,
                name: None,
                sound: Sound::default(),
                effect: effect.to_string(),
                params: Params::new(),
                opacity: 1.0,
                blend,
                targets: Vec::new(),
                enabled: true,
                group,
                each: false,
                only: Only::Any,
                angle: 0.0,
                spin: 0.0,
                turns: false,
                links: Vec::new(),
            },
        );
        id
    }

    /// Adds an empty folder for effects and returns its id.
    pub fn add_layer_group(&mut self, name: &str) -> u32 {
        let id = self.alloc_id();
        self.layer_groups.push(LayerGroup {
            id,
            name: name.to_string(),
            enabled: true,
            opacity: 1.0,
            pinned: false,
        });
        id
    }

    fn is_pinned(&self, layer: &Layer) -> bool {
        layer
            .group
            .is_some_and(|g| self.layer_groups.iter().any(|x| x.id == g && x.pinned))
    }

    /// The part of the stack a scene saves: everything outside pinned folders.
    pub fn scene_part(&self) -> (Vec<Layer>, Vec<LayerGroup>) {
        let layers = self.layers.iter().filter(|l| !self.is_pinned(l)).cloned().collect();
        let groups = self.layer_groups.iter().filter(|g| !g.pinned).cloned().collect();
        (layers, groups)
    }

    /// What plays when a scene is switched to: the scene's layers with the
    /// pinned folders, as they are now, kept on top.
    pub fn scene_stack(&self, scene: &Scene) -> (Vec<Layer>, Vec<LayerGroup>) {
        let pinned: Vec<&LayerGroup> = self.layer_groups.iter().filter(|g| g.pinned).collect();
        let kept: Vec<&Layer> = self.layers.iter().filter(|l| self.is_pinned(l)).collect();
        // A scene saved before a folder was pinned may still hold that
        // folder or its layers; the pinned ones win.
        let mut layers: Vec<Layer> = scene
            .layers
            .iter()
            .filter(|l| {
                !kept.iter().any(|k| k.id == l.id) && !l.group.is_some_and(|g| pinned.iter().any(|p| p.id == g))
            })
            .cloned()
            .collect();
        layers.extend(kept.into_iter().cloned());
        let mut groups: Vec<LayerGroup> = scene
            .groups
            .iter()
            .filter(|g| !pinned.iter().any(|p| p.id == g.id))
            .cloned()
            .collect();
        groups.extend(pinned.into_iter().cloned());
        (layers, groups)
    }

    /// Removes a folder. Its layers go with it, or stay in place unfiled.
    pub fn remove_layer_group(&mut self, id: u32, keep_layers: bool) {
        self.layer_groups.retain(|g| g.id != id);
        if keep_layers {
            for layer in self.layers.iter_mut().filter(|l| l.group == Some(id)) {
                layer.group = None;
            }
        } else {
            self.layers.retain(|l| l.group != Some(id));
        }
        self.prune_bindings();
    }

    /// Puts the layer stack in order: a layer filed in a folder that no
    /// longer exists becomes unfiled, and each folder's layers are pulled
    /// together at the place of the lowest one. Order is otherwise kept.
    pub fn tidy_layers(&mut self) {
        tidy(&mut self.layers, &self.layer_groups);
    }

    /// Adds an empty group of lights and returns its id.
    pub fn add_light_group(&mut self, name: &str) -> u32 {
        let id = self.alloc_id();
        self.light_groups.push(LightGroup {
            id,
            name: name.to_string(),
            enabled: true,
            brightness: 1.0,
        });
        id
    }

    /// Removes a group of lights. The lights stay. An effect that played
    /// on the group keeps playing on the lights that were in it.
    pub fn remove_light_group(&mut self, id: u32) {
        let members: Vec<u32> = self
            .fixtures
            .iter()
            .filter(|f| f.group == Some(id))
            .map(|f| f.id)
            .collect();
        self.light_groups.retain(|g| g.id != id);
        for fixture in &mut self.fixtures {
            if fixture.group == Some(id) {
                fixture.group = None;
            }
        }
        let rewrite = |layers: &mut Vec<Layer>| {
            for layer in layers.iter_mut().filter(|l| l.targets.contains(&id)) {
                layer.targets.retain(|t| *t != id);
                layer.targets.extend(&members);
                layer.targets.sort_unstable();
                layer.targets.dedup();
                // A layer aimed only at an empty group must not silently
                // spread to every light.
                if layer.targets.is_empty() {
                    layer.enabled = false;
                }
            }
        };
        rewrite(&mut self.layers);
        for scene in &mut self.scenes {
            rewrite(&mut scene.layers);
        }
        self.prune_bindings();
    }

    /// Whether a layer draws on a fixture.
    pub fn layer_hits(layer: &Layer, fixture: &Fixture) -> bool {
        layer.only.allows(&fixture.shape)
            && (layer.targets.is_empty()
                || layer.targets.contains(&fixture.id)
                || fixture.group.is_some_and(|g| layer.targets.contains(&g)))
    }

    /// Drops keys whose target no longer exists anywhere in the show. A
    /// layer or folder that lives on in a saved scene keeps its key.
    pub fn prune_bindings(&mut self) {
        let layers: Vec<u32> = self
            .layers
            .iter()
            .chain(self.scenes.iter().flat_map(|s| &s.layers))
            .map(|l| l.id)
            .collect();
        let folders: Vec<u32> = self
            .layer_groups
            .iter()
            .chain(self.scenes.iter().flat_map(|s| &s.groups))
            .map(|g| g.id)
            .collect();
        let (scenes, light_groups, swatches) = (&self.scenes, &self.light_groups, &self.live.swatches);
        self.bindings.retain(|b| match b.action {
            Action::Scene { id } => scenes.iter().any(|s| s.id == id),
            Action::Layer { id } => layers.contains(&id),
            Action::LayerGroup { id } => folders.contains(&id),
            Action::LightGroup { id } => light_groups.iter().any(|g| g.id == id),
            Action::Colours { id } => swatches.iter().any(|s| s.id == id),
            Action::Blackout
            | Action::TapTempo
            | Action::Hit
            | Action::NextScene
            | Action::PrevScene
            | Action::Freeze => true,
        });
        // A swatch lives as long as its pad does.
        let bindings = &self.bindings;
        self.live
            .swatches
            .retain(|s| bindings.iter().any(|b| b.action == Action::Colours { id: s.id }));
        if self
            .live
            .colours
            .is_some_and(|id| !self.live.swatches.iter().any(|s| s.id == id))
        {
            self.live.colours = None;
        }
    }

    /// The key tied to an action, if there is one.
    pub fn binding_for(&self, action: Action) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.action == action)
    }

    /// Ties a key to an action and returns the pad's id. See [`Show::add_pad`].
    pub fn bind(&mut self, key: &str, label: &str, action: Action, mode: KeyMode) -> u32 {
        self.add_pad(Some(key), label, action, mode)
    }

    /// Gives an action a pad, with or without a key, and returns the pad's
    /// id. A thing has one pad: if it has one already, that pad is changed
    /// and keeps its place. A key does one thing: a pad that had the key
    /// stays, without it.
    pub fn add_pad(&mut self, key: Option<&str>, label: &str, action: Action, mode: KeyMode) -> u32 {
        let mode = action.allowed(mode);
        if let Some(key) = key {
            // The key is taken from any pad that could be showing at the
            // same time as this one.
            let page = self.bindings.iter().find(|b| b.action == action).map_or(0, |b| b.page);
            for other in self
                .bindings
                .iter_mut()
                .filter(|b| b.key.as_deref() == Some(key) && b.action != action && b.shares_a_page_with(page))
            {
                other.key = None;
                other.label.clear();
            }
        }
        let (key, label) = (key.map(str::to_string), if key.is_some() { label } else { "" });
        if let Some(pad) = self.bindings.iter_mut().find(|b| b.action == action) {
            pad.key = key;
            pad.label = label.to_string();
            pad.mode = mode;
            return pad.id;
        }
        let id = self.alloc_id();
        self.bindings.push(Binding {
            id,
            key,
            label: label.to_string(),
            action,
            mode,
            midi: None,
            page: 0,
        });
        id
    }

    /// The scene `step` places after the one that is playing (before, for
    /// a negative step), going round at the ends. `from` is the scene to
    /// count from when what is playing matches no scene.
    pub fn scene_after(&self, from: Option<u32>, step: i32) -> Option<u32> {
        if self.scenes.is_empty() {
            return None;
        }
        let count = self.scenes.len() as i32;
        let at = self
            .current_scene()
            .or(from)
            .and_then(|id| self.scenes.iter().position(|s| s.id == id));
        let next = match at {
            Some(at) => (at as i32 + step).rem_euclid(count),
            // Nothing to count from: start at the first, or the last going back.
            None if step >= 0 => 0,
            None => count - 1,
        };
        Some(self.scenes[next as usize].id)
    }

    /// Adds a device and one fixture covering all of its pixels, placed
    /// below the fixtures already on the canvas. Returns the device id.
    pub fn add_device(&mut self, mut device: Device, matrix: Option<(u32, u32)>) -> u32 {
        let device_id = self.alloc_id();
        device.id = device_id;
        let fixture_id = self.alloc_id();

        let (shape, width, height) = match matrix {
            Some((cols, rows))
                if cols > 1 && rows > 1 && cols.checked_mul(rows).is_some_and(|n| n <= device.pixel_count) =>
            {
                let aspect = rows as f32 / cols as f32;
                let width = if aspect > 1.0 { 0.5 / aspect } else { 0.5 };
                (
                    Shape::Matrix {
                        cols,
                        rows,
                        serpentine: false,
                    },
                    width,
                    width * aspect,
                )
            }
            _ => (
                Shape::Strip {
                    count: device.pixel_count,
                    reverse: false,
                },
                1.0,
                0.0,
            ),
        };

        // Stack new fixtures downward so they never land on top of each other.
        let bottom = self
            .fixtures
            .iter()
            .map(|f| f.y + f.height.max(0.04) / 2.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let y = if bottom.is_finite() {
            bottom + 0.1 + height / 2.0
        } else {
            0.0
        };

        self.fixtures.push(Fixture {
            id: fixture_id,
            name: device.name.clone(),
            device: device_id,
            start: 0,
            shape,
            x: 0.0,
            y,
            width,
            height,
            rotation: 0.0,
            group: None,
            path: Vec::new(),
            closed: false,
        });
        self.devices.push(device);
        device_id
    }

    /// Adds a light that exists only in the preview, with one fixture of
    /// the given shape. Returns the device id.
    pub fn add_virtual(&mut self, name: &str, shape: &Shape, demo: bool) -> u32 {
        let mut device = Device::virtual_light(name, shape.pixel_count());
        device.demo = demo;
        let matrix = match *shape {
            Shape::Matrix { cols, rows, .. } => Some((cols, rows)),
            Shape::Strip { .. } => None,
        };
        self.add_device(device, matrix)
    }

    /// True when the built-in demo lights are in the show.
    pub fn has_demo(&self) -> bool {
        self.devices.iter().any(|d| d.demo)
    }

    /// Adds or removes the built-in demo lights: two strips and a matrix,
    /// enough to see every kind of effect without owning hardware.
    pub fn set_demo(&mut self, enabled: bool) {
        if enabled == self.has_demo() {
            return;
        }
        if enabled {
            let strip = |count| Shape::Strip { count, reverse: false };
            self.add_virtual("Demo strip", &strip(120), true);
            self.add_virtual("Demo strip 2", &strip(90), true);
            let matrix = Shape::Matrix {
                cols: 16,
                rows: 16,
                serpentine: false,
            };
            self.add_virtual("Demo matrix", &matrix, true);
        } else {
            let demo: Vec<u32> = self.devices.iter().filter(|d| d.demo).map(|d| d.id).collect();
            for id in demo {
                self.remove_device(id);
            }
        }
    }

    /// Removes a device, its fixtures, and any layer targets pointing at them.
    pub fn remove_device(&mut self, id: u32) {
        let gone: Vec<u32> = self.fixtures.iter().filter(|f| f.device == id).map(|f| f.id).collect();
        self.devices.retain(|d| d.id != id);
        self.fixtures.retain(|f| f.device != id);
        let scrub = |layers: &mut Vec<Layer>| {
            for layer in layers {
                let had_targets = !layer.targets.is_empty();
                layer.targets.retain(|t| !gone.contains(t));
                // A layer aimed only at removed fixtures must not silently
                // spread to every fixture.
                if had_targets && layer.targets.is_empty() {
                    layer.enabled = false;
                }
            }
        };
        scrub(&mut self.layers);
        for scene in &mut self.scenes {
            scrub(&mut scene.layers);
        }
    }

    /// Saves the current layers and their folders as a scene and returns
    /// its id. The first ten scenes get the number keys, so a new user can
    /// switch scenes from the keyboard without setting anything up.
    pub fn save_scene(&mut self, name: &str) -> u32 {
        let (layers, groups) = self.scene_part();
        self.push_scene(name, layers, groups)
    }

    /// Adds a scene made of the given layers and folders, and gives it a
    /// number key if one is free. Returns its id.
    pub fn push_scene(&mut self, name: &str, layers: Vec<Layer>, groups: Vec<LayerGroup>) -> u32 {
        let id = self.alloc_id();
        self.scenes.push(Scene {
            id,
            name: name.to_string(),
            layers,
            groups,
            auto: true,
            mood: Mood::Any,
        });
        let free = "1234567890"
            .chars()
            .find(|digit| !self.bindings.iter().any(|b| b.key == Some(format!("Digit{digit}"))));
        if let Some(digit) = free {
            self.bind(
                &format!("Digit{digit}"),
                &digit.to_string(),
                Action::Scene { id },
                KeyMode::Toggle,
            );
        }
        id
    }

    /// Replaces the current layers and folders with a scene's. Returns
    /// false if unknown.
    pub fn recall_scene(&mut self, id: u32) -> bool {
        let Some(scene) = self.scenes.iter().find(|s| s.id == id) else {
            return false;
        };
        (self.layers, self.layer_groups) = self.scene_stack(scene);
        self.tidy_layers();
        // An effect that was never saved in a scene is gone now, and its
        // key with it.
        self.prune_bindings();
        true
    }

    /// The scene whose layers and folders are exactly what is playing,
    /// pinned folders aside.
    pub fn current_scene(&self) -> Option<u32> {
        let (layers, groups) = self.scene_part();
        self.scenes
            .iter()
            .find(|s| s.layers == layers && s.groups == groups)
            .map(|s| s.id)
    }

    /// Removes a scene and its key. Returns false if unknown.
    pub fn delete_scene(&mut self, id: u32) -> bool {
        let before = self.scenes.len();
        self.scenes.retain(|s| s.id != id);
        if self.live.idle.scene == Some(id) {
            self.live.idle.scene = None;
        }
        self.prune_bindings();
        self.scenes.len() != before
    }

    /// Mends what a file from an older version, or edited by hand, can get
    /// wrong without being unusable: folders, groups and keys that point
    /// at things that are not there.
    pub fn repair(&mut self) {
        let known = |groups: &[LightGroup], id: u32| groups.iter().any(|g| g.id == id);
        for fixture in &mut self.fixtures {
            if fixture.group.is_some_and(|g| !known(&self.light_groups, g)) {
                fixture.group = None;
            }
        }
        self.tidy_layers();
        for scene in &mut self.scenes {
            tidy(&mut scene.layers, &scene.groups);
        }
        for device in &mut self.devices {
            device.mend_universe();
        }
        for fixture in &mut self.fixtures {
            fixture.mend_path();
        }
        if !FRAME_RATES.contains(&self.output.frame_rate) {
            self.output.frame_rate = OutputSettings::default().frame_rate;
        }
        self.live.auto.every_bars = self.live.auto.every_bars.clamp(1, 64);
        self.schedule.on %= 1440;
        self.schedule.off %= 1440;
        if self
            .live
            .idle
            .scene
            .is_some_and(|id| !self.scenes.iter().any(|s| s.id == id))
        {
            self.live.idle.scene = None;
        }
        self.live.idle.after = if self.live.idle.after.is_finite() {
            self.live.idle.after.clamp(3.0, 600.0)
        } else {
            default_idle_after()
        };
        // Pages in range; then one pad per key and per MIDI note among
        // pads that can be showing together.
        self.live.page = self.live.page.clamp(1, PAGES);
        let mut keys: Vec<(String, u8)> = Vec::new();
        let mut notes: Vec<(MidiNote, u8)> = Vec::new();
        for binding in &mut self.bindings {
            binding.page = binding.page.min(PAGES);
            let page = binding.page;
            let clash = |other: u8| other == 0 || page == 0 || other == page;
            if let Some(key) = &binding.key {
                if keys.iter().any(|(k, p)| k == key && clash(*p)) {
                    binding.key = None;
                    binding.label.clear();
                } else {
                    keys.push((key.clone(), page));
                }
            }
            if let Some(note) = binding.midi {
                if note.channel > 15 || note.note > 127 || notes.iter().any(|(n, p)| *n == note && clash(*p)) {
                    binding.midi = None;
                } else {
                    notes.push((note, page));
                }
            }
        }
        let mut seen: Vec<Knob> = Vec::new();
        self.live.knobs.retain(|k| {
            let fits = k.channel <= 15
                && k.cc <= 127
                && !matches!(k.target, KnobTarget::Fader { n } if n == 0 || n as usize > FADERS);
            let fresh = !seen
                .iter()
                .any(|s| s.target == k.target || (s.channel, s.cc) == (k.channel, k.cc));
            seen.push(*k);
            fits && fresh
        });
        for fader in &mut self.live.faders {
            *fader = if fader.is_finite() { fader.clamp(0.0, 1.0) } else { 0.0 };
        }
        for layer in self
            .layers
            .iter_mut()
            .chain(self.scenes.iter_mut().flat_map(|s| s.layers.iter_mut()))
        {
            mend_links(layer);
        }
        self.prune_bindings();
    }

    /// Checks a show that came from outside the program (a file). Sizes
    /// must be sane and every number finite, so that rendering cannot be
    /// asked for absurd amounts of memory or fed NaN positions.
    pub fn validate(&self) -> Result<(), String> {
        for device in &self.devices {
            if device.pixel_count > MAX_PIXELS {
                return Err(format!("device {:?} claims {} pixels", device.name, device.pixel_count));
            }
            if !(device.gamma.is_finite() && device.brightness.is_finite()) {
                return Err(format!("device {:?} has a setting that is not a number", device.name));
            }
        }
        for fixture in &self.fixtures {
            let pixels = match fixture.shape {
                Shape::Strip { count, .. } => Some(count),
                Shape::Matrix { cols, rows, .. } => cols.checked_mul(rows),
            };
            if pixels.is_none_or(|n| n > MAX_PIXELS) {
                return Err(format!("fixture {:?} is too large", fixture.name));
            }
            let numbers = [fixture.x, fixture.y, fixture.width, fixture.height, fixture.rotation];
            if numbers.iter().any(|n| !n.is_finite()) {
                return Err(format!(
                    "fixture {:?} has a position that is not a number",
                    fixture.name
                ));
            }
        }
        let layers = self.layers.iter().chain(self.scenes.iter().flat_map(|s| &s.layers));
        for layer in layers {
            let range_ok = match layer.sound.band {
                Band::Range { low, high } => low.is_finite() && high.is_finite(),
                _ => true,
            };
            if !(layer.opacity.is_finite() && layer.sound.gain.is_finite() && layer.sound.pulse.is_finite() && range_ok)
            {
                return Err("a layer has a setting that is not a number".into());
            }
        }
        let folders = self
            .layer_groups
            .iter()
            .chain(self.scenes.iter().flat_map(|s| &s.groups));
        if folders.into_iter().any(|g| !g.opacity.is_finite())
            || self.light_groups.iter().any(|g| !g.brightness.is_finite())
        {
            return Err("a group has a setting that is not a number".into());
        }
        let numbers = [
            self.master.brightness,
            self.master.limit,
            self.audio.sensitivity,
            self.audio.delay_ms,
            self.audio.dynamics,
            self.live.fade,
            self.live.speed,
            self.flash.limit,
        ];
        if numbers.iter().any(|n| !n.is_finite()) {
            return Err("a master setting is not a number".into());
        }
        Ok(())
    }

    /// Total pixels across all fixtures, in fixture order.
    pub fn pixel_count(&self) -> usize {
        self.fixtures.iter().map(|f| f.pixel_count() as usize).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, pixels: u32) -> Device {
        Device::wled(name, "127.0.0.1", 4048, pixels)
    }

    #[test]
    fn new_show_has_one_layer_and_round_trips() {
        let show = Show::default();
        assert_eq!(show.layers.len(), 1);
        let json = serde_json::to_string(&show).unwrap();
        let back: Show = serde_json::from_str(&json).unwrap();
        assert_eq!(show, back);
    }

    #[test]
    fn minimal_file_loads_with_defaults() {
        let show: Show = serde_json::from_str(r#"{"version":1,"name":"x"}"#).unwrap();
        assert!(show.devices.is_empty());
        assert_eq!(show.master.brightness, 1.0);
        assert_eq!(show.audio.source, "system");
    }

    #[test]
    fn devices_get_fixtures_that_do_not_overlap() {
        let mut show = Show::default();
        show.add_device(device("strip", 60), None);
        show.add_device(device("matrix", 256), Some((16, 16)));
        show.add_device(device("strip 2", 30), None);
        assert_eq!(show.fixtures.len(), 3);
        assert!(matches!(
            show.fixtures[1].shape,
            Shape::Matrix { cols: 16, rows: 16, .. }
        ));
        let ys: Vec<f32> = show.fixtures.iter().map(|f| f.y).collect();
        assert!(ys[0] < ys[1] && ys[1] < ys[2], "{ys:?}");
        assert_eq!(show.pixel_count(), 60 + 256 + 30);
    }

    #[test]
    fn a_matrix_size_that_does_not_fit_becomes_a_strip() {
        let mut show = Show::default();
        show.add_device(device("odd", 100), Some((16, 16)));
        assert!(matches!(show.fixtures[0].shape, Shape::Strip { count: 100, .. }));
    }

    #[test]
    fn an_impossible_matrix_size_does_not_overflow() {
        let mut show = Show::default();
        show.add_device(device("odd", 100), Some((u32::MAX, u32::MAX)));
        assert!(matches!(show.fixtures[0].shape, Shape::Strip { count: 100, .. }));
    }

    #[test]
    fn validate_rejects_absurd_sizes_and_bad_numbers() {
        let mut show = Show::default();
        show.add_device(device("ok", 300), Some((20, 15)));
        assert!(show.validate().is_ok());

        let mut huge = show.clone();
        huge.devices[0].pixel_count = u32::MAX;
        assert!(huge.validate().is_err());

        let mut huge = show.clone();
        huge.fixtures[0].shape = Shape::Matrix {
            cols: 70_000,
            rows: 70_000,
            serpentine: false,
        };
        assert!(huge.validate().is_err(), "the product overflows 32 bits");

        let mut huge = show.clone();
        huge.fixtures[0].shape = Shape::Strip {
            count: MAX_PIXELS + 1,
            reverse: false,
        };
        assert!(huge.validate().is_err());

        let mut nan = show.clone();
        nan.fixtures[0].x = f32::NAN;
        assert!(nan.validate().is_err());

        let mut nan = show.clone();
        nan.master.brightness = f32::INFINITY;
        assert!(nan.validate().is_err());
    }

    #[test]
    fn ids_are_unique_even_if_the_counter_is_stale() {
        let mut show = Show::default();
        show.add_device(device("a", 10), None);
        show.next_id = 1;
        let id = show.alloc_id();
        assert!(show.devices.iter().all(|d| d.id != id));
        assert!(show.fixtures.iter().all(|f| f.id != id));
        assert!(show.layers.iter().all(|l| l.id != id));
    }

    #[test]
    fn removing_a_device_disables_layers_that_only_targeted_it() {
        let mut show = Show::default();
        let a = show.add_device(device("a", 10), None);
        show.add_device(device("b", 10), None);
        let fixture_a = show.fixtures.iter().find(|f| f.device == a).unwrap().id;
        let layer = show.add_layer("solid");
        show.layer_mut(layer).unwrap().targets = vec![fixture_a];
        show.remove_device(a);
        assert_eq!(show.devices.len(), 1);
        assert_eq!(show.fixtures.len(), 1);
        let layer = show.layers.iter().find(|l| l.id == layer).unwrap();
        assert!(layer.targets.is_empty());
        assert!(!layer.enabled);
    }

    #[test]
    fn demo_lights_are_virtual_and_come_and_go_as_a_set() {
        let mut show = Show::default();
        show.add_device(device("real", 10), None);
        show.set_demo(true);
        show.set_demo(true);
        assert_eq!(show.devices.len(), 4, "turning it on twice adds one set");
        let demo: Vec<_> = show.devices.iter().filter(|d| d.demo).collect();
        assert_eq!(demo.len(), 3);
        assert!(
            demo.iter()
                .all(|d| d.kind == crate::layout::DeviceKind::Virtual && !d.sends())
        );
        assert!(show.devices[0].sends(), "the real device still sends");
        assert_eq!(show.pixel_count(), 10 + 120 + 90 + 256);
        assert!(show.validate().is_ok());

        // A layer aimed at a demo light is switched off with it, not spread to every light.
        let demo_fixture = show.fixtures.iter().find(|f| f.name == "Demo matrix").unwrap().id;
        let layer = show.add_layer("solid");
        show.layer_mut(layer).unwrap().targets = vec![demo_fixture];
        show.set_demo(false);
        assert_eq!(show.devices.len(), 1);
        assert_eq!(show.fixtures.len(), 1);
        assert!(!show.layers.iter().find(|l| l.id == layer).unwrap().enabled);
    }

    #[test]
    fn a_virtual_light_gets_a_fixture_of_its_shape() {
        let mut show = Show::default();
        let id = show.add_virtual(
            "Ceiling plan",
            &Shape::Matrix {
                cols: 30,
                rows: 10,
                serpentine: false,
            },
            false,
        );
        let device = show.device(id).unwrap();
        assert_eq!(
            (device.pixel_count, device.demo, device.address.as_str()),
            (300, false, "")
        );
        assert!(matches!(
            show.fixtures[0].shape,
            Shape::Matrix { cols: 30, rows: 10, .. }
        ));
        assert!(!show.has_demo());
    }

    #[test]
    fn older_show_files_without_the_new_fields_still_load() {
        // A layer and a device as version 0.1 wrote them.
        let json = r#"{"version":1,"name":"old",
            "devices":[{"id":1,"name":"d","address":"10.0.0.2","pixel_count":30}],
            "layers":[{"id":2,"effect":"plasma"}]}"#;
        let show: Show = serde_json::from_str(json).unwrap();
        assert_eq!(show.devices[0].kind, crate::layout::DeviceKind::Wled);
        assert!(!show.devices[0].demo);
        assert_eq!(show.layers[0].sound, Sound::default());
        assert_eq!(show.layers[0].name, None);
    }

    #[test]
    fn a_folders_layers_are_kept_together() {
        let mut show = Show::default();
        let a = show.layers[0].id;
        let folder = show.add_layer_group("Chorus");
        let b = show.add_layer("solid");
        let c = show.add_layer("sparkle");
        show.layer_mut(a).unwrap().group = Some(folder);
        show.layer_mut(c).unwrap().group = Some(folder);
        show.tidy_layers();
        let order: Vec<u32> = show.layers.iter().map(|l| l.id).collect();
        assert_eq!(order, [a, c, b], "c joins a at a's place; b keeps its turn");

        // A new layer in the folder goes on top of the folder, not the stack.
        let d = show.add_layer_in("flow", Some(folder));
        let order: Vec<u32> = show.layers.iter().map(|l| l.id).collect();
        assert_eq!(order, [a, c, d, b]);
        // A folder that does not exist is ignored.
        let e = show.add_layer_in("flow", Some(9999));
        assert_eq!(show.layers.last().unwrap().id, e);
        assert_eq!(show.layers.last().unwrap().group, None);
    }

    #[test]
    fn removing_a_folder_keeps_or_removes_its_layers() {
        let mut show = Show::default();
        let folder = show.add_layer_group("Chorus");
        let inside = show.add_layer_in("solid", Some(folder));
        let mut kept = show.clone();
        kept.remove_layer_group(folder, true);
        assert_eq!(kept.layers.len(), 2);
        assert!(kept.layers.iter().all(|l| l.group.is_none()));
        show.remove_layer_group(folder, false);
        assert!(show.layers.iter().all(|l| l.id != inside));
        assert!(show.layer_groups.is_empty());
    }

    #[test]
    fn removing_a_light_group_keeps_effects_on_its_lights() {
        let mut show = Show::default();
        show.add_device(device("a", 10), None);
        show.add_device(device("b", 10), None);
        show.add_device(device("c", 10), None);
        let group = show.add_light_group("Ceiling");
        let (fa, fb) = (show.fixtures[0].id, show.fixtures[1].id);
        show.fixtures[0].group = Some(group);
        show.fixtures[1].group = Some(group);
        let layer = show.layers[0].id;
        show.layer_mut(layer).unwrap().targets = vec![group];
        show.bind("KeyC", "C", Action::LightGroup { id: group }, KeyMode::Hold);

        show.remove_light_group(group);
        assert!(show.fixtures.iter().all(|f| f.group.is_none()));
        assert_eq!(show.layers[0].targets, [fa, fb]);
        assert!(show.layers[0].enabled);
        assert!(show.bindings.is_empty(), "the key went with the group");

        // An effect aimed only at an empty group is switched off, not spread to every light.
        let empty = show.add_light_group("Empty");
        show.layer_mut(layer).unwrap().targets = vec![empty];
        show.remove_light_group(empty);
        assert!(show.layers[0].targets.is_empty());
        assert!(!show.layers[0].enabled);
    }

    #[test]
    fn a_key_does_one_thing_and_a_thing_has_one_key() {
        let mut show = Show::default();
        let layer = show.layers[0].id;
        let pad = show.bind("KeyA", "A", Action::Layer { id: layer }, KeyMode::Toggle);
        show.bind("KeyA", "A", Action::Blackout, KeyMode::Hold);
        assert_eq!(show.bindings.len(), 2);
        assert_eq!(show.bindings[0].key, None, "the key moved to blackout; the pad stayed");
        assert_eq!(show.bindings[0].label, "");
        let blackout = show.bind("KeyB", "B", Action::Blackout, KeyMode::Toggle);
        assert_eq!(show.bindings.len(), 2, "blackout kept its one pad");
        assert_eq!(show.bindings[1].key.as_deref(), Some("KeyB"));
        assert_eq!(show.binding_for(Action::Blackout).unwrap().mode, KeyMode::Toggle);
        // A pad that gets a key again keeps its id and its place.
        assert_eq!(show.bind("KeyC", "C", Action::Layer { id: layer }, KeyMode::Hold), pad);
        assert_eq!(show.bindings.iter().map(|b| b.id).collect::<Vec<_>>(), [pad, blackout]);
    }

    #[test]
    fn pads_need_no_key_and_only_do_what_their_thing_can() {
        let mut show = Show::default();
        let layer = show.layers[0].id;
        let scene = show.save_scene("One");
        // A pad for touch only.
        let pad = show.add_pad(None, "ignored", Action::Layer { id: layer }, KeyMode::Fire);
        assert_eq!(show.bindings.last().unwrap().key, None);
        assert_eq!(show.bindings.last().unwrap().label, "");
        assert_eq!(show.bindings.last().unwrap().mode, KeyMode::Fire);
        assert_eq!(show.add_pad(None, "", Action::Layer { id: layer }, KeyMode::Hold), pad);

        // An effect can be fired and held; a scene held but not fired; a tap neither.
        assert_eq!(Action::Scene { id: scene }.allowed(KeyMode::Hold), KeyMode::Hold);
        assert_eq!(Action::Scene { id: scene }.allowed(KeyMode::Fire), KeyMode::Toggle);
        assert_eq!(Action::Blackout.allowed(KeyMode::Fire), KeyMode::Toggle);
        assert_eq!(Action::Freeze.allowed(KeyMode::Hold), KeyMode::Hold);
        assert_eq!(Action::TapTempo.allowed(KeyMode::Hold), KeyMode::Toggle);
        assert_eq!(Action::NextScene.allowed(KeyMode::Hold), KeyMode::Toggle);
        show.add_pad(None, "", Action::NextScene, KeyMode::Fire);
        assert_eq!(show.binding_for(Action::NextScene).unwrap().mode, KeyMode::Toggle);
        show.prune_bindings();
        assert!(
            show.binding_for(Action::NextScene).is_some(),
            "it points at nothing that can vanish"
        );
    }

    #[test]
    fn next_and_previous_scene_go_round() {
        let mut show = Show::default();
        assert_eq!(show.scene_after(None, 1), None);
        let a = show.save_scene("A");
        show.add_layer("solid");
        let b = show.save_scene("B");
        show.add_layer("flow");
        let c = show.save_scene("C");
        assert_eq!(show.current_scene(), Some(c));
        assert_eq!(show.scene_after(None, 1), Some(a), "after the last comes the first");
        assert_eq!(show.scene_after(None, -1), Some(b));
        // What is playing matches no scene: count from the one given, or from an end.
        show.add_layer("sparkle");
        assert_eq!(show.current_scene(), None);
        assert_eq!(show.scene_after(Some(a), 1), Some(b));
        assert_eq!(show.scene_after(None, 1), Some(a));
        assert_eq!(show.scene_after(None, -1), Some(c));
        assert_eq!(show.scene_after(Some(9999), 1), Some(a));
    }

    #[test]
    fn scenes_take_the_number_keys_until_they_run_out() {
        let mut show = Show::default();
        let first = show.save_scene("one");
        let key_of = |show: &Show, id| show.binding_for(Action::Scene { id }).unwrap().key.clone().unwrap();
        assert_eq!(key_of(&show, first), "Digit1");
        show.bind("Digit2", "2", Action::Blackout, KeyMode::Toggle);
        let second = show.save_scene("two");
        assert_eq!(key_of(&show, second), "Digit3");
        for n in 0..10 {
            show.save_scene(&format!("more {n}"));
        }
        assert_eq!(show.bindings.len(), 10, "ten number keys, then no more");

        assert!(show.delete_scene(first));
        assert!(show.binding_for(Action::Scene { id: first }).is_none());
        assert!(!show.delete_scene(first));
    }

    #[test]
    fn a_key_survives_while_its_effect_lives_on_in_a_scene() {
        let mut show = Show::default();
        let extra = show.add_layer("solid");
        show.bind("KeyF", "F", Action::Layer { id: extra }, KeyMode::Hold);
        let scene = show.save_scene("with extra");
        show.layers.retain(|l| l.id != extra);
        show.prune_bindings();
        assert!(show.binding_for(Action::Layer { id: extra }).is_some());
        show.delete_scene(scene);
        assert!(show.binding_for(Action::Layer { id: extra }).is_none());
    }

    #[test]
    fn pinned_folders_stay_on_top_and_through_scene_changes() {
        let mut show = Show::default();
        let plasma = show.layers[0].id;
        let live = show.add_layer_group("Live");
        show.layer_groups[0].pinned = true;
        let flash = show.add_layer_in("solid", Some(live));
        show.bind("KeyF", "F", Action::Layer { id: flash }, KeyMode::Hold);
        // A new effect goes under the pinned folder, not over it.
        let ripple = show.add_layer("ripple");
        let order = |show: &Show| show.layers.iter().map(|l| l.id).collect::<Vec<_>>();
        assert_eq!(order(&show), [plasma, ripple, flash]);

        // A scene saves everything but the pinned folder.
        let both = show.save_scene("Both");
        assert_eq!(show.scenes[0].layers.len(), 2);
        assert!(show.scenes[0].groups.is_empty());
        assert_eq!(show.current_scene(), Some(both));
        show.layers.retain(|l| l.id != ripple);
        let one = show.save_scene("One");

        // Switching scenes keeps the pinned folder, its effect and its key.
        assert!(show.recall_scene(both));
        assert_eq!(order(&show), [plasma, ripple, flash]);
        assert_eq!(show.layer_groups.len(), 1);
        assert!(show.binding_for(Action::Layer { id: flash }).is_some());
        assert_eq!(show.current_scene(), Some(both));
        assert!(show.recall_scene(one));
        assert_eq!(order(&show), [plasma, flash]);
        // Changing something in the pinned folder does not make it "no scene".
        show.layer_mut(flash).unwrap().enabled = false;
        assert_eq!(show.current_scene(), Some(one));

        // Tidying puts a pinned folder's effects back on top wherever they were put.
        show.layers.reverse();
        show.tidy_layers();
        assert_eq!(order(&show), [plasma, flash]);
        let mut mended = show.clone();
        mended.repair();
        assert_eq!(mended, show);
    }

    #[test]
    fn a_folder_pinned_after_a_scene_was_saved_is_not_doubled() {
        let mut show = Show::default();
        let folder = show.add_layer_group("Hits");
        let hit = show.add_layer_in("beat_pulse", Some(folder));
        let scene = show.save_scene("With hits");
        assert_eq!(show.scenes[0].layers.len(), 2, "saved while the folder was ordinary");
        show.layer_groups[0].pinned = true;
        assert!(show.recall_scene(scene));
        assert_eq!(show.layers.iter().filter(|l| l.id == hit).count(), 1);
        assert_eq!(show.layer_groups.iter().filter(|g| g.id == folder).count(), 1);
        assert!(show.layer_groups[0].pinned);
    }

    #[test]
    fn scenes_bring_their_folders_back() {
        let mut show = Show::default();
        let folder = show.add_layer_group("Verse");
        show.layers[0].group = Some(folder);
        let scene = show.save_scene("verse");
        assert_eq!(show.current_scene(), Some(scene));
        show.remove_layer_group(folder, false);
        assert!(show.layers.is_empty() && show.layer_groups.is_empty());
        assert_eq!(show.current_scene(), None);
        assert!(show.recall_scene(scene));
        assert_eq!(show.layer_groups[0].name, "Verse");
        assert_eq!(show.layers[0].group, Some(folder));
        assert_eq!(show.current_scene(), Some(scene));
    }

    #[test]
    fn repair_mends_dangling_references() {
        let json = r#"{"version":1,"name":"x",
            "devices":[{"id":1,"name":"d","address":"10.0.0.2","pixel_count":30}],
            "fixtures":[{"id":2,"name":"d","device":1,"shape":{"type":"strip","count":30},
                         "x":0,"y":0,"width":1,"height":0,"group":77}],
            "layer_groups":[{"id":5,"name":"F"}],
            "layers":[{"id":3,"effect":"plasma","group":5},{"id":4,"effect":"solid","group":88},
                      {"id":6,"effect":"flow","group":5}],
            "bindings":[{"id":7,"key":"KeyA","action":{"type":"layer","id":999}},
                        {"id":8,"key":"KeyB","action":{"type":"layer","id":3},"mode":"hold"}],
            "output":{"frame_rate":1000}}"#;
        let mut show: Show = serde_json::from_str(json).unwrap();
        assert!(show.validate().is_ok());
        show.repair();
        assert_eq!(show.fixtures[0].group, None);
        let order: Vec<(u32, Option<u32>)> = show.layers.iter().map(|l| (l.id, l.group)).collect();
        assert_eq!(order, [(3, Some(5)), (6, Some(5)), (4, None)]);
        assert_eq!(show.bindings.len(), 1);
        assert_eq!(show.bindings[0].mode, KeyMode::Hold);
        assert_eq!(show.output.frame_rate, 60);
        let id = show.alloc_id();
        assert!(id > 8, "ids of folders and keys are counted: {id}");
    }

    #[test]
    fn scenes_save_and_recall_layers() {
        let mut show = Show::default();
        let scene = show.save_scene("one");
        show.add_layer("solid");
        assert_eq!(show.layers.len(), 2);
        assert!(show.recall_scene(scene));
        assert_eq!(show.layers.len(), 1);
        assert!(!show.recall_scene(9999));
    }

    #[test]
    fn sources_run_from_nothing_to_full() {
        let faders = [0.0, 0.25, 0.0, 1.0];
        let quiet = AudioFeatures::default();
        let loud = AudioFeatures {
            level: 0.8,
            bass: 0.9,
            mid: 0.4,
            treble: 0.2,
            beat: 1.0,
            onset: 0.7,
            intensity: 0.6,
            active: true,
            ..AudioFeatures::default()
        };
        let heard = AudioFeatures { level: 0.3, ..loud };
        let value = |source: Source| source.value(&heard, &loud, &faders, 0.0);
        assert_eq!(value(Source::Level), 0.3, "loudness is what the layer hears");
        assert_eq!(value(Source::Bass), 0.9, "bass is the whole sound's");
        assert_eq!((value(Source::Mid), value(Source::Treble)), (0.4, 0.2));
        assert_eq!((value(Source::Beat), value(Source::Hit)), (1.0, 0.7));
        assert_eq!(value(Source::Energy), 0.6);
        assert_eq!(
            Source::Energy.value(&quiet, &quiet, &faders, 0.0),
            0.0,
            "no song, no energy"
        );
        assert_eq!(
            (value(Source::Fader { n: 2 }), value(Source::Fader { n: 4 })),
            (0.25, 1.0)
        );
        assert_eq!(value(Source::Fader { n: 0 }), 0.0);
        assert_eq!(value(Source::Fader { n: 9 }), 0.0);

        // A wave goes out and back once every so many beats.
        let at = |beat: f64| {
            let audio = AudioFeatures {
                bpm: 120.0,
                beat_count: beat.floor() as u32,
                beat_phase: beat.fract() as f32,
                ..loud
            };
            Source::Wave { beats: 8.0 }.value(&audio, &audio, &faders, 0.0)
        };
        assert!(at(0.0) < 0.01 && at(8.0) < 0.01);
        assert!((at(4.0) - 1.0).abs() < 0.01);
        assert!((at(2.0) - 0.5).abs() < 0.01 && (at(6.0) - 0.5).abs() < 0.01);
        // With no tempo it keeps time as if at 120: 8 beats in 4 seconds.
        let free = |time: f64| Source::Wave { beats: 8.0 }.value(&quiet, &quiet, &faders, time);
        assert!(free(0.0) < 0.01 && (free(2.0) - 1.0).abs() < 0.01 && free(4.0) < 0.01);
        // Nonsense in, something sane out.
        let odd = Source::Wave { beats: f32::NAN }.value(&quiet, &quiet, &faders, 1.0);
        assert!((0.0..=1.0).contains(&odd));
    }

    #[test]
    fn repair_mends_links_a_file_should_not_have() {
        let mut show = Show::default();
        let layer = &mut show.layers[0];
        layer.effect = "beat_pulse".into();
        let link = |param: &str, source, amount| Link {
            param: param.into(),
            source,
            amount,
        };
        layer.links = vec![
            link("floor", Source::Fader { n: 200 }, 7.0),
            link("floor", Source::Bass, 0.5),
            link("palette", Source::Bass, 0.5),
            link("nothing", Source::Bass, 0.5),
            link(LINK_STRENGTH, Source::Wave { beats: f32::NAN }, f32::NAN),
        ];
        show.live.faders = [f32::NAN, 3.0, -1.0, 0.5];
        show.repair();
        let links = &show.layers[0].links;
        assert_eq!(links.len(), 2, "one per slider, and only sliders: {links:?}");
        assert_eq!(links[0], link("floor", Source::Fader { n: 4 }, 1.0));
        assert_eq!(links[1], link(LINK_STRENGTH, Source::Wave { beats: 8.0 }, 0.0));
        assert_eq!(show.live.faders, [0.0, 1.0, 0.0, 0.5]);
        assert!(linkable("beat_pulse", "decay") && !linkable("beat_pulse", "palette"));
        assert!(linkable("anything", LINK_STRENGTH));
        let again = show.clone();
        show.repair();
        assert_eq!(show, again);
    }

    #[test]
    fn a_schedule_keeps_hours_including_through_midnight() {
        let at = |h: u16, m: u16| h * 60 + m;
        let mut hours = Schedule::default();
        assert!(hours.lit_at(at(3, 0)), "no schedule: always on");
        hours.enabled = true;
        // Five in the afternoon until eleven at night.
        assert!(!hours.lit_at(at(16, 59)) && hours.lit_at(at(17, 0)));
        assert!(hours.lit_at(at(22, 59)) && !hours.lit_at(at(23, 0)));
        assert!(!hours.lit_at(at(3, 0)));
        // Six in the evening until one in the morning.
        hours.on = at(18, 0);
        hours.off = at(1, 0);
        assert!(hours.lit_at(at(23, 30)) && hours.lit_at(at(0, 59)) && hours.lit_at(at(18, 0)));
        assert!(!hours.lit_at(at(1, 0)) && !hours.lit_at(at(12, 0)) && !hours.lit_at(at(17, 59)));
        // The same time for both is never on, which is at least not a surprise.
        hours.off = hours.on;
        assert!(!hours.lit_at(at(18, 0)) && !hours.lit_at(at(6, 0)));
    }
}
