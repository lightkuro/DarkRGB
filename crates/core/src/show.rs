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
    /// Fixture ids this layer draws on. Empty means all fixtures.
    #[serde(default)]
    pub targets: Vec<u32>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// A saved set of layers that can be recalled in one click.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: u32,
    pub name: String,
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Master {
    pub brightness: f32,
    pub blackout: bool,
}

impl Default for Master {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            blackout: false,
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
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            source: "system".into(),
            sensitivity: 1.0,
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
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub master: Master,
    #[serde(default)]
    pub audio: AudioSettings,
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

impl Default for Show {
    fn default() -> Self {
        let mut show = Show {
            version: SHOW_VERSION,
            name: "Untitled show".into(),
            devices: Vec::new(),
            fixtures: Vec::new(),
            layers: Vec::new(),
            scenes: Vec::new(),
            master: Master::default(),
            audio: AudioSettings::default(),
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
        let id = self.alloc_id();
        let blend = if self.layers.is_empty() {
            Blend::Normal
        } else {
            Blend::Add
        };
        self.layers.push(Layer {
            id,
            name: None,
            sound: Sound::default(),
            effect: effect.to_string(),
            params: Params::new(),
            opacity: 1.0,
            blend,
            targets: Vec::new(),
            enabled: true,
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

    /// Saves the current layers as a scene and returns its id.
    pub fn save_scene(&mut self, name: &str) -> u32 {
        let id = self.alloc_id();
        self.scenes.push(Scene {
            id,
            name: name.to_string(),
            layers: self.layers.clone(),
        });
        id
    }

    /// Replaces the current layers with a scene's. Returns false if unknown.
    pub fn recall_scene(&mut self, id: u32) -> bool {
        let Some(scene) = self.scenes.iter().find(|s| s.id == id) else {
            return false;
        };
        self.layers = scene.layers.clone();
        true
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
        if !self.master.brightness.is_finite() || !self.audio.sensitivity.is_finite() {
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
