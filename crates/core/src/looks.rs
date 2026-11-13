//! Starter scenes: ready-made looks, each a few effects that work
//! together, so a new show has something worth watching before the user
//! has built anything.
//!
//! A look becomes an ordinary scene when it is added. From then on it is
//! the user's to change or delete.

use crate::audio::Band;
use crate::effects;
use crate::params::{ParamValue, Params};
use crate::show::{Blend, Kind, LINK_STRENGTH, Layer, Link, Mirror, Mood, Only, Show, Sound, Source};

pub struct LookLayer {
    pub effect: &'static str,
    pub params: &'static [(&'static str, Value)],
    pub band: Band,
    pub blend: Blend,
    pub opacity: f32,
    /// Strips only, matrices only, or everything.
    pub only: Only,
    /// On each light by itself, not once across all of them.
    pub each: bool,
    /// Sliders that follow the music: the setting, what it follows, and
    /// by how much of its range.
    pub links: &'static [(&'static str, Source, f32)],
    /// The pattern folded on itself, wrapped into wedges, or set turning.
    pub mirror: Mirror,
    pub kaleido: u8,
    pub spin: f32,
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
    /// The parts of a song it suits, for autopilot.
    pub mood: Mood,
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
        only: Only::Any,
        each: false,
        links: &[],
        mirror: Mirror::Off,
        kaleido: 0,
        spin: 0.0,
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

