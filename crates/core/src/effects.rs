//! Built-in effects.

use std::sync::OnceLock;

use crate::color::{Pal, Rgb, palette};
use crate::effect::{
    AMBIENT, Effect, EffectDef, EffectInfo, FOLLOWS_SOUND, FlashUse, FrameCtx, HITS_AND_BEATS, HitWatch, hash2, noise2,
    smoothstep,
};
use crate::params::{ParamReader, ParamSpec};

/// The effect a new show starts with. It moves without sound, so a first
/// run never looks broken, and it reacts once music plays.
pub const DEFAULT_EFFECT: &str = "plasma";

pub fn registry() -> &'static [EffectDef] {
    static REGISTRY: OnceLock<Vec<EffectDef>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        // Listed in the order the picker shows them, group by group.
        vec![
            Spectrum::def(),
            Energy::def(),
            Scroll::def(),
            Meter::def(),
            Fire::def(),
            BeatPulse::def(),
            Ripple::def(),
            Sweep::def(),
            Chase::def(),
            Meteors::def(),
            Sparkle::def(),
            Strobe::def(),
        ]
        .into_iter()
        .chain(crate::extra_effects::follows())
        .chain(crate::more_effects::hits_and_beats())
        .chain(crate::extra_effects::hits_and_beats())
        .chain([Plasma::def(), Flow::def()])
        .chain(crate::more_effects::ambient())
        .chain(crate::extra_effects::ambient())
        .chain([Solid::def()])
        .collect()
    })
}

pub fn find(id: &str) -> Option<&'static EffectDef> {
    registry().iter().find(|d| d.info.id == id)
}

// ---------------------------------------------------------------- Plasma

struct Plasma {
    palette: Pal,
    speed: f32,
    scale: f32,
    react: f32,
    phase: f32,
    glow: f32,
}

impl Plasma {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "plasma",
                name: "Plasma",
                description: "Slow drifting colour that brightens and speeds up with the music.",
                category: AMBIENT,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("speed", "Speed", 0.0, 2.0, 0.5),
                    ParamSpec::float("scale", "Detail", 0.3, 4.0, 1.2).advanced(),
                    ParamSpec::float("react", "React to sound", 0.0, 1.0, 0.6).advanced(),
                ],
            },
            make: || {
                Box::new(Plasma {
                    palette: Pal::Borrowed(palette("neon")),
                    speed: 0.5,
                    scale: 1.2,
                    react: 0.6,
                    phase: 0.0,
                    glow: 1.0,
                })
            },
        }
    }
}

impl Effect for Plasma {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.speed = p.float("speed");
        self.scale = p.float("scale");
        self.react = p.float("react");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let a = &ctx.audio;
        let drive = if a.active { a.level } else { 0.0 };
        self.phase += ctx.dt * self.speed * (1.0 + self.react * drive * 3.0);
        // With no sound at all, stay fully lit instead of going dim.
        let target = if a.active {
            (1.0 - self.react) + self.react * (0.3 + 0.7 * a.level)
        } else {
            1.0
        };
        self.glow += (target - self.glow) * (ctx.dt * 12.0).min(1.0);
    }

    fn shade(&self, x: f32, y: f32, _ctx: &FrameCtx) -> Rgb {
        let s = self.scale * std::f32::consts::TAU;
        let v = (x * s + self.phase).sin()
            + (y * s * 0.7 - self.phase * 1.3).sin()
            + ((x + y) * s * 0.6 + self.phase * 0.7).sin();
        self.palette.sample(v / 6.0 + 0.5 + self.phase * 0.05).scale(self.glow)
    }
}

// -------------------------------------------------------------- Spectrum

struct Spectrum {
    palette: Pal,
    mirror: bool,
}

impl Spectrum {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "spectrum",
                name: "Spectrum",
                description: "Bass on one side, treble on the other. Bars on a matrix, brightness on a strip.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "rainbow"),
                    ParamSpec::bool("mirror", "Mirror from centre", false).advanced(),
                ],
            },
            make: || {
                Box::new(Spectrum {
                    palette: Pal::Borrowed(palette("rainbow")),
                    mirror: false,
                })
            },
        }
    }
}

impl Effect for Spectrum {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.mirror = p.bool("mirror");
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let t = if self.mirror { (x - 0.5).abs() * 2.0 } else { x };
        let v = ctx.audio.band_at(t).clamp(0.0, 1.0);
        // Stop short of the palette's end so a cyclic one does not wrap.
        let colour = self.palette.sample(t * 0.85);
        if ctx.flat {
            colour.scale(v * v)
        } else {
            let height = 1.0 - y;
            // Edges chosen so that silence is fully dark, bottom row included.
            colour.scale(smoothstep(v, v - 0.04, height))
        }
    }
}

// ---------------------------------------------------------------- Energy

struct Energy {
    low: Rgb,
    mid: Rgb,
    high: Rgb,
    soft: f32,
}

impl Energy {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "energy",
                name: "Energy",
                description: "Three colours grow from the centre: one each for bass, mids and treble.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::color("low", "Bass colour", "#ff2a2a"),
                    ParamSpec::color("mid", "Mid colour", "#22ff66"),
                    ParamSpec::color("high", "Treble colour", "#3a6bff"),
                    ParamSpec::float("soft", "Soft edges", 0.0, 1.0, 0.3).advanced(),
                ],
            },
            make: || {
                Box::new(Energy {
                    low: Rgb::WHITE,
                    mid: Rgb::WHITE,
                    high: Rgb::WHITE,
                    soft: 0.3,
                })
            },
        }
    }
}

impl Effect for Energy {
    fn configure(&mut self, p: &ParamReader) {
        self.low = p.color("low");
        self.mid = p.color("mid");
        self.high = p.color("high");
        self.soft = p.float("soft");
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let r = ctx.radius(x, y);
        let edge = 0.02 + self.soft * 0.4;
        let blob = |v: f32| {
            let v = v.clamp(0.0, 1.0);
            smoothstep(v, v - edge, r) * (0.4 + 0.6 * v)
        };
        let a = &ctx.audio;
        self.low
            .scale(blob(a.bass))
            .add(self.mid.scale(blob(a.mid * 0.8)))
            .add(self.high.scale(blob(a.treble * 0.6)))
    }
}

// ------------------------------------------------------------ Beat pulse

/// The choices for a length of time that can be counted in beats.
const PACES: [&str; 4] = ["Its own time", "Half a beat", "A beat", "Two beats"];

