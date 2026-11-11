//! Starter scenes: ready-made looks, each a few effects that work
//! together, so a new show has something worth watching before the user
//! has built anything.
//!
//! A look becomes an ordinary scene when it is added. From then on it is
//! the user's to change or delete.

use crate::audio::Band;
use crate::effects;
use crate::layout::Shape;
use crate::params::{ParamValue, Params};
use crate::show::{Blend, Layer, Show, Sound};

/// Which lights a layer of a look goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum On {
    All,
    /// Only strips, when the show also has matrices.
    Strips,
    /// Only matrices, when the show also has strips.
    Matrices,
}

pub struct LookLayer {
    pub effect: &'static str,
    pub params: &'static [(&'static str, Value)],
    pub band: Band,
    pub blend: Blend,
    pub opacity: f32,
    pub on: On,
    /// On each light by itself, not once across all of them.
    pub each: bool,
}

/// A parameter value that can be written as a constant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Text(&'static str),
    Bool(bool),
}

pub struct Look {
    pub name: &'static str,
    pub description: &'static str,
    /// Bottom layer first.
    pub layers: &'static [LookLayer],
}

const fn layer(effect: &'static str, params: &'static [(&'static str, Value)]) -> LookLayer {
    LookLayer {
        effect,
        params,
        band: Band::All,
        blend: Blend::Add,
        opacity: 1.0,
        on: On::All,
        each: false,
    }
}

/// The same, drawn on each light by itself.
const fn each(effect: &'static str, params: &'static [(&'static str, Value)]) -> LookLayer {
    LookLayer {
        each: true,
        ..layer(effect, params)
    }
}

use Value::{Number as N, Text as T};

/// The starter looks, in the order they are added and autopilot plays them.
/// Each has something that moves without sound, so none sits dark when
/// the music stops.
pub const LOOKS: &[Look] = &[
    Look {
        name: "Pulse",
        description: "A new colour on every beat, with sparks on the highs.",
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..layer(
                    "beat_pulse",
                    &[("palette", T("candy")), ("decay", N(0.45)), ("floor", N(0.12))],
                )
            },
            LookLayer {
                band: Band::Treble,
                ..layer("sparkle", &[("colour", T("#ffffff")), ("density", N(0.3))])
            },
        ],
    },
    Look {
        name: "Spectrum",
        description: "The music's spectrum on matrices, its colours running along strips.",
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.18,
                ..layer("plasma", &[("palette", T("ocean")), ("speed", N(0.2))])
            },
            LookLayer {
                on: On::Strips,
                ..each("scroll", &[("speed", N(1.2))])
            },
            LookLayer {
                on: On::Matrices,
                ..each("spectrum", &[("palette", T("rainbow"))])
            },
            LookLayer {
                band: Band::Bass,
                opacity: 0.5,
                on: On::Matrices,
                ..each("ripple", &[("palette", T("ocean")), ("speed", N(1.4))])
            },
        ],
    },
    Look {
        name: "Neon chase",
        description: "Blocks stepping with the beat over a slow neon glow.",
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.35,
                ..layer("plasma", &[("palette", T("neon")), ("speed", N(0.35))])
            },
            LookLayer {
                on: On::Strips,
                ..each("chase", &[("palette", T("candy")), ("blocks", N(5.0))])
            },
            LookLayer {
                on: On::Matrices,
                ..each("sweep", &[("palette", T("neon")), ("time", N(0.4))])
            },
        ],
    },
    Look {
        name: "Fire",
        description: "Flames on every light that leap with the bass.",
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..each("fire", &[("palette", T("fire")), ("height", N(0.65))])
            },
            LookLayer {
                band: Band::Treble,
                opacity: 0.7,
                ..layer("sparkle", &[("colour", T("#ffb347")), ("density", N(0.15))])
            },
        ],
    },
    Look {
        name: "Rain",
        description: "Streaks falling on every hit, over deep blue.",
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.4,
                ..layer("plasma", &[("palette", T("ocean")), ("speed", N(0.25))])
            },
            each("meteors", &[("palette", T("ice")), ("speed", N(1.2))]),
        ],
    },
    Look {
        name: "Sweep",
        description: "A bar of light across everything on each beat, and a pulse underneath.",
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.3,
                ..layer("plasma", &[("palette", T("sunset")), ("speed", N(0.3))])
            },
            LookLayer {
                band: Band::Bass,
                opacity: 0.4,
                ..layer(
                    "beat_pulse",
                    &[("palette", T("sunset")), ("decay", N(0.6)), ("floor", N(0.0))],
                )
            },
            layer("sweep", &[("palette", T("neon")), ("time", N(0.5))]),
        ],
    },
    Look {
        name: "Calm",
        description: "Slow warm colour for the quiet parts.",
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..layer("plasma", &[("palette", T("sunset")), ("speed", N(0.2))])
            },
            LookLayer {
                opacity: 0.5,
                ..layer("sparkle", &[("colour", T("#ffe0b0")), ("density", N(0.08))])
            },
        ],
    },
];