/// Every starter look. Which of them a show gets, and in what order,
/// depends on its kind: see [`Kind::looks`]. Each has something that moves
/// without sound, so none sits dark when the music stops.
pub const LOOKS: &[Look] = &[
    Look {
        name: "Pulse",
        description: "A new colour on every beat, with sparks on the highs.",
        mood: Mood::Big,
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
                // More sparks in the big parts of a song.
                links: &[("density", Source::Energy, 0.4)],
                ..layer("sparkle", &[("colour", T("#ffffff")), ("density", N(0.3))])
            },
        ],
    },
    Look {
        name: "Spectrum",
        description: "The music's spectrum on matrices, its colours running along strips.",
        mood: Mood::Any,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.18,
                // The glow underneath swells with the bass.
                links: &[(LINK_STRENGTH, Source::Bass, 0.25)],
                ..layer("plasma", &[("palette", T("ocean")), ("speed", N(0.2))])
            },
            LookLayer {
                only: Only::Strips,
                ..each("scroll", &[("speed", N(1.2))])
            },
            LookLayer {
                only: Only::Matrices,
                ..each("spectrum", &[("palette", T("rainbow"))])
            },
            LookLayer {
                band: Band::Bass,
                opacity: 0.5,
                only: Only::Matrices,
                ..each("ripple", &[("palette", T("ocean")), ("speed", N(1.4))])
            },
        ],
    },
    Look {
        name: "Neon chase",
        description: "Blocks stepping with the beat over a slow neon glow.",
        mood: Mood::Big,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.35,
                ..layer("plasma", &[("palette", T("neon")), ("speed", N(0.35))])
            },
            LookLayer {
                only: Only::Strips,
                ..each("chase", &[("palette", T("candy")), ("blocks", N(5.0))])
            },
            LookLayer {
                only: Only::Matrices,
                ..each("sweep", &[("palette", T("neon")), ("time", N(0.4))])
            },
        ],
    },
    Look {
        name: "Fire",
        description: "Flames on every light that leap with the bass.",
        mood: Mood::Any,
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
        mood: Mood::Any,
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
        mood: Mood::Big,
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
        mood: Mood::Calm,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                // Drifts a little faster as the music gets louder.
                links: &[("speed", Source::Level, 0.1)],
                ..layer("plasma", &[("palette", T("sunset")), ("speed", N(0.2))])
            },
            LookLayer {
                opacity: 0.5,
                ..layer("sparkle", &[("colour", T("#ffe0b0")), ("density", N(0.08))])
            },
        ],
    },
    Look {
        name: "Aurora",
        description: "Slow green and violet light, with a few stars.",
        mood: Mood::Calm,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.7,
                // Swells and settles by itself, music or not.
                links: &[(LINK_STRENGTH, Source::Wave { beats: 32.0 }, 0.3)],
                ..layer("plasma", &[("palette", T("aurora")), ("speed", N(0.15))])
            },
            LookLayer {
                opacity: 0.6,
                ..layer(
                    "twinkle",
                    &[("palette", T("ice")), ("amount", N(0.12)), ("glow", N(0.0))],
                )
            },
        ],
    },
    // For a house, a garden or a window.
    Look {
        name: "Fairy lights",
        description: "A string of coloured bulbs, each slowly brightening and dimming.",
        mood: Mood::Calm,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.2,
                ..layer("plasma", &[("palette", T("warm")), ("speed", N(0.1))])
            },
            layer("twinkle", &[("palette", T("fairy")), ("amount", N(0.7))]),
        ],
    },
    Look {
        name: "Candy cane",
        description: "Red and white stripes travelling along every light.",
        mood: Mood::Any,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..each(
                    "stripes",
                    &[("palette", T("candy_cane")), ("count", N(8.0)), ("speed", N(0.6))],
                )
            },
            LookLayer {
                band: Band::Treble,
                opacity: 0.8,
                ..layer("sparkle", &[("colour", T("#ffffff")), ("density", N(0.15))])
            },
        ],
    },
    Look {
        name: "Snowfall",
        description: "Snow drifting down over a deep blue night.",
        mood: Mood::Calm,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.3,
                ..layer("plasma", &[("palette", T("ice")), ("speed", N(0.12))])
            },
            each("snow", &[("amount", N(0.75)), ("speed", N(0.3))]),
        ],
    },
    Look {
        name: "Red and green",
        description: "Blocks of Christmas colours that swap places with the music.",
        mood: Mood::Big,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..each(
                    "steps",
                    &[
                        ("palette", T("christmas")),
                        ("blocks", N(6.0)),
                        ("every", T("2 beats")),
                        ("fade", N(0.3)),
                    ],
                )
            },
            LookLayer {
                band: Band::Treble,
                opacity: 0.7,
                ..layer("sparkle", &[("colour", T("#fff4d6")), ("density", N(0.12))])
            },
        ],
    },
    Look {
        name: "Warm glow",
        description: "Candlelight: warm white that breathes, for when the music is off.",
        mood: Mood::Calm,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..layer("plasma", &[("palette", T("warm")), ("speed", N(0.12))])
            },
            LookLayer {
                opacity: 0.5,
                ..layer(
                    "twinkle",
                    &[("palette", T("warm")), ("amount", N(0.4)), ("speed", N(0.5))],
                )
            },
        ],
    },
    // For a dance floor.
    Look {
        name: "Colour hits",
        description: "Hard blocks of colour that change on every beat.",
        mood: Mood::Big,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..each(
                    "steps",
                    &[("palette", T("neon")), ("blocks", N(4.0)), ("fade", N(0.08))],
                )
            },
            LookLayer {
                band: Band::Treble,
                ..layer("sparkle", &[("colour", T("#ffffff")), ("density", N(0.25))])
            },
        ],
    },
    Look {
        name: "Storm",
        description: "Fast streaks on every hit over blacklight purple.",
        mood: Mood::Big,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.4,
                ..layer("plasma", &[("palette", T("blacklight")), ("speed", N(0.4))])
            },
            each(
                "meteors",
                &[("palette", T("neon")), ("speed", N(2.0)), ("tail", N(0.25))],
            ),
        ],
    },
    // ---- Added in 0.9.0. Not in any kind's starting set: they are
    // offered where scenes are managed.
    Look {
        name: "Lighthouse",
        description: "One beam crossing slowly over a deep blue that breathes.",
        mood: Mood::Calm,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.35,
                ..layer("breathe", &[("palette", T("ocean")), ("period", N(8.0))])
            },
            each(
                "scanner",
                &[("colour", T("#fff3c4")), ("width", N(0.12)), ("speed", N(0.35))],
            ),
        ],
    },
    Look {
        name: "Marquee",
        description: "Theatre bulbs stepping round in warm white, with a twinkle over them.",
        mood: Mood::Any,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                ..each(
                    "marquee",
                    &[("palette", T("warm")), ("spacing", N(3.0)), ("speed", N(4.0))],
                )
            },
            LookLayer {
                opacity: 0.6,
                ..layer("twinkle", &[])
            },
        ],
    },
    Look {
        name: "Fireworks night",
        description: "Bursts on every hit over a dark sky, with sparks along the strips.",
        mood: Mood::Big,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.12,
                ..layer("plasma", &[("palette", T("blacklight")), ("speed", N(0.1))])
            },
            LookLayer {
                only: Only::Matrices,
                ..each("fireworks", &[("palette", T("rainbow")), ("size", N(0.7))])
            },
            LookLayer {
                only: Only::Strips,
                band: Band::Treble,
                ..layer("sparkle", &[("colour", T("#ffe9b0")), ("density", N(0.35))])
            },
        ],
    },
    Look {
        name: "Pinwheel",
        description: "Arms of colour turning in time, pushed brighter by the bass.",
        mood: Mood::Big,
        layers: &[LookLayer {
            blend: Blend::Normal,
            links: &[(LINK_STRENGTH, Source::Bass, 0.3)],
            opacity: 0.75,
            ..each(
                "pinwheel",
                &[("palette", T("candy")), ("arms", N(5.0)), ("speed", N(0.3))],
            )
        }],
    },
    Look {
        name: "Ripples",
        description: "Rings leaving the middle of each light, one a beat.",
        mood: Mood::Any,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.15,
                ..layer("plasma", &[("palette", T("ocean")), ("speed", N(0.15))])
            },
            each("rings", &[("palette", T("neon")), ("rings", N(3.0))]),
        ],
    },
    Look {
        name: "Dance floor",
        description: "Lit tiles that reshuffle on the beat, with a chase round the strips.",
        mood: Mood::Big,
        layers: &[
            LookLayer {
                blend: Blend::Normal,
                only: Only::Matrices,
                ..each("tiles", &[("palette", T("neon")), ("across", N(4.0))])
            },
            LookLayer {
                only: Only::Strips,
                ..each("chase", &[("palette", T("neon"))])
            },
        ],
    },
    Look {
        name: "Equaliser",
        description: "Bars for the music with caps that hang and fall, mirrored on the strips.",
        mood: Mood::Any,
        layers: &[
            // So that it is not dark when the music stops.
            LookLayer {
                blend: Blend::Normal,
                opacity: 0.1,
                ..layer("plasma", &[("palette", T("ocean")), ("speed", N(0.15))])
            },
            LookLayer {
                only: Only::Matrices,
                ..each("peaks", &[("palette", T("rainbow")), ("bars", N(16.0))])
            },
            LookLayer {
                only: Only::Strips,
                mirror: Mirror::Across,
                ..each("meter", &[])
            },
        ],
    },
    Look {
        name: "Deep breath",
        description: "Everything swells and fades together, slowly. For winding down.",
        mood: Mood::Calm,
        layers: &[LookLayer {
            blend: Blend::Normal,
            ..layer("breathe", &[("palette", T("aurora")), ("period", N(10.0))])
        }],
    },
    Look {
        name: "Kaleidoscope",
        description: "A slow pattern folded into six wedges, turning. Best on a matrix.",
        mood: Mood::Any,
        layers: &[LookLayer {
            blend: Blend::Normal,
            kaleido: 6,
            spin: 8.0,
            links: &[(LINK_STRENGTH, Source::Level, 0.2)],
            opacity: 0.8,
            ..each("plasma", &[("palette", T("candy")), ("speed", N(0.5))])
        }],
    },
    Look {
        name: "Heartbeat",
        description: "One colour that thumps with the kick and rests between.",
        mood: Mood::Any,
        layers: &[LookLayer {
            blend: Blend::Normal,
            band: Band::Bass,
            ..layer(
                "beat_pulse",
                &[("palette", T("warm")), ("decay", N(0.3)), ("floor", N(0.05))],
            )
        }],
    },
];