/// A length of time: the effect's own, in seconds, or so many beats when
/// that was asked for and there is a tempo to count them by.
fn paced(choice: usize, own_seconds: f32, ctx: &FrameCtx) -> f32 {
    let beats = [0.0, 0.5, 1.0, 2.0][choice.min(3)];
    if beats > 0.0 && ctx.audio.bpm > 0.0 {
        beats * 60.0 / ctx.audio.bpm
    } else {
        own_seconds
    }
}

struct BeatPulse {
    palette: Pal,
    pace: usize,
    decay: f32,
    floor: f32,
    trigger: usize,
    envelope: f32,
    hue: f32,
    last_beat: u32,
    armed: bool,
}

impl BeatPulse {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "beat_pulse",
                name: "Beat pulse",
                description: "Flashes a new colour on every beat, then fades.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "candy"),
                    ParamSpec::float("decay", "Fade time (s)", 0.05, 1.5, 0.35),
                    ParamSpec::choice("pace", "Fades over", &PACES),
                    ParamSpec::float("floor", "Resting brightness", 0.0, 0.5, 0.04).advanced(),
                    ParamSpec::choice("trigger", "Flash on", &["Beat", "Any hit"]).advanced(),
                ],
            },
            make: || {
                Box::new(BeatPulse {
                    palette: Pal::Borrowed(palette("candy")),
                    pace: 0,
                    decay: 0.35,
                    floor: 0.04,
                    trigger: 0,
                    envelope: 0.0,
                    hue: 0.0,
                    last_beat: 0,
                    armed: true,
                })
            },
        }
    }
}

impl Effect for BeatPulse {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.decay = p.float("decay");
        self.pace = p.choice("pace");
        self.floor = p.float("floor");
        self.trigger = p.choice("trigger");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let a = &ctx.audio;
        let fired = if self.trigger == 0 {
            let fired = a.beat_count != self.last_beat;
            self.last_beat = a.beat_count;
            fired
        } else {
            // Fire once per hit: wait for the onset to fall before re-arming.
            let fired = self.armed && a.onset > 0.9;
            if fired {
                self.armed = false;
            } else if a.onset < 0.5 {
                self.armed = true;
            }
            fired
        };
        if fired {
            self.envelope = 1.0;
            // Golden-ratio steps never repeat a colour twice in a row.
            self.hue = (self.hue + 0.618_034).fract();
        } else {
            // Counted in beats, a flash is gone just as the next is due.
            let fade = paced(self.pace, self.decay, ctx);
            self.envelope *= (-ctx.dt / fade.max(0.01) * 3.0).exp();
        }
    }

    fn shade(&self, _x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let level = self.floor + self.envelope * (1.0 - self.floor);
        self.palette.sample(self.hue).scale(level)
    }
}

// ----------------------------------------------------------------- Sweep

/// One bar of light on its way across.
struct Bar {
    /// How far it has got, 0 at the start to 1 at the far side.
    at: f32,
    colour: Rgb,
    /// Left to right, right to left, or out from the middle.
    path: usize,
}

struct Sweep {
    palette: Pal,
    pace: usize,
    time: f32,
    width: f32,
    direction: usize,
    any_hit: bool,
    bars: Vec<Bar>,
    hue: f32,
    flip: bool,
    watch: HitWatch,
}

impl Sweep {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "sweep",
                name: "Sweep",
                description: "Each beat sends a bar of light across the lights.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("time", "Crossing time (s)", 0.1, 2.0, 0.45),
                    ParamSpec::choice("pace", "A crossing takes", &PACES),
                    ParamSpec::choice(
                        "direction",
                        "Direction",
                        &["Back and forth", "Left to right", "Right to left", "From the middle"],
                    ),
                    ParamSpec::float("width", "Width", 0.03, 0.5, 0.14).advanced(),
                    ParamSpec::choice("trigger", "New bar on", &["Beat", "Any hit"]).advanced(),
                ],
            },
            make: || {
                Box::new(Sweep {
                    palette: Pal::Borrowed(palette("neon")),
                    pace: 0,
                    time: 0.45,
                    width: 0.14,
                    direction: 0,
                    any_hit: false,
                    bars: Vec::new(),
                    hue: 0.0,
                    flip: false,
                    watch: HitWatch::default(),
                })
            },
        }
    }
}

impl Effect for Sweep {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.time = p.float("time");
        self.pace = p.choice("pace");
        self.width = p.float("width");
        self.direction = p.choice("direction");
        self.any_hit = p.choice("trigger") == 1;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let crossing = paced(self.pace, self.time, ctx);
        for bar in &mut self.bars {
            bar.at += ctx.dt / crossing.max(0.05);
        }
        // A bar is done once its tail has left the far side.
        self.bars.retain(|b| b.at < 1.0 + self.width * 3.0);
        if self.watch.fired(&ctx.audio, self.any_hit) && self.bars.len() < 12 {
            self.hue = (self.hue + 0.618_034).fract();
            self.flip = !self.flip;
            let path = match self.direction {
                0 => usize::from(self.flip),
                1 => 0,
                2 => 1,
                _ => 2,
            };
            let t = if self.palette.cyclic {
                self.hue
            } else {
                0.35 + self.hue * 0.65
            };
            self.bars.push(Bar {
                at: 0.0,
                colour: self.palette.sample(t),
                path,
            });
        }
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let mut out = Rgb::BLACK;
        for bar in &self.bars {
            // How far along the bar's own path this pixel is.
            let along = match bar.path {
                0 => x,
                1 => 1.0 - x,
                _ => (x - 0.5).abs() * 2.0,
            };
            // Behind the bar's front edge: a bright head, then a fading tail.
            let behind = bar.at - along;
            let level = if behind < 0.0 {
                0.0
            } else if behind < self.width {
                1.0
            } else {
                (1.0 - (behind - self.width) / (self.width * 2.0)).max(0.0).powi(2) * 0.6
            };
            out = out.add(bar.colour.scale(level));
        }
        out
    }
}

// ----------------------------------------------------------------- Chase

struct Chase {
    palette: Pal,
    blocks: f32,
    fill: f32,
    sync: usize,
    speed: f32,
    glide: bool,
    /// How far the pattern has moved, in blocks (a block and its gap).
    travelled: f32,
    glow: f32,
}