impl Show {
    /// Adds a starter look as a scene and returns the scene's id. Nothing
    /// is added, and `None` returned, if a scene of that name exists.
    pub fn add_look(&mut self, look: &Look) -> Option<u32> {
        if self.scenes.iter().any(|s| s.name == look.name) {
            return None;
        }
        let of_kind = |show: &Show, strips: bool| -> Vec<u32> {
            show.fixtures
                .iter()
                .filter(|f| matches!(f.shape, Shape::Strip { .. }) == strips)
                .map(|f| f.id)
                .collect()
        };
        let (strips, matrices) = (of_kind(self, true), of_kind(self, false));
        let mut layers = Vec::new();
        for part in look.layers {
            // "Only strips" means something only when there are matrices
            // too. In a show of one kind, every layer plays on everything,
            // and a layer for the kind that is missing is left out if its
            // partner for the other kind is there to take its place.
            let targets = match part.on {
                On::All => Vec::new(),
                On::Strips if matrices.is_empty() => Vec::new(),
                On::Matrices if strips.is_empty() => Vec::new(),
                On::Strips if strips.is_empty() => continue,
                On::Matrices if matrices.is_empty() => continue,
                On::Strips => strips.clone(),
                On::Matrices => matrices.clone(),
            };
            debug_assert!(effects::find(part.effect).is_some(), "{} is not an effect", part.effect);
            let mut params = Params::new();
            for (id, value) in part.params {
                let value = match *value {
                    Value::Number(n) => ParamValue::Number(n),
                    Value::Text(t) => ParamValue::Text(t.to_string()),
                    Value::Bool(b) => ParamValue::Bool(b),
                };
                params.insert((*id).to_string(), value);
            }
            layers.push(Layer {
                id: self.alloc_id(),
                name: None,
                effect: part.effect.to_string(),
                sound: Sound {
                    band: part.band,
                    ..Sound::default()
                },
                params,
                opacity: part.opacity,
                blend: part.blend,
                targets,
                enabled: true,
                group: None,
                each: part.each,
            });
        }
        // The bottom layer of what is left must cover what is under it.
        if let Some(first) = layers.first_mut() {
            first.blend = Blend::Normal;
        }
        Some(self.push_scene(look.name, layers, Vec::new()))
    }