impl Kind {
    /// In the order they are offered.
    pub const ALL: [Kind; 4] = [Kind::Display, Kind::Club, Kind::Room, Kind::Custom];

    /// What the kind is called where a show is started.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Display => "Light show",
            Kind::Club => "Live show",
            Kind::Room => "Room show",
            Kind::Custom => "Blank",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Kind::Display => "A house, a garden, a window. Festive scenes for people passing by.",
            Kind::Club => "A party, a dance floor, a stage. Pads to play the lights by hand, strobes at full speed.",
            Kind::Room => "A desk, a shelf, a bedroom. The lights follow your music by themselves.",
            Kind::Custom => "An empty show. Nothing is set up for you.",
        }
    }

    /// The starter looks a show of this kind gets, in the order they are
    /// added and autopilot plays them.
    pub fn looks(self) -> &'static [&'static str] {
        match self {
            Kind::Room => &[
                "Calm",
                "Aurora",
                "Pulse",
                "Spectrum",
                "Fire",
                "Rain",
                "Neon chase",
                "Sweep",
            ],
            Kind::Display => &[
                "Fairy lights",
                "Candy cane",
                "Snowfall",
                "Red and green",
                "Warm glow",
                "Spectrum",
                "Sweep",
            ],
            Kind::Club => &[
                "Pulse",
                "Neon chase",
                "Colour hits",
                "Sweep",
                "Storm",
                "Spectrum",
                "Fire",
            ],
            Kind::Custom => &["Pulse", "Spectrum", "Neon chase", "Fire", "Rain", "Sweep", "Calm"],
        }
    }
}