impl Chase {
    /// Steps per beat for each choice; zero runs at its own speed.
    const PER_BEAT: [f32; 3] = [1.0, 2.0, 0.0];
    /// A step moves the pattern a quarter of the way to the next block.
    /// A whole block per step would land every block where the last one
    /// was, and look like standing still.
    const STEP: f32 = 0.25;

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "chase",
                name: "Chase",
                description: "Blocks of colour that step along in time with the beat.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "candy"),
                    ParamSpec::float("blocks", "Blocks", 1.0, 12.0, 4.0),
                    ParamSpec::choice("sync", "Steps", &["1 per beat", "2 per beat", "Own speed"]),
                    ParamSpec::float("fill", "Block length", 0.1, 0.9, 0.5).advanced(),
                    ParamSpec::float("speed", "Own speed", 0.0, 6.0, 2.0).advanced(),
                    ParamSpec::bool("glide", "Glide between steps", false).advanced(),
                ],
            },
            make: || {
                Box::new(Chase {
                    palette: Pal::Borrowed(palette("candy")),
                    blocks: 4.0,
                    fill: 0.5,
                    sync: 0,
                    speed: 2.0,
                    glide: false,
                    travelled: 0.0,
                    glow: 1.0,
                })
            },
        }
    }
}

impl Effect for Chase {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.blocks = p.float("blocks").round().max(1.0);
        self.fill = p.float("fill");
        self.sync = p.choice("sync");
        self.speed = p.float("speed");
        self.glide = p.bool("glide");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let per_beat = Self::PER_BEAT[self.sync.min(2)];
        if per_beat > 0.0 && ctx.audio.bpm > 0.0 {
            // In time: one whole step per beat (or half beat). Without
            // glide the move happens in the first part of the step, so it
            // reads as a step and not as a crawl.
            let steps = ctx.beats * per_beat as f64;
            let within = steps.fract() as f32;
            let eased = if self.glide {
                within
            } else {
                smoothstep(0.0, 0.25, within)
            };
            self.travelled = ((steps.floor() % 4096.0) as f32 + eased) * Self::STEP;
        } else {
            self.travelled = (self.travelled + ctx.dt * self.speed * Self::STEP) % 1024.0;
        }
        // Brighter with the music, never dark without it.
        let target = if ctx.audio.active {
            0.45 + 0.55 * ctx.audio.level
        } else {
            1.0
        };
        self.glow += (target - self.glow) * (ctx.dt * 10.0).min(1.0);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let u = x * self.blocks - self.travelled;
        let within = u.rem_euclid(1.0);
        if within > self.fill {
            return Rgb::BLACK;
        }
        // Each block keeps its colour as it travels.
        let block = (u.floor().rem_euclid(self.blocks)) / self.blocks;
        // Soft ends, so a block does not pop on and off a pixel at a time.
        let edge = (within.min(self.fill - within) / 0.08).clamp(0.0, 1.0);
        self.palette.sample(block).scale(self.glow * edge)
    }
}

// ------------------------------------------------------------------ Fire

struct Fire {
    palette: Pal,
    height: f32,
    speed: f32,
    react: f32,
    /// How far the flames have risen, for scrolling the noise.
    rise: f32,
    /// The flames' height right now, following the bass.
    lift: f32,
}

impl Fire {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "fire",
                name: "Fire",
                description: "Flames that leap with the bass. Along a strip, a bed of embers that flares.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "fire"),
                    ParamSpec::float("height", "Height", 0.2, 1.0, 0.6),
                    ParamSpec::float("speed", "Speed", 0.2, 3.0, 1.0).advanced(),
                    ParamSpec::float("react", "React to sound", 0.0, 1.0, 0.7).advanced(),
                ],
            },
            make: || {
                Box::new(Fire {
                    palette: Pal::Borrowed(palette("fire")),
                    height: 0.6,
                    speed: 1.0,
                    react: 0.7,
                    rise: 0.0,
                    lift: 0.6,
                })
            },
        }
    }
}

impl Effect for Fire {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.height = p.float("height");
        self.speed = p.float("speed");
        self.react = p.float("react");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let a = &ctx.audio;
        self.rise = (self.rise + ctx.dt * self.speed * 2.2) % 4096.0;
        // With no sound the fire burns at its set height; with sound it
        // sinks low in the quiet and leaps on the bass.
        let drive = if a.active { a.bass.max(a.level * 0.6) } else { 0.6 };
        let target = self.height * ((1.0 - self.react) + self.react * (0.35 + 1.1 * drive));
        let rate = if target > self.lift { 14.0 } else { 5.0 };
        self.lift += (target - self.lift) * (ctx.dt * rate).min(1.0);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // Two layers of noise drifting upward at different speeds.
        let heat = |sx: f32, sy: f32| {
            0.6 * noise2(sx * 5.0, sy * 4.0 + self.rise) + 0.4 * noise2(sx * 11.0 + 31.0, sy * 9.0 + self.rise * 1.7)
        };
        if ctx.flat {
            // Embers: no up or down, so the flames become how hot each spot is.
            let glow = heat(x * 2.0, 0.0) * (0.45 + self.lift);
            return self.palette.sample(glow.clamp(0.0, 0.95)).scale((glow * 1.3).min(1.0));
        }
        // 0 at the bottom, 1 at the top.
        let up = 1.0 - y;
        // Hot at the base, cooling with height; the noise tears the top edge.
        let flame = (1.0 - up / self.lift.max(0.05)) * 0.75 + (heat(x, y) - 0.5) * 0.9;
        if flame <= 0.0 {
            return Rgb::BLACK;
        }
        self.palette.sample(flame.min(0.95)).scale((flame * 1.6).min(1.0))
    }
}

// --------------------------------------------------------------- Meteors

struct Meteor {
    /// The head's place along its path; it starts before 0 and ends past 1.
    head: f32,
    /// Across the lights, 0..1. Only matters where there is a second dimension.
    lane: f32,
    colour: Rgb,
    backward: bool,
}

struct Meteors {
    palette: Pal,
    speed: f32,
    tail: f32,
    any_hit: bool,
    direction: usize,
    meteors: Vec<Meteor>,
    count: u32,
    watch: HitWatch,
}

impl Meteors {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "meteors",
                name: "Meteors",
                description: "Streaks with fading tails, set off by hits. They fall on a matrix and run along a strip.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "ice"),
                    ParamSpec::float("speed", "Speed", 0.2, 3.0, 1.1),
                    ParamSpec::float("tail", "Tail length", 0.05, 0.8, 0.35),
                    ParamSpec::choice("trigger", "New ones on", &["Any hit", "Beat"]).advanced(),
                    ParamSpec::choice("direction", "Direction", &["Forward", "Backward", "Both"]).advanced(),
                ],
            },
            make: || {
                Box::new(Meteors {
                    palette: Pal::Borrowed(palette("ice")),
                    speed: 1.1,
                    tail: 0.35,
                    any_hit: true,
                    direction: 0,
                    meteors: Vec::new(),
                    count: 0,
                    watch: HitWatch::default(),
                })
            },
        }
    }
}

