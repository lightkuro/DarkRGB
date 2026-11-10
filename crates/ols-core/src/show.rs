//! The show file: everything that is saved and restored.

use serde::{Deserialize, Serialize};

use crate::effects;
use crate::layout::{Device, Fixture, Shape};
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

/// One effect running on some or all fixtures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: u32,
    /// Effect id from the registry.
    pub effect: String,
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
        let blend = if self.layers.is_empty() { Blend::Normal } else { Blend::Add };
        self.layers.push(Layer {
            id,
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
            Some((cols, rows)) if cols > 1 && rows > 1 && cols * rows <= device.pixel_count => {
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
        let y = if bottom.is_finite() { bottom + 0.1 + height / 2.0 } else { 0.0 };

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

    /// Total pixels across all fixtures, in fixture order.
    pub fn pixel_count(&self) -> usize {
        self.fixtures.iter().map(|f| f.pixel_count() as usize).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, pixels: u32) -> Device {
        Device {
            id: 0,
            name: name.into(),
            address: "127.0.0.1".into(),
            port: 4048,
            pixel_count: pixels,
            enabled: true,
            gamma: 2.2,
            brightness: 1.0,
            mac: None,
            version: None,
        }
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
        assert!(matches!(show.fixtures[1].shape, Shape::Matrix { cols: 16, rows: 16, .. }));
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