/// Finds a starter look by name.
pub fn look(name: &str) -> Option<&'static Look> {
    LOOKS.iter().find(|l| l.name == name)
}

impl Show {
    /// Adds a starter look as a scene and returns the scene's id. Nothing
    /// is added, and `None` returned, if a scene of that name exists.
    pub fn add_look(&mut self, look: &Look) -> Option<u32> {
        if self.scenes.iter().any(|s| s.name == look.name) {
            return None;
        }
        let mut layers = Vec::new();
        for part in look.layers {
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
                targets: Vec::new(),
                enabled: true,
                group: None,
                each: part.each,
                // Kept as a rule, not as a list of lights, so the look
                // lands where it should on lights added later too.
                only: part.only,
                angle: 0.0,
                spin: part.spin,
                turns: false,
                mirror: part.mirror,
                repeat: 1,
                kaleido: part.kaleido,
                spread: 0.0,
                hue: 0.0,
                links: part
                    .links
                    .iter()
                    .map(|(param, source, amount)| Link {
                        param: (*param).to_string(),
                        source: *source,
                        amount: *amount,
                        smooth: 0.0,
                    })
                    .collect(),
            });
        }
        // The bottom layer of what is left must cover what is under it.
        if let Some(first) = layers.first_mut() {
            first.blend = Blend::Normal;
        }
        let id = self.push_scene(look.name, layers, Vec::new());
        if let Some(scene) = self.scenes.last_mut() {
            scene.mood = look.mood;
        }
        Some(id)
    }

    /// Adds the starter looks for this show's kind that are not there
    /// yet. Returns how many were added.
    pub fn add_looks(&mut self) -> usize {
        let names = self.kind.looks();
        names
            .iter()
            .filter_map(|name| look(name))
            .filter(|look| self.add_look(look).is_some())
            .count()
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
                        (ParamKind::Words { max }, Value::Text(t)) => t.chars().count() <= *max,
                        (ParamKind::Picture { .. }, Value::Text(t)) => crate::picture::Picture::parse(t).is_some(),
                        _ => false,
                    };
                    assert!(ok, "{}: {} = {value:?} is not valid for {}", look.name, id, part.effect);
                }
                assert!((0.0..=1.0).contains(&part.opacity));
                for (param, _, amount) in part.links {
                    assert!(
                        crate::show::linkable(part.effect, param),
                        "{}: {} has no slider {param}",
                        look.name,
                        part.effect
                    );
                    assert!((-1.0..=1.0).contains(amount));
                }
            }
        }
        let mut names: Vec<_> = LOOKS.iter().map(|l| l.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LOOKS.len(), "names must differ; they identify the scenes");
        // Every kind of show names looks that exist, none twice, and every look is used somewhere.
        for kind in Kind::ALL {
            let wanted = kind.looks();
            assert!(wanted.len() >= 5, "{kind:?}");
            for name in wanted {
                assert!(look(name).is_some(), "{kind:?} asks for {name}");
                assert_eq!(
                    wanted.iter().filter(|n| *n == name).count(),
                    1,
                    "{kind:?}: {name} twice"
                );
            }
        }
        // The first fifteen each start some kind of show. Those added
        // since are offered where scenes are managed, to any show.
        for l in &LOOKS[..15] {
            assert!(
                Kind::ALL.iter().any(|k| k.looks().contains(&l.name)),
                "{} is never offered",
                l.name
            );
        }
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
        let (strip, matrix) = (show.fixtures[0].clone(), show.fixtures[1].clone());
        let count = show.kind.looks().len();
        assert_eq!(show.add_looks(), count);
        assert_eq!(show.add_looks(), 0, "adding them again adds nothing");
        assert_eq!(show.scenes.len(), count);
        // The first ten scenes take the number keys, as scenes always do.
        assert_eq!(show.bindings.len(), count.min(10));

        let spectrum = show.scenes.iter().find(|s| s.name == "Spectrum").unwrap();
        let layer = |effect: &str| spectrum.layers.iter().find(|l| l.effect == effect).unwrap();
        assert_eq!(layer("scroll").only, Only::Strips);
        assert!(Show::layer_hits(layer("scroll"), &strip) && !Show::layer_hits(layer("scroll"), &matrix));
        assert!(Show::layer_hits(layer("spectrum"), &matrix) && !Show::layer_hits(layer("spectrum"), &strip));
        assert!(Show::layer_hits(layer("plasma"), &matrix) && Show::layer_hits(layer("plasma"), &strip));
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
    fn looks_added_before_the_lights_still_land_where_they_belong() {
        let mut show = Show::default();
        show.add_looks();
        show.add_device(Device::wled("strip", "127.0.0.1", 4048, 60), None);
        show.add_device(Device::wled("matrix", "127.0.0.2", 4048, 64), Some((8, 8)));
        let (strip, matrix) = (show.fixtures[0].clone(), show.fixtures[1].clone());
        let spectrum = show.scenes.iter().find(|s| s.name == "Spectrum").unwrap();
        let layer = |effect: &str| spectrum.layers.iter().find(|l| l.effect == effect).unwrap();
        assert!(Show::layer_hits(layer("scroll"), &strip) && !Show::layer_hits(layer("scroll"), &matrix));
        assert!(Show::layer_hits(layer("spectrum"), &matrix) && !Show::layer_hits(layer("spectrum"), &strip));
    }

    #[test]
    fn every_look_of_every_kind_lights_a_show_of_only_strips_and_one_of_only_matrices() {
        for (kind, matrix) in Kind::ALL.into_iter().flat_map(|k| [(k, false), (k, true)]) {
            lights_up(kind, matrix);
        }
    }

    fn lights_up(kind: Kind, matrix: bool) {
        let mut show = Show {
            kind,
            ..Show::default()
        };
        if matrix {
            show.add_device(Device::wled("matrix", "127.0.0.1", 4048, 256), Some((16, 16)));
        } else {
            show.add_device(Device::wled("strip", "127.0.0.1", 4048, 60), None);
        }
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
            assert!(
                brightest > 0.3,
                "{kind:?}, {}: {} stayed dark: {brightest}",
                if matrix { "matrix" } else { "strip" },
                scene.name
            );
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