impl Effect for Meteors {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.speed = p.float("speed");
        self.tail = p.float("tail");
        self.any_hit = p.choice("trigger") == 0;
        self.direction = p.choice("direction");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        for m in &mut self.meteors {
            m.head += ctx.dt * self.speed;
        }
        self.meteors.retain(|m| m.head < 1.0 + self.tail);
        if self.watch.fired(&ctx.audio, self.any_hit) {
            // One along a strip; a few side by side where there is room.
            let batch = if ctx.flat { 1 } else { 3 };
            for _ in 0..batch {
                if self.meteors.len() >= 24 {
                    break;
                }
                self.count = self.count.wrapping_add(1);
                let colour = self.palette.sample(0.45 + 0.5 * hash2(self.count, 7));
                self.meteors.push(Meteor {
                    head: -0.02 - 0.15 * hash2(self.count, 3),
                    lane: hash2(self.count, 11),
                    colour,
                    backward: match self.direction {
                        0 => false,
                        1 => true,
                        _ => self.count.is_multiple_of(2),
                    },
                });
            }
        }
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let mut out = Rgb::BLACK;
        for m in &self.meteors {
            // Along a strip they run its length; on a matrix they fall.
            let (along, across) = if ctx.flat {
                (x, 1.0)
            } else {
                // Wide enough to light a full column of a small matrix.
                (y, (1.5 - (x - m.lane).abs() / 0.05).clamp(0.0, 1.0))
            };
            if across <= 0.0 {
                continue;
            }
            let along = if m.backward { 1.0 - along } else { along };
            let behind = m.head - along;
            if behind < 0.0 || behind > self.tail {
                continue;
            }
            let fade = 1.0 - behind / self.tail;
            // A white-hot head on a coloured tail.
            let head = (1.0 - behind / 0.03).max(0.0);
            let colour = m.colour.lerp(Rgb::WHITE, head * 0.8);
            out = out.add(colour.scale(fade.powf(1.5) * across));
        }
        out
    }
}

// ---------------------------------------------------------------- Strobe

struct Strobe {
    colour: Rgb,
    rate: f32,
    sync: usize,
    on_time: f32,
    /// Position in the flash cycle, 0..1.
    phase: f32,
    lit: bool,
}

impl Strobe {
    /// Flashes per beat for each "In time with the music" choice.
    const PER_BEAT: [f32; 4] = [0.0, 1.0, 2.0, 4.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "strobe",
                name: "Strobe",
                description: "Hard flashes at a steady rate, or in time with the music.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::Capped,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ffffff"),
                    ParamSpec::float("rate", "Flashes per second", 1.0, 20.0, 6.0),
                    ParamSpec::choice(
                        "sync",
                        "In time with the music",
                        &["Off", "1 per beat", "2 per beat", "4 per beat"],
                    ),
                    ParamSpec::float("on_time", "Length of each flash", 0.05, 0.9, 0.3).advanced(),
                ],
            },
            make: || {
                Box::new(Strobe {
                    colour: Rgb::WHITE,
                    rate: 6.0,
                    sync: 0,
                    on_time: 0.3,
                    phase: 0.0,
                    lit: false,
                })
            },
        }
    }
}

impl Effect for Strobe {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.rate = p.float("rate");
        self.sync = p.choice("sync");
        self.on_time = p.float("on_time");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let mut per_beat = Self::PER_BEAT[self.sync.min(3)];
        if per_beat > 0.0 && ctx.audio.bpm > 0.0 {
            // Under a flash limit, flash on every second or fourth beat
            // rather than faster than allowed: still in time, still crisp.
            let beats_a_second = ctx.audio.bpm / 60.0;
            while per_beat * beats_a_second > ctx.flash_cap && per_beat > 1.0 / 16.0 {
                per_beat /= 2.0;
            }
            // Locked to the beat: a flash starts exactly on it.
            self.phase = (ctx.beats * per_beat as f64).fract() as f32;
        } else {
            // With no tempo to lock to, the steady rate stands in.
            self.phase = (self.phase + ctx.dt * self.rate.min(ctx.flash_cap)).fract();
        }
        self.lit = self.phase < self.on_time;
    }

    fn shade(&self, _x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        if self.lit { self.colour } else { Rgb::BLACK }
    }
}

// ---------------------------------------------------------------- Scroll

const SCROLL_LEN: usize = 512;

struct Scroll {
    low: Rgb,
    mid: Rgb,
    high: Rgb,
    speed: f32,
    origin: usize,
    history: Vec<Rgb>,
    head: usize,
    pending: f32,
    last: Rgb,
}

impl Scroll {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "scroll",
                name: "Scroll",
                description: "The sound's colour is fed in at one point and travels along the lights.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::color("low", "Bass colour", "#ff3d2e"),
                    ParamSpec::color("mid", "Mid colour", "#2eff8a"),
                    ParamSpec::color("high", "Treble colour", "#3d7bff"),
                    ParamSpec::float("speed", "Speed", 0.1, 3.0, 1.0),
                    ParamSpec::choice("origin", "Starts at", &["Centre", "Left", "Right"]).advanced(),
                ],
            },
            make: || {
                Box::new(Scroll {
                    low: Rgb::WHITE,
                    mid: Rgb::WHITE,
                    high: Rgb::WHITE,
                    speed: 1.0,
                    origin: 0,
                    history: vec![Rgb::BLACK; SCROLL_LEN],
                    head: 0,
                    pending: 0.0,
                    last: Rgb::BLACK,
                })
            },
        }
    }
}

impl Effect for Scroll {
    fn configure(&mut self, p: &ParamReader) {
        self.low = p.color("low");
        self.mid = p.color("mid");
        self.high = p.color("high");
        self.speed = p.float("speed");
        self.origin = p.choice("origin");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let a = &ctx.audio;
        let now = self
            .low
            .scale(a.bass.clamp(0.0, 1.0).powi(2))
            .add(self.mid.scale(a.mid.clamp(0.0, 1.0).powi(2)))
            .add(self.high.scale(a.treble.clamp(0.0, 1.0).powi(2)))
            .clamp();
        // A full pass along the lights takes about two seconds at speed 1.
        self.pending += ctx.dt * self.speed * SCROLL_LEN as f32 / 2.0;
        let steps = (self.pending as usize).min(SCROLL_LEN);
        self.pending -= steps as f32;
        for i in 0..steps {
            self.head = (self.head + 1) % SCROLL_LEN;
            self.history[self.head] = self.last.lerp(now, (i + 1) as f32 / steps as f32);
        }
        if steps > 0 {
            self.last = now;
        }
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let d = match self.origin {
            0 => ctx.radius(x, y).min(1.0),
            1 => x,
            _ => 1.0 - x,
        };
        let f = d.clamp(0.0, 1.0) * (SCROLL_LEN - 1) as f32;
        let i = f as usize;
        let at = |k: usize| self.history[(self.head + SCROLL_LEN - k.min(SCROLL_LEN - 1)) % SCROLL_LEN];
        at(i).lerp(at(i + 1), f - i as f32)
    }
}

