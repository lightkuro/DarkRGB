//! The show file: everything that is saved and restored.

use serde::{Deserialize, Serialize};

use crate::audio::Band;
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
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            band: Band::All,
            gain: 1.0,
            pulse: 0.0,
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
}

impl Action {
    /// Actions that have a state to flip, so "while held" means something.
    pub fn is_switch(&self) -> bool {
        matches!(
            self,
            Action::Layer { .. } | Action::LayerGroup { .. } | Action::LightGroup { .. } | Action::Blackout
        )
    }
}

/// How a key that switches something behaves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyMode {
    /// Each press flips it.
    #[default]
    Toggle,
    /// Flipped only while the key is down.
    Hold,
}

/// A key on the keyboard tied to something in the show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub id: u32,
    /// Where the key sits on the keyboard, as browsers name it: `KeyA`,
    /// `Digit1`, `Space`. The same key on any keyboard layout.
    pub key: String,
    /// What is printed on that key on the user's keyboard, for display.
    #[serde(default)]
    pub label: String,
    pub action: Action,
    #[serde(default)]
    pub mode: KeyMode,
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveSettings {
    #[serde(default)]
    pub launch: Launch,
    /// Seconds a scene change takes to fade in, 0..10. Zero cuts.
    #[serde(default)]
    pub fade: f32,
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
    /// Most flashes of the whole picture allowed in a second. Zero
    /// switches the limit off. See `safety.rs`.
    #[serde(default = "default_flash_limit")]
    pub flash_limit: f32,
}

fn default_flash_limit() -> f32 {
    DEFAULT_FLASH_LIMIT
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            frame_rate: 60,
            flash_limit: DEFAULT_FLASH_LIMIT,
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
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            source: "system".into(),
            sensitivity: 1.0,
            delay_ms: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Show {
    pub version: u32,
    pub name: String,
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
    *layers = ordered;
}

impl Default for Show {
    fn default() -> Self {
        let mut show = Show {
            version: SHOW_VERSION,
            name: "Untitled show".into(),
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
        let at = group
            .and_then(|g| self.layers.iter().rposition(|l| l.group == Some(g)))
            .map_or(self.layers.len(), |last| last + 1);
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
        });
        id
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
        layer.targets.is_empty()
            || layer.targets.contains(&fixture.id)
            || fixture.group.is_some_and(|g| layer.targets.contains(&g))
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
        let (scenes, light_groups) = (&self.scenes, &self.light_groups);
        self.bindings.retain(|b| match b.action {
            Action::Scene { id } => scenes.iter().any(|s| s.id == id),
            Action::Layer { id } => layers.contains(&id),
            Action::LayerGroup { id } => folders.contains(&id),
            Action::LightGroup { id } => light_groups.iter().any(|g| g.id == id),
            Action::Blackout | Action::TapTempo | Action::Hit => true,
        });
    }

    /// The key tied to an action, if there is one.
    pub fn binding_for(&self, action: Action) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.action == action)
    }

    /// Ties a key to an action and returns the binding's id. A key does
    /// one thing and a thing has one key, so earlier bindings of either
    /// are replaced.
    pub fn bind(&mut self, key: &str, label: &str, action: Action, mode: KeyMode) -> u32 {
        self.bindings.retain(|b| b.key != key && b.action != action);
        let id = self.alloc_id();
        self.bindings.push(Binding {
            id,
            key: key.to_string(),
            label: label.to_string(),
            action,
            mode,
        });
        id
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
        let id = self.alloc_id();
        self.scenes.push(Scene {
            id,
            name: name.to_string(),
            layers: self.layers.clone(),
            groups: self.layer_groups.clone(),
        });
        let free = "1234567890"
            .chars()
            .find(|digit| !self.bindings.iter().any(|b| b.key == format!("Digit{digit}")));
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
        self.layers = scene.layers.clone();
        self.layer_groups = scene.groups.clone();
        self.tidy_layers();
        // An effect that was never saved in a scene is gone now, and its
        // key with it.
        self.prune_bindings();
        true
    }

    /// The scene whose layers and folders are exactly what is playing.
    pub fn current_scene(&self) -> Option<u32> {
        self.scenes
            .iter()
            .find(|s| s.layers == self.layers && s.groups == self.layer_groups)
            .map(|s| s.id)
    }

    /// Removes a scene and its key. Returns false if unknown.
    pub fn delete_scene(&mut self, id: u32) -> bool {
        let before = self.scenes.len();
        self.scenes.retain(|s| s.id != id);
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
        if !FRAME_RATES.contains(&self.output.frame_rate) {
            self.output.frame_rate = OutputSettings::default().frame_rate;
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
            self.live.fade,
            self.output.flash_limit,
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
        show.bind("KeyA", "A", Action::Layer { id: layer }, KeyMode::Toggle);
        show.bind("KeyA", "A", Action::Blackout, KeyMode::Hold);
        assert_eq!(show.bindings.len(), 1, "the key moved to blackout");
        show.bind("KeyB", "B", Action::Blackout, KeyMode::Toggle);
        assert_eq!(show.bindings.len(), 1, "blackout moved to the new key");
        assert_eq!(show.bindings[0].key, "KeyB");
        assert_eq!(show.binding_for(Action::Blackout).unwrap().mode, KeyMode::Toggle);
    }

    #[test]
    fn scenes_take_the_number_keys_until_they_run_out() {
        let mut show = Show::default();
        let first = show.save_scene("one");
        assert_eq!(show.binding_for(Action::Scene { id: first }).unwrap().key, "Digit1");
        show.bind("Digit2", "2", Action::Blackout, KeyMode::Toggle);
        let second = show.save_scene("two");
        assert_eq!(show.binding_for(Action::Scene { id: second }).unwrap().key, "Digit3");
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
}