    /// Adds every starter look that is not there yet. Returns how many
    /// were added.
    pub fn add_looks(&mut self) -> usize {
        LOOKS.iter().filter(|look| self.add_look(look).is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioFeatures;
    use crate::layout::Device;
    use crate::params::{ParamKind, ParamReader};
    use crate::render::Renderer;

    #[test]
    fn every_look_uses_real_effects_and_settings_within_range() {
        for look in LOOKS {
            assert!(!look.layers.is_empty(), "{}", look.name);
            for part in look.layers {
                let def =
                    effects::find(part.effect).unwrap_or_else(|| panic!("{}: no effect {}", look.name, part.effect));
                for (id, value) in part.params {
                    let spec = def
                        .info
                        .params
                        .iter()
                        .find(|p| p.id == *id)
                        .unwrap_or_else(|| panic!("{}: {} has no setting {id}", look.name, part.effect));
                    let ok = match (&spec.kind, value) {
                        (ParamKind::Float { min, max, .. }, Value::Number(n)) => (*min..=*max).contains(n),
                        (ParamKind::Palette, Value::Text(t)) => crate::color::palettes().iter().any(|p| p.id == *t),
                        (ParamKind::Color, Value::Text(t)) => crate::color::Rgb::from_hex(t).is_some(),
                        (ParamKind::Choice { options }, Value::Text(t)) => options.contains(t),
                        (ParamKind::Bool, Value::Bool(_)) => true,
                        _ => false,
                    };
                    assert!(ok, "{}: {} = {value:?} is not valid for {}", look.name, id, part.effect);
                }
                assert!((0.0..=1.0).contains(&part.opacity));
            }
        }
        let mut names: Vec<_> = LOOKS.iter().map(|l| l.name).collect();
        names.dedup();
        assert_eq!(names.len(), LOOKS.len(), "names must differ; they identify the scenes");
    }

    fn mixed_show() -> Show {
        let mut show = Show::default();
        show.add_device(Device::wled("strip", "127.0.0.1", 4048, 60), None);
        show.add_device(Device::wled("matrix", "127.0.0.2", 4048, 64), Some((8, 8)));
        show
    }

    #[test]
    fn looks_become_scenes_with_strip_and_matrix_effects_where_they_belong() {
        let mut show = mixed_show();
        let (strip, matrix) = (show.fixtures[0].id, show.fixtures[1].id);
        assert_eq!(show.add_looks(), LOOKS.len());
        assert_eq!(show.add_looks(), 0, "adding them again adds nothing");
        assert_eq!(show.scenes.len(), LOOKS.len());
        // The first ten scenes take the number keys, as scenes always do.
        assert_eq!(show.bindings.len(), LOOKS.len().min(10));

        let spectrum = show.scenes.iter().find(|s| s.name == "Spectrum").unwrap();
        let on = |effect: &str| &spectrum.layers.iter().find(|l| l.effect == effect).unwrap().targets;
        assert_eq!(on("scroll"), &vec![strip]);
        assert_eq!(on("spectrum"), &vec![matrix]);
        assert!(show.validate().is_ok());
        let mut mended = show.clone();
        mended.repair();
        assert_eq!(mended, show);
        // Ids are all different.
        let mut ids: Vec<u32> = show.scenes.iter().flat_map(|s| s.layers.iter().map(|l| l.id)).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    #[test]
    fn in_a_show_of_only_strips_every_look_still_lights_them() {
        let mut show = Show::default();
        show.add_device(Device::wled("strip", "127.0.0.1", 4048, 60), None);
        show.add_looks();
        let loud = AudioFeatures {
            level: 0.9,
            bass: 0.9,
            mid: 0.7,
            treble: 0.7,
            bands: [0.8; crate::audio::BANDS],
            onset: 1.0,
            onsets: [1.0; 3],
            beat: 1.0,
            bpm: 120.0,
            active: true,
            ..AudioFeatures::default()
        };
        for scene in show.scenes.clone() {
            assert!(
                scene.layers.iter().all(|l| l.targets.is_empty()),
                "{}: with one kind of light, everything plays everywhere",
                scene.name
            );
            assert_eq!(scene.layers[0].blend, Blend::Normal, "{}", scene.name);
            show.recall_scene(scene.id);
            let mut r = Renderer::new();
            r.sync(&show);
            // Half a second of loud music with a beat in it.
            let mut brightest = 0.0f32;
            for frame in 0..30 {
                let audio = AudioFeatures {
                    beat_count: 1 + frame / 15,
                    beat_phase: (frame % 15) as f32 / 15.0,
                    ..loud
                };
                for px in r.render(&show, frame as f64 / 60.0, 1.0 / 60.0, &audio) {
                    assert!(px.r.is_finite() && px.g.is_finite() && px.b.is_finite());
                    brightest = brightest.max(px.r).max(px.g).max(px.b);
                }
            }
            assert!(brightest > 0.3, "{} stayed dark: {brightest}", scene.name);
        }
        // Every setting a look stores is one its effect reads.
        for scene in &show.scenes {
            for layer in &scene.layers {
                let def = effects::find(&layer.effect).unwrap();
                let reader = ParamReader::new(&def.info.params, &layer.params);
                for spec in &def.info.params {
                    if let ParamKind::Float { .. } = spec.kind {
                        assert!(reader.float(spec.id).is_finite());
                    }
                }
            }
        }
    }
}