// ---------------------------------------------------------------- Ripple

struct Ring {
    radius: f32,
    colour: Rgb,
}

struct Ripple {
    palette: Pal,
    speed: f32,
    width: f32,
    trigger: usize,
    rings: Vec<Ring>,
    hue: f32,
    last_beat: u32,
    armed: bool,
}

impl Ripple {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "ripple",
                name: "Ripple",
                description: "Each beat sends a ring of colour out from the centre.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "ocean"),
                    ParamSpec::float("speed", "Speed", 0.2, 3.0, 1.0),
                    ParamSpec::float("width", "Ring width", 0.02, 0.5, 0.12).advanced(),
                    ParamSpec::choice("trigger", "New ring on", &["Beat", "Any hit"]).advanced(),
                ],
            },
            make: || {
                Box::new(Ripple {
                    palette: Pal::Borrowed(palette("ocean")),
                    speed: 1.0,
                    width: 0.12,
                    trigger: 0,
                    rings: Vec::new(),
                    hue: 0.0,
                    last_beat: 0,
                    armed: true,
                })
            },
        }
    }
}

impl Effect for Ripple {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.speed = p.float("speed");
        self.width = p.float("width");
        self.trigger = p.choice("trigger");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let a = &ctx.audio;
        let fired = if self.trigger == 0 {
            let fired = a.beat_count != self.last_beat;
            self.last_beat = a.beat_count;
            fired
        } else {
            let fired = self.armed && a.onset > 0.9;
            if fired {
                self.armed = false;
            } else if a.onset < 0.5 {
                self.armed = true;
            }
            fired
        };
        for ring in &mut self.rings {
            ring.radius += ctx.dt * self.speed;
        }
        self.rings.retain(|r| r.radius < 1.6);
        if fired && self.rings.len() < 16 {
            self.hue = (self.hue + 0.27).fract();
            // Skip the dark end of non-cyclic palettes.
            let t = if self.palette.cyclic {
                self.hue
            } else {
                0.35 + self.hue * 0.65
            };
            self.rings.push(Ring {
                radius: 0.0,
                colour: self.palette.sample(t),
            });
        }
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let r = ctx.radius(x, y);
        let mut out = Rgb::BLACK;
        for ring in &self.rings {
            let d = (r - ring.radius) / self.width;
            let fade = (1.0 - ring.radius / 1.6).max(0.0);
            out = out.add(ring.colour.scale((-d * d).exp() * fade));
        }
        out
    }
}

// --------------------------------------------------------------- Sparkle

struct Sparkle {
    colour: Rgb,
    density: f32,
    rate: f32,
    react: bool,
    live_density: f32,
}

impl Sparkle {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "sparkle",
                name: "Sparkle",
                description: "Random twinkles. More of them when the sound is busy.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ffffff"),
                    ParamSpec::float("density", "Amount", 0.0, 1.0, 0.25),
                    ParamSpec::float("rate", "Twinkle speed", 1.0, 20.0, 6.0).advanced(),
                    ParamSpec::bool("react", "React to sound", true).advanced(),
                ],
            },
            make: || {
                Box::new(Sparkle {
                    colour: Rgb::WHITE,
                    density: 0.25,
                    rate: 6.0,
                    react: true,
                    live_density: 0.25,
                })
            },
        }
    }
}

impl Effect for Sparkle {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.density = p.float("density");
        self.rate = p.float("rate");
        self.react = p.bool("react");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let a = &ctx.audio;
        self.live_density = if self.react && a.active {
            self.density * (0.15 + 1.6 * a.level + 0.6 * a.onset)
        } else {
            self.density
        }
        .clamp(0.0, 1.0);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // Each small cell twinkles on its own schedule.
        let cell = ((x * 2048.0) as u32).wrapping_mul(4099) ^ ((y * 512.0) as u32).wrapping_mul(131);
        let t = ctx.time as f32 * self.rate + hash2(cell, 7) * 16.0;
        let slot = t.floor();
        if hash2(cell, slot as u32) >= self.live_density * 0.6 {
            return Rgb::BLACK;
        }
        let phase = t - slot;
        self.colour.scale(1.0 - (phase * 2.0 - 1.0).abs())
    }
}

// ----------------------------------------------------------------- Meter

struct Meter {
    palette: Pal,
    hold: bool,
    peak: f32,
}

impl Meter {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "meter",
                name: "Level meter",
                description: "A bar that grows with loudness, with a marker that hangs at the peak.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "fire"),
                    ParamSpec::bool("hold", "Peak marker", true).advanced(),
                ],
            },
            make: || {
                Box::new(Meter {
                    palette: Pal::Borrowed(palette("fire")),
                    hold: true,
                    peak: 0.0,
                })
            },
        }
    }
}

impl Effect for Meter {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.hold = p.bool("hold");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let level = ctx.audio.level.clamp(0.0, 1.0);
        self.peak = (self.peak - ctx.dt * 0.35).max(level);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // Run along the longer side: upward on tall layouts, rightward otherwise.
        let pos = if !ctx.flat && ctx.aspect < 1.0 { 1.0 - y } else { x };
        let level = ctx.audio.level.clamp(0.0, 1.0);
        let bar = self
            .palette
            .sample(0.25 + pos * 0.75)
            .scale(smoothstep(level, level - 0.02, pos));
        if self.hold && self.peak > 0.02 && (pos - self.peak).abs() < 0.012 {
            Rgb::WHITE
        } else {
            bar
        }
    }
}

// ------------------------------------------------------------------ Flow

struct Flow {
    palette: Pal,
    speed: f32,
    scale: f32,
    dir: (f32, f32),
}

impl Flow {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "flow",
                name: "Colour flow",
                description: "A gradient that slides across the lights. Does not react to sound.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "rainbow"),
                    ParamSpec::float("speed", "Speed", -2.0, 2.0, 0.2),
                    ParamSpec::float("scale", "Repeats", 0.1, 4.0, 1.0).advanced(),
                    ParamSpec::float("angle", "Angle", 0.0, 360.0, 0.0).advanced(),
                ],
            },
            make: || {
                Box::new(Flow {
                    palette: Pal::Borrowed(palette("rainbow")),
                    speed: 0.2,
                    scale: 1.0,
                    dir: (1.0, 0.0),
                })
            },
        }
    }
}

impl Effect for Flow {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.speed = p.float("speed");
        self.scale = p.float("scale");
        let (sin, cos) = p.float("angle").to_radians().sin_cos();
        self.dir = (cos, sin);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let along = (x - 0.5) * self.dir.0 + (y - 0.5) * self.dir.1;
        let t = along * self.scale - ctx.time as f32 * self.speed;
        if self.palette.cyclic {
            self.palette.sample(t)
        } else {
            // Bounce back and forth so the ends of the palette never jump.
            let f = t.rem_euclid(2.0);
            self.palette.sample(if f > 1.0 { 2.0 - f } else { f })
        }
    }
}

// ----------------------------------------------------------------- Solid

struct Solid {
    colour: Rgb,
}

impl Solid {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "solid",
                name: "Solid colour",
                description: "One colour everywhere.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![ParamSpec::color("colour", "Colour", "#ff7a18")],
            },
            make: || Box::new(Solid { colour: Rgb::WHITE }),
        }
    }
}

impl Effect for Solid {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
    }

    fn shade(&self, _x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.colour
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioFeatures, BANDS};
    use crate::params::Params;

    fn ctx(time: f64, audio: AudioFeatures, flat: bool) -> FrameCtx {
        FrameCtx {
            time,
            dt: 1.0 / 60.0,
            audio,
            beats: audio.beat_count as f64 + audio.beat_phase as f64,
            aspect: if flat { 1.0 } else { 2.0 },
            flat,
            flash_cap: f32::INFINITY,
        }
    }

    fn loud(beat_count: u32) -> AudioFeatures {
        AudioFeatures {
            level: 0.8,
            bass: 0.9,
            mid: 0.6,
            treble: 0.5,
            bands: [0.7; BANDS],
            onset: 1.0,
            onsets: [1.0; 3],
            beat: 1.0,
            beat_phase: 0.0,
            bpm: 120.0,
            beat_count,
            active: true,
            ..AudioFeatures::default()
        }
    }

    fn make(id: &str) -> Box<dyn Effect> {
        let def = find(id).unwrap();
        let mut fx = (def.make)();
        fx.configure(&ParamReader::new(&def.info.params, &Params::new()));
        fx
    }

    fn brightest(fx: &dyn Effect, ctx: &FrameCtx) -> f32 {
        let mut max = 0.0f32;
        for i in 0..64 {
            for j in 0..8 {
                let c = fx.shade(i as f32 / 63.0, j as f32 / 7.0, ctx);
                max = max.max(c.r).max(c.g).max(c.b);
            }
        }
        max
    }

    #[test]
    fn a_sweep_and_a_pulse_can_take_their_time_from_the_tempo() {
        // A sweep asked to cross in one beat: at 120 that is half a
        // second, at 60 a whole one, whatever its own time says.
        let crossed = |bpm: f32, pace: &str| {
            let def = find("sweep").unwrap();
            let mut params = Params::new();
            params.insert("pace".into(), crate::params::ParamValue::Text(pace.into()));
            params.insert(
                "direction".into(),
                crate::params::ParamValue::Text("Left to right".into()),
            );
            let mut fx = (def.make)();
            let mut seconds = 0.0;
            for frame in 0..240u32 {
                let audio = AudioFeatures {
                    bpm,
                    // One beat, on the first frame, sets a bar going.
                    beat_count: 1,
                    active: true,
                    ..AudioFeatures::default()
                };
                let c = ctx(frame as f64 / 60.0, audio, true);
                fx.configure(&ParamReader::new(&def.info.params, &params));
                fx.update(&c);
                // Where the bar is: the brightest place along the strip.
                let at = (0..100)
                    .map(|i| i as f32 / 99.0)
                    .max_by(|a, b| {
                        let l = |x: f32| {
                            let p = fx.shade(x, 0.5, &c);
                            p.r + p.g + p.b
                        };
                        l(*a).total_cmp(&l(*b))
                    })
                    .unwrap();
                if at >= 0.9 {
                    seconds = frame as f32 / 60.0;
                    break;
                }
            }
            seconds
        };
        let (fast, slow) = (crossed(120.0, "A beat"), crossed(60.0, "A beat"));
        assert!((0.35..0.6).contains(&fast), "{fast}");
        assert!((0.75..1.1).contains(&slow), "{slow}");
        // On its own time the tempo makes no difference.
        assert_eq!(crossed(120.0, "Its own time"), crossed(60.0, "Its own time"));
        // And with no tempo, a pace in beats falls back to the effect's own time.
        assert_eq!(crossed(0.0, "A beat"), crossed(120.0, "Its own time"));
    }

    #[test]
    fn ids_are_unique_and_default_exists() {
        let ids: Vec<_> = registry().iter().map(|d| d.info.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
        assert!(find(DEFAULT_EFFECT).is_some());
    }

    #[test]
    fn every_effect_gives_finite_colours_with_and_without_sound() {
        for def in registry() {
            for flat in [true, false] {
                let mut fx = make(def.info.id);
                for frame in 0..180u32 {
                    let audio = if frame < 60 {
                        AudioFeatures::default()
                    } else {
                        loud(frame / 30)
                    };
                    let c = ctx(frame as f64 / 60.0, audio, flat);
                    fx.configure(&ParamReader::new(&def.info.params, &Params::new()));
                    fx.update(&c);
                    for i in 0..40 {
                        let p = fx.shade(i as f32 / 39.0, (i % 5) as f32 / 4.0, &c);
                        assert!(
                            p.r.is_finite() && p.g.is_finite() && p.b.is_finite(),
                            "{} gave {p:?}",
                            def.info.id
                        );
                        assert!(p.r >= 0.0 && p.g >= 0.0 && p.b >= 0.0, "{} went negative", def.info.id);
                    }
                }
            }
        }
    }

    #[test]
    fn audio_effects_are_dark_in_silence_and_lit_when_loud() {
        for id in ["spectrum", "energy", "meter"] {
            let mut fx = make(id);
            let quiet = ctx(0.0, AudioFeatures::default(), false);
            fx.update(&quiet);
            assert!(brightest(fx.as_ref(), &quiet) < 0.05, "{id} lit in silence");
            let noisy = ctx(0.1, loud(1), false);
            fx.update(&noisy);
            assert!(brightest(fx.as_ref(), &noisy) > 0.5, "{id} dark when loud");
        }
    }

    #[test]
    fn an_effect_can_use_colours_of_the_users_own() {
        use crate::params::ParamValue;
        let def = find("plasma").unwrap();
        let mut params = Params::new();
        params.insert("palette".into(), ParamValue::Text("#ff0000,#ff0000".into()));
        let mut fx = (def.make)();
        fx.configure(&ParamReader::new(&def.info.params, &params));
        let c = ctx(0.0, AudioFeatures::default(), false);
        fx.update(&c);
        for i in 0..20 {
            let p = fx.shade(i as f32 / 19.0, 0.5, &c);
            assert!(p.r > 0.0 && p.g == 0.0 && p.b == 0.0, "only red, got {p:?}");
        }
        // Something that is neither a palette's name nor a list of colours falls back.
        params.insert("palette".into(), ParamValue::Text("#nonsense".into()));
        fx.configure(&ParamReader::new(&def.info.params, &params));
        let p = fx.shade(0.3, 0.5, &c);
        assert!(p.r.is_finite() && p.g.is_finite());
    }

    #[test]
    fn default_effect_is_lit_in_silence() {
        let mut fx = make(DEFAULT_EFFECT);
        let quiet = ctx(0.0, AudioFeatures::default(), false);
        fx.update(&quiet);
        assert!(brightest(fx.as_ref(), &quiet) > 0.5);
    }

    #[test]
    fn beat_pulse_flashes_on_a_new_beat_then_fades() {
        let mut fx = make("beat_pulse");
        let c = ctx(0.0, loud(1), true);
        fx.update(&c);
        let flash = brightest(fx.as_ref(), &c);
        assert!(flash > 0.6);
        // Same beat number: no retrigger, so it fades.
        for _ in 0..60 {
            fx.update(&c);
        }
        assert!(brightest(fx.as_ref(), &c) < flash * 0.3);
        let next = ctx(1.0, loud(2), true);
        fx.update(&next);
        assert!(brightest(fx.as_ref(), &next) > 0.6);
    }

    #[test]
    fn spectrum_draws_bars_on_a_matrix() {
        let mut fx = make("spectrum");
        let mut audio = loud(1);
        audio.bands = [0.5; BANDS];
        let c = ctx(0.0, audio, false);
        fx.update(&c);
        let lit = |y: f32| {
            let p = fx.shade(0.3, y, &c);
            p.r.max(p.g).max(p.b)
        };
        assert!(lit(0.9) > 0.5, "bottom of the bar should be lit");
        assert!(lit(0.1) < 0.05, "above the bar should be dark");
    }

    #[test]
    fn scroll_moves_colour_away_from_the_origin() {
        let def = find("scroll").unwrap();
        let mut params = Params::new();
        params.insert("origin".into(), crate::params::ParamValue::Text("Left".into()));
        let mut fx = (def.make)();
        fx.configure(&ParamReader::new(&def.info.params, &params));
        let c = ctx(0.0, loud(1), true);
        for _ in 0..6 {
            fx.update(&c);
        }
        let near = fx.shade(0.01, 0.5, &c);
        let far = fx.shade(0.95, 0.5, &c);
        assert!(near.r + near.g + near.b > 0.5);
        assert!(far.r + far.g + far.b < 0.01);
    }
    #[test]
    fn strobe_flashes_at_its_rate_or_on_the_beat() {
        use crate::params::ParamValue;
        let def = find("strobe").unwrap();
        // The most flashes a second the show allows; none to begin with.
        let cap = std::cell::Cell::new(f32::INFINITY);
        // Counts dark-to-lit changes over `seconds` of frames.
        let flashes = |params: &Params, audio_at: &dyn Fn(f64) -> AudioFeatures, seconds: f64| {
            let mut fx = (def.make)();
            fx.configure(&ParamReader::new(&def.info.params, params));
            let (mut count, mut was_lit) = (0, false);
            for frame in 0..(seconds * 60.0) as usize {
                let mut c = ctx(frame as f64 / 60.0, audio_at(frame as f64 / 60.0), true);
                c.flash_cap = cap.get();
                fx.update(&c);
                let lit = fx.shade(0.5, 0.5, &c).r > 0.5;
                if lit && !was_lit {
                    count += 1;
                }
                was_lit = lit;
            }
            count
        };
        let silent = |_: f64| AudioFeatures::default();
        // Six a second by default.
        let free = flashes(&Params::new(), &silent, 5.0);
        assert!((29..=31).contains(&free), "{free} flashes in five seconds");

        // Two per beat at 120 beats a minute is four a second, whatever the rate says.
        let mut synced = Params::new();
        synced.insert("sync".into(), ParamValue::Text("2 per beat".into()));
        let music = |t: f64| AudioFeatures {
            bpm: 120.0,
            beat_count: (t * 2.0) as u32,
            beat_phase: (t * 2.0).fract() as f32,
            ..AudioFeatures::default()
        };
        let on_beat = flashes(&synced, &music, 5.0);
        assert!((19..=21).contains(&on_beat), "{on_beat} flashes in five seconds");
        // With no tempo found, it falls back to the steady rate.
        let fallback = flashes(&synced, &silent, 5.0);
        assert!((29..=31).contains(&fallback), "{fallback}");

        // Under a flash limit of three a second (a cap of 2.7) the strobe
        // slows itself: evenly spaced, never over the limit.
        cap.set(2.7);
        let capped = flashes(&Params::new(), &silent, 10.0);
        assert!(
            (26..=28).contains(&capped),
            "{capped} flashes in ten seconds under the cap"
        );
        // In time with the music it drops to one per beat (two a second),
        // not to something out of time.
        let capped_on_beat = flashes(&synced, &music, 5.0);
        assert!((9..=11).contains(&capped_on_beat), "{capped_on_beat}");
        // A slow strobe is not changed by the cap.
        let mut slow = Params::new();
        slow.insert("rate".into(), ParamValue::Number(2.0));
        let untouched = flashes(&slow, &silent, 5.0);
        assert!((9..=11).contains(&untouched), "{untouched}");
    }
    /// The brightest pixel along a strip, and where it is.
    fn peak_along(fx: &dyn Effect, c: &FrameCtx) -> (f32, f32) {
        (0..200)
            .map(|i| {
                let x = i as f32 / 199.0;
                let px = fx.shade(x, 0.5, c);
                (px.r.max(px.g).max(px.b), x)
            })
            .fold((0.0, 0.0), |best, now| if now.0 > best.0 { now } else { best })
    }

    #[test]
    fn sweep_sends_a_bar_across_on_each_beat() {
        let mut fx = make("sweep");
        let quiet = |beat_count| AudioFeatures {
            beat_count,
            active: true,
            ..AudioFeatures::default()
        };
        let c = ctx(0.0, quiet(0), true);
        fx.update(&c);
        assert_eq!(peak_along(fx.as_ref(), &c).0, 0.0, "dark until a beat");
        // A beat: a bar starts at one end and travels to the other.
        fx.update(&ctx(0.0, quiet(1), true));
        let mut places = Vec::new();
        for frame in 1..=24 {
            let c = ctx(frame as f64 / 60.0, quiet(1), true);
            fx.update(&c);
            let (level, at) = peak_along(fx.as_ref(), &c);
            assert!(level > 0.5, "frame {frame}");
            places.push(at);
        }
        assert!(places[0] > 0.8 || places[0] < 0.2, "it starts at an end: {places:?}");
        let travelled = (places[23] - places[0]).abs();
        assert!(travelled > 0.6, "it crossed most of the way in 0.4 s: {places:?}");
        // Long after, it has gone.
        for frame in 25..120 {
            fx.update(&ctx(frame as f64 / 60.0, quiet(1), true));
        }
        assert_eq!(peak_along(fx.as_ref(), &ctx(2.0, quiet(1), true)).0, 0.0);
    }

    #[test]
    fn chase_moves_one_block_per_beat() {
        let mut fx = make("chase");
        let at_beats = |beats: f64| AudioFeatures {
            bpm: 120.0,
            beat_count: beats.floor() as u32,
            beat_phase: beats.fract() as f32,
            level: 1.0,
            active: true,
            ..AudioFeatures::default()
        };
        // Which of eighty pixels are lit, as a string of marks.
        let picture = |fx: &dyn Effect, c: &FrameCtx| -> String {
            (0..80)
                .map(|i| {
                    let px = fx.shade((i as f32 + 0.5) / 80.0, 0.5, c);
                    if px.r.max(px.g).max(px.b) > 0.2 { '#' } else { '.' }
                })
                .collect()
        };
        // Let the brightness settle, then look late in two beats running.
        let mut c = ctx(0.0, at_beats(8.9), true);
        for _ in 0..60 {
            fx.update(&c);
        }
        let first = picture(fx.as_ref(), &c);
        c = ctx(0.5, at_beats(9.9), true);
        fx.update(&c);
        let second = picture(fx.as_ref(), &c);
        assert_ne!(first, second);
        // Four blocks across eighty pixels are twenty pixels apart; a step
        // is a quarter of that.
        let shifted: String = first.chars().cycle().skip(75).take(80).collect();
        assert_eq!(second, shifted, "one beat later the pattern is five pixels along");
        // Within a beat, after the step, it holds still.
        fx.update(&ctx(0.6, at_beats(9.5), true));
        let held = picture(fx.as_ref(), &ctx(0.6, at_beats(9.5), true));
        fx.update(&ctx(0.7, at_beats(9.8), true));
        assert_eq!(held, picture(fx.as_ref(), &ctx(0.7, at_beats(9.8), true)));
    }

    #[test]
    fn fire_burns_from_the_bottom_and_leaps_with_the_bass() {
        let lit_rows = |bass: f32| {
            let mut fx = make("fire");
            let audio = AudioFeatures {
                bass,
                level: bass,
                active: true,
                ..AudioFeatures::default()
            };
            let mut c = ctx(0.0, audio, false);
            for frame in 0..90 {
                c = ctx(frame as f64 / 60.0, audio, false);
                fx.update(&c);
            }
            // How bright each row is on average, bottom row first.
            (0..10)
                .map(|row| {
                    let y = 1.0 - (row as f32 + 0.5) / 10.0;
                    (0..40).map(|i| fx.shade((i as f32 + 0.5) / 40.0, y, &c).r).sum::<f32>() / 40.0
                })
                .collect::<Vec<f32>>()
        };
        let calm = lit_rows(0.0);
        let loud = lit_rows(1.0);
        assert!(calm[0] > 0.5, "the base always burns: {calm:?}");
        assert!(
            calm[0] > calm[4] && calm[9] < 0.05,
            "hot at the bottom, dark at the top: {calm:?}"
        );
        let height = |rows: &[f32]| rows.iter().filter(|r| **r > 0.15).count();
        assert!(height(&loud) >= height(&calm) + 2, "calm {calm:?} loud {loud:?}");
    }

    #[test]
    fn meteors_start_on_a_hit_run_the_strip_and_leave_a_tail() {
        let mut fx = make("meteors");
        let with_onset = |onset: f32| AudioFeatures {
            onset,
            active: true,
            ..AudioFeatures::default()
        };
        fx.update(&ctx(0.0, with_onset(0.0), true));
        assert_eq!(peak_along(fx.as_ref(), &ctx(0.0, with_onset(0.0), true)).0, 0.0);
        fx.update(&ctx(0.0, with_onset(1.0), true));
        // The hit rings on for a while; it must still be only one meteor.
        let mut heads = Vec::new();
        for frame in 1..=30 {
            let c = ctx(
                frame as f64 / 60.0,
                with_onset(if frame < 6 { 0.95 } else { 0.0 }),
                true,
            );
            fx.update(&c);
            heads.push(peak_along(fx.as_ref(), &c));
        }
        assert!(heads[10].0 > 0.8);
        assert!(heads[29].1 > heads[10].1 + 0.2, "the head moves along: {heads:?}");
        // Behind the head the light fades rather than stops.
        let c = ctx(0.5, with_onset(0.0), true);
        let head = heads[29].1;
        let near = fx.shade(head - 0.05, 0.5, &c);
        let far = fx.shade(head - 0.25, 0.5, &c);
        assert!(near.b > far.b && far.b > 0.0, "near {near:?} far {far:?}");
        assert_eq!(
            fx.shade((head + 0.1).min(1.0), 0.5, &c),
            Rgb::BLACK,
            "nothing ahead of it"
        );
    }

    #[test]
    fn smooth_noise_is_smooth_and_covers_its_range() {
        let mut low = 1.0f32;
        let mut high = 0.0f32;
        for i in 0..2000 {
            let (x, y) = (i as f32 * 0.037, i as f32 * 0.011);
            let v = noise2(x, y);
            assert!((0.0..=1.0).contains(&v));
            assert!((noise2(x + 0.01, y) - v).abs() < 0.05, "a small step is a small change");
            low = low.min(v);
            high = high.max(v);
        }
        assert!(low < 0.2 && high > 0.8, "{low}..{high}");
        // Negative coordinates are as good as positive ones.
        assert!((0.0..=1.0).contains(&noise2(-3.7, -0.2)));
    }
}
