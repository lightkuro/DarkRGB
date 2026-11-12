//! Staples of light shows: a scanner, a marquee, breathing, bars with
//! falling peaks, fireworks, rings, a pinwheel and tiles. They follow the
//! same rules as the effects in `effects.rs`, and most can keep time with
//! the music: given a tempo, their movement is counted in beats.

use crate::audio::BANDS;
use crate::color::{Pal, Rgb, palette};
use crate::effect::{
    AMBIENT, Effect, EffectDef, EffectInfo, FOLLOWS_SOUND, FlashUse, FrameCtx, HITS_AND_BEATS, HitWatch, hash2,
    smoothstep,
};
use crate::params::{ParamReader, ParamSpec};
use std::f32::consts::TAU;

pub(crate) fn follows() -> Vec<EffectDef> {
    vec![Peaks::def()]
}

pub(crate) fn hits_and_beats() -> Vec<EffectDef> {
    vec![Fireworks::def(), Rings::def(), Tiles::def()]
}

pub(crate) fn ambient() -> Vec<EffectDef> {
    vec![Scanner::def(), Marquee::def(), Breathe::def(), Pinwheel::def()]
}

/// A movement that repeats, at its own speed or in time with the music.
#[derive(Default)]
struct Cycle {
    /// How far through the cycle the effect's own clock is, 0..1.
    own: f32,
}

impl Cycle {
    fn advance(&mut self, dt: f32, cycles_a_second: f32) {
        self.own = (self.own + dt * cycles_a_second).rem_euclid(1.0);
    }

    /// Where the cycle is, 0..1. With `beats` above zero and a tempo
    /// found, the music's own count decides, so the movement lands on the
    /// beat; otherwise the effect's own clock does.
    fn at(&self, ctx: &FrameCtx, beats: f32) -> f32 {
        if beats > 0.0 && ctx.audio.bpm > 0.0 {
            (ctx.beats / beats as f64).rem_euclid(1.0) as f32
        } else {
            self.own
        }
    }
}

// --------------------------------------------------------------- Scanner

struct Scanner {
    colour: Rgb,
    width: f32,
    tail: f32,
    speed: f32,
    pace: usize,
    cycle: Cycle,
}

impl Scanner {
    const PACES: [&'static str; 4] = ["Own speed", "1 beat", "2 beats", "4 beats"];
    const BEATS: [f32; 4] = [0.0, 1.0, 2.0, 4.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "scanner",
                name: "Scanner",
                description: "A bar of light going to and fro, trailing a tail.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ff2a2a"),
                    ParamSpec::float("width", "Width", 0.02, 0.5, 0.1),
                    ParamSpec::float("speed", "Speed", 0.1, 4.0, 0.8),
                    ParamSpec::choice("pace", "One crossing takes", &Self::PACES),
                    ParamSpec::float("tail", "Tail", 0.0, 1.0, 0.6).advanced(),
                ],
            },
            make: || {
                Box::new(Scanner {
                    colour: Rgb::new(1.0, 0.16, 0.16),
                    width: 0.1,
                    tail: 0.6,
                    speed: 0.8,
                    pace: 0,
                    cycle: Cycle::default(),
                })
            },
        }
    }
}

impl Effect for Scanner {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.width = p.float("width");
        self.tail = p.float("tail");
        self.speed = p.float("speed");
        self.pace = p.choice("pace");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // There and back is one cycle.
        self.cycle.advance(ctx.dt, self.speed / 2.0);
    }

    fn shade(&self, x: f32, _y: f32, ctx: &FrameCtx) -> Rgb {
        let there_and_back = self.cycle.at(ctx, Self::BEATS[self.pace.min(3)] * 2.0) * 2.0;
        let (at, forwards) = if there_and_back < 1.0 {
            (there_and_back, true)
        } else {
            (2.0 - there_and_back, false)
        };
        let ahead = if forwards { x - at } else { at - x };
        let head = (1.0 - ahead.abs() / (self.width * 0.5).max(1e-4)).max(0.0);
        // The tail hangs behind the bar, fading out.
        let tail = if ahead < 0.0 && self.tail > 0.0 {
            0.6 * (ahead / (self.tail * 0.35)).exp()
        } else {
            0.0
        };
        self.colour.scale(head.max(tail).min(1.0))
    }
}

// --------------------------------------------------------------- Marquee

struct Marquee {
    palette: Pal,
    lights: f32,
    spacing: f32,
    speed: f32,
    pace: usize,
    /// Steps taken on the effect's own clock.
    stepped: f32,
}

impl Marquee {
    const PACES: [&'static str; 4] = ["Own speed", "1 step a beat", "2 steps a beat", "4 steps a beat"];
    const PER_BEAT: [f64; 4] = [0.0, 1.0, 2.0, 4.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "marquee",
                name: "Marquee",
                description: "Every few lights lit, stepping along like the bulbs round a theatre sign.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "warm"),
                    ParamSpec::float("lights", "Bulbs across", 6.0, 120.0, 30.0),
                    ParamSpec::float("spacing", "One lit in every", 2.0, 8.0, 3.0),
                    ParamSpec::float("speed", "Speed", 0.5, 20.0, 5.0),
                    ParamSpec::choice("pace", "Steps", &Self::PACES),
                ],
            },
            make: || {
                Box::new(Marquee {
                    palette: Pal::Borrowed(palette("warm")),
                    lights: 30.0,
                    spacing: 3.0,
                    speed: 5.0,
                    pace: 0,
                    stepped: 0.0,
                })
            },
        }
    }
}

impl Effect for Marquee {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.lights = p.float("lights").round().max(1.0);
        self.spacing = p.float("spacing").round().max(2.0);
        self.speed = p.float("speed");
        self.pace = p.choice("pace");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.stepped = (self.stepped + ctx.dt * self.speed).rem_euclid(self.spacing * 1024.0);
    }

    fn shade(&self, x: f32, _y: f32, ctx: &FrameCtx) -> Rgb {
        let per_beat = Self::PER_BEAT[self.pace.min(3)];
        let step = if per_beat > 0.0 && ctx.audio.bpm > 0.0 {
            (ctx.beats * per_beat).floor() as i64
        } else {
            self.stepped as i64
        };
        let place = x * self.lights;
        let bulb = (place as i64).min(self.lights as i64 - 1);
        if (bulb - step).rem_euclid(self.spacing as i64) != 0 {
            return Rgb::BLACK;
        }
        // A round bulb with a dark gap either side of it.
        let within = (place - bulb as f32 - 0.5).abs() * 2.0;
        let glow = 1.0 - smoothstep(0.55, 1.0, within);
        self.palette.sample((bulb as f32 + 0.5) / self.lights).scale(glow)
    }
}

// --------------------------------------------------------------- Breathe

struct Breathe {
    palette: Pal,
    period: f32,
    depth: f32,
    pace: usize,
    cycle: Cycle,
}

impl Breathe {
    const PACES: [&'static str; 4] = ["Own speed", "4 beats", "8 beats", "16 beats"];
    const BEATS: [f32; 4] = [0.0, 4.0, 8.0, 16.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "breathe",
                name: "Breathe",
                description: "Colour swelling and fading, slowly, like breathing.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "aurora"),
                    ParamSpec::float("period", "Seconds a breath", 1.0, 30.0, 6.0),
                    ParamSpec::choice("pace", "A breath takes", &Self::PACES),
                    ParamSpec::float("depth", "Depth", 0.0, 1.0, 0.85).advanced(),
                ],
            },
            make: || {
                Box::new(Breathe {
                    palette: Pal::Borrowed(palette("aurora")),
                    period: 6.0,
                    depth: 0.85,
                    pace: 0,
                    cycle: Cycle::default(),
                })
            },
        }
    }
}

impl Effect for Breathe {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.period = p.float("period");
        self.depth = p.float("depth");
        self.pace = p.choice("pace");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.cycle.advance(ctx.dt, 1.0 / self.period.max(0.1));
    }

    fn shade(&self, x: f32, _y: f32, ctx: &FrameCtx) -> Rgb {
        let breath = self.cycle.at(ctx, Self::BEATS[self.pace.min(3)]);
        // Fullest at the start of the cycle, so a breath in time with the
        // music is brightest on the first beat.
        let level = 1.0 - self.depth * (0.5 - 0.5 * (breath * TAU).cos());
        // The colour drifts through the palette far more slowly than the breath.
        let drift = (ctx.time * 0.015).rem_euclid(1.0) as f32;
        self.palette
            .sample((drift + x * 0.15).rem_euclid(1.0))
            .scale(level * level)
    }
}

// ----------------------------------------------------------------- Peaks

struct Peaks {
    palette: Pal,
    cap: Rgb,
    bars: usize,
    fall: f32,
    /// How high each bar stands, and where its cap rests.
    levels: [f32; BANDS],
    caps: [f32; BANDS],
    /// Seconds each cap still hangs before it starts to fall.
    hold: [f32; BANDS],
    /// The same for the one bar a strip shows.
    whole: (f32, f32, f32),
}

impl Peaks {
    const HOLD: f32 = 0.25;

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "peaks",
                name: "Peaks",
                description: "Bars for low to high sound, each with a cap that hangs at its peak and then falls.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "rainbow"),
                    ParamSpec::float("bars", "Bars", 4.0, BANDS as f32, 16.0),
                    ParamSpec::float("fall", "Caps fall", 0.1, 3.0, 0.7),
                    ParamSpec::color("cap", "Cap colour", "#ffffff").advanced(),
                ],
            },
            make: || {
                Box::new(Peaks {
                    palette: Pal::Borrowed(palette("rainbow")),
                    cap: Rgb::WHITE,
                    bars: 16,
                    fall: 0.7,
                    levels: [0.0; BANDS],
                    caps: [0.0; BANDS],
                    hold: [0.0; BANDS],
                    whole: (0.0, 0.0, 0.0),
                })
            },
        }
    }

    /// Lets a cap ride up with its bar, hang, then fall.
    fn settle(level: f32, cap: &mut f32, hold: &mut f32, fall: f32, dt: f32) {
        if level >= *cap {
            *cap = level;
            *hold = Self::HOLD;
        } else if *hold > 0.0 {
            *hold -= dt;
        } else {
            *cap = (*cap - fall * dt).max(level);
        }
    }
}

impl Effect for Peaks {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.cap = p.color("cap");
        self.bars = (p.float("bars").round() as usize).clamp(1, BANDS);
        self.fall = p.float("fall");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        for bar in 0..self.bars {
            // Each bar is the loudest of the bands it covers.
            let (from, to) = (
                bar * BANDS / self.bars,
                ((bar + 1) * BANDS / self.bars).max(bar * BANDS / self.bars + 1),
            );
            let level = ctx.audio.bands[from..to.min(BANDS)]
                .iter()
                .fold(0.0f32, |a, b| a.max(*b))
                .clamp(0.0, 1.0);
            self.levels[bar] = level;
            Self::settle(level, &mut self.caps[bar], &mut self.hold[bar], self.fall, ctx.dt);
        }
        let level = ctx.audio.level.clamp(0.0, 1.0);
        self.whole.0 = level;
        Self::settle(level, &mut self.whole.1, &mut self.whole.2, self.fall, ctx.dt);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // A strip is one bar from its start; a matrix has a row of them.
        let (level, cap, height, hue, thick) = if ctx.flat {
            (self.whole.0, self.whole.1, x, x, 0.02)
        } else {
            let bar = ((x * self.bars as f32) as usize).min(self.bars - 1);
            // A sliver of dark between neighbouring bars.
            let within = x * self.bars as f32 - bar as f32;
            if self.bars > 1 && !(0.06..=0.94).contains(&within) {
                return Rgb::BLACK;
            }
            (
                self.levels[bar],
                self.caps[bar],
                1.0 - y,
                (bar as f32 + 0.5) / self.bars as f32,
                0.05,
            )
        };
        if cap > 0.02 && (height - cap).abs() <= thick {
            return self.cap;
        }
        // Strictly below the level, so silence is dark all the way down.
        if height < level {
            return self.palette.sample(hue).scale(0.45 + 0.55 * height);
        }
        Rgb::BLACK
    }
}

// ------------------------------------------------------------- Fireworks

#[derive(Clone, Copy)]
struct Burst {
    x: f32,
    y: f32,
    born: f64,
    /// Where in the palette its colour comes from, and what makes its
    /// sparks differ from another burst's.
    hue: f32,
    seed: u32,
}

struct Fireworks {
    palette: Pal,
    size: f32,
    rate: f32,
    on_hits: bool,
    bursts: Vec<Burst>,
    watch: HitWatch,
    /// Counts up to the next burst the effect sets off by itself.
    due: f32,
    launched: u32,
}

impl Fireworks {
    const LIFE: f32 = 1.5;
    const SPARKS: u32 = 14;
    const MOST: usize = 8;

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "fireworks",
                name: "Fireworks",
                description: "Bursts of sparks that spread, fall and fade. Hits in the music set them off.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "rainbow"),
                    ParamSpec::float("size", "Size", 0.2, 1.0, 0.6),
                    ParamSpec::bool("on_hits", "On hits in the music", true),
                    ParamSpec::float("rate", "By themselves, a second", 0.0, 3.0, 0.4),
                ],
            },
            make: || {
                Box::new(Fireworks {
                    palette: Pal::Borrowed(palette("rainbow")),
                    size: 0.6,
                    rate: 0.4,
                    on_hits: true,
                    bursts: Vec::with_capacity(Self::MOST),
                    watch: HitWatch::default(),
                    due: 0.0,
                    launched: 0,
                })
            },
        }
    }

    fn launch(&mut self, time: f64) {
        self.launched = self.launched.wrapping_add(1);
        let n = self.launched;
        if self.bursts.len() >= Self::MOST {
            self.bursts.remove(0);
        }
        self.bursts.push(Burst {
            x: 0.15 + 0.7 * hash2(n, 3),
            y: 0.15 + 0.45 * hash2(n, 11),
            born: time,
            hue: hash2(n, 23),
            seed: n,
        });
    }
}

impl Effect for Fireworks {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.size = p.float("size");
        self.rate = p.float("rate");
        self.on_hits = p.bool("on_hits");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let life = Self::LIFE as f64;
        self.bursts.retain(|b| ctx.time - b.born < life && ctx.time >= b.born);
        let hit = self.watch.fired(&ctx.audio, true);
        if self.on_hits && hit {
            self.launch(ctx.time);
        }
        self.due += ctx.dt * self.rate;
        if self.due >= 1.0 {
            self.due = 0.0;
            self.launch(ctx.time);
        }
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // Measured in heights of the area, so bursts are round on any shape.
        let wide = if ctx.flat { 1.0 } else { ctx.aspect.max(0.05) };
        let (px, py) = (x * wide, if ctx.flat { 0.5 } else { y });
        let mut out = Rgb::BLACK;
        for burst in &self.bursts {
            let age = (ctx.time - burst.born) as f32;
            let (cx, cy) = (burst.x * wide, if ctx.flat { 0.5 } else { burst.y });
            // Sparks rush out and slow down; the whole burst sinks a little.
            let spread = self.size * 0.42 * (1.0 - (-3.0 * age).exp());
            let sink = if ctx.flat { 0.0 } else { 0.12 * age * age };
            let reach = spread + 0.12;
            if (px - cx).abs() > reach || (py - cy - sink).abs() > reach {
                continue;
            }
            let fade = (1.0 - age / Self::LIFE).max(0.0);
            let colour = self.palette.sample(burst.hue);
            let mut light = 0.0f32;
            // The flash it starts with.
            if age < 0.12 {
                let d = (px - cx).hypot(py - cy);
                light = (1.0 - d / 0.11).max(0.0) * (1.0 - age / 0.12);
            }
            for spark in 0..Self::SPARKS {
                let angle = (spark as f32 + hash2(burst.seed, spark)) / Self::SPARKS as f32 * TAU;
                let far = spread * (0.55 + 0.45 * hash2(burst.seed, spark + 100));
                let (sx, sy) = if ctx.flat {
                    // Along a strip, sparks fly left and right.
                    (cx + far * angle.cos().signum() * (0.4 + 0.6 * angle.cos().abs()), 0.5)
                } else {
                    (cx + far * angle.cos(), cy + far * angle.sin() + sink)
                };
                let d = (px - sx).hypot(py - sy);
                // Wide enough to light an LED of a small matrix as it passes.
                light = light.max((1.0 - d / 0.06).max(0.0).sqrt() * fade.powf(1.5));
            }
            out = out.max(colour.lerp(Rgb::WHITE, 0.25 * light).scale(light.min(1.0)));
        }
        out
    }
}

// ----------------------------------------------------------------- Rings

struct Rings {
    palette: Pal,
    rings: f32,
    speed: f32,
    pace: usize,
    punch: f32,
    cycle: Cycle,
    /// Rings that have gone by on the effect's own clock.
    passed: f32,
}

impl Rings {
    const PACES: [&'static str; 4] = [
        "Own speed",
        "A ring each beat",
        "A ring every 2 beats",
        "A ring every 4 beats",
    ];
    const BEATS: [f32; 4] = [0.0, 1.0, 2.0, 4.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "rings",
                name: "Rings",
                description: "Rings of colour spreading from the middle, thicker when the bass hits.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("rings", "Rings", 1.0, 8.0, 3.0),
                    ParamSpec::float("speed", "Speed", -3.0, 3.0, 0.8),
                    ParamSpec::choice("pace", "Keeps time", &Self::PACES),
                    ParamSpec::float("punch", "Bass makes them thicker", 0.0, 1.0, 0.5).advanced(),
                ],
            },
            make: || {
                Box::new(Rings {
                    palette: Pal::Borrowed(palette("neon")),
                    rings: 3.0,
                    speed: 0.8,
                    pace: 0,
                    punch: 0.5,
                    cycle: Cycle::default(),
                    passed: 0.0,
                })
            },
        }
    }
}

impl Effect for Rings {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.rings = p.float("rings");
        self.speed = p.float("speed");
        self.pace = p.choice("pace");
        self.punch = p.float("punch");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.passed = (self.passed + ctx.dt * self.speed).rem_euclid(4096.0);
        self.cycle.advance(ctx.dt, self.speed);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let beats = Self::BEATS[self.pace.min(3)];
        // How many rings have left the middle so far.
        let gone = if beats > 0.0 && ctx.audio.bpm > 0.0 {
            (ctx.beats / beats as f64).rem_euclid(4096.0) as f32
        } else {
            self.passed
        };
        let at = ctx.radius(x, y) * self.rings - gone;
        let ring = at.floor();
        let within = at - ring;
        let thick = (0.4 + self.punch * 0.5 * ctx.audio.bass.clamp(0.0, 1.0)).min(0.95);
        let glow = 1.0 - smoothstep(thick * 0.6, thick, (within - 0.5).abs() * 2.0);
        // Each ring keeps its colour all the way out.
        self.palette.sample((ring * 0.21).rem_euclid(1.0)).scale(glow)
    }
}

// -------------------------------------------------------------- Pinwheel

struct Pinwheel {
    palette: Pal,
    arms: f32,
    speed: f32,
    twist: f32,
    pace: usize,
    cycle: Cycle,
}

impl Pinwheel {
    const PACES: [&'static str; 4] = [
        "Own speed",
        "A turn every 4 beats",
        "A turn every 8 beats",
        "A turn every 16 beats",
    ];
    const BEATS: [f32; 4] = [0.0, 4.0, 8.0, 16.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "pinwheel",
                name: "Pinwheel",
                description: "Arms of colour turning about the middle, straight or swept into a spiral.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "rainbow"),
                    ParamSpec::float("arms", "Arms", 2.0, 12.0, 4.0),
                    ParamSpec::float("speed", "Speed", -2.0, 2.0, 0.2),
                    ParamSpec::choice("pace", "Keeps time", &Self::PACES),
                    ParamSpec::float("twist", "Twist", -3.0, 3.0, 0.0),
                ],
            },
            make: || {
                Box::new(Pinwheel {
                    palette: Pal::Borrowed(palette("rainbow")),
                    arms: 4.0,
                    speed: 0.2,
                    twist: 0.0,
                    pace: 0,
                    cycle: Cycle::default(),
                })
            },
        }
    }
}

impl Effect for Pinwheel {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.arms = p.float("arms").round().max(1.0);
        self.speed = p.float("speed");
        self.twist = p.float("twist");
        self.pace = p.choice("pace");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.cycle.advance(ctx.dt, self.speed);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let turned = self.cycle.at(ctx, Self::BEATS[self.pace.min(3)]);
        // Round the middle on a matrix; along a strip, the arms become
        // blocks of colour travelling down it.
        let round = if ctx.flat {
            x
        } else {
            let (dx, dy) = ((x - 0.5) * ctx.aspect.max(0.05), y - 0.5);
            dy.atan2(dx) / TAU + self.twist * ctx.radius(x, y) * 0.25
        };
        let at = (round - turned).rem_euclid(1.0) * self.arms;
        let arm = at.floor();
        let within = at - arm;
        let colour = |n: f32| self.palette.sample(n.rem_euclid(self.arms) / self.arms);
        // Neighbouring arms melt into each other over a narrow edge.
        let edge = smoothstep(0.0, 0.12, within);
        colour(arm - 1.0).lerp(colour(arm), edge)
    }
}

// ----------------------------------------------------------------- Tiles

struct Tiles {
    palette: Pal,
    across: f32,
    fill: f32,
    speed: f32,
    pace: usize,
    fade: f32,
    /// Changes made on the effect's own clock: whole ones, and how far
    /// into the current one.
    changed: f32,
}

impl Tiles {
    const PACES: [&'static str; 4] = ["Each beat", "Every 2 beats", "Every 4 beats", "Own speed"];
    const BEATS: [f64; 4] = [1.0, 2.0, 4.0, 0.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "tiles",
                name: "Tiles",
                description: "A floor of coloured tiles that reshuffles on the beat.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("across", "Tiles across", 2.0, 16.0, 6.0),
                    ParamSpec::float("fill", "How many are lit", 0.1, 1.0, 0.6),
                    ParamSpec::choice("pace", "Reshuffles", &Self::PACES),
                    ParamSpec::float("speed", "Own speed", 0.2, 8.0, 2.0).advanced(),
                    ParamSpec::float("fade", "Fade between", 0.0, 1.0, 0.25).advanced(),
                ],
            },
            make: || {
                Box::new(Tiles {
                    palette: Pal::Borrowed(palette("neon")),
                    across: 6.0,
                    fill: 0.6,
                    speed: 2.0,
                    pace: 0,
                    fade: 0.25,
                    changed: 0.0,
                })
            },
        }
    }

    fn tile(&self, id: u32, change: u32) -> Rgb {
        if hash2(id, change) >= self.fill {
            return Rgb::BLACK;
        }
        self.palette.sample(hash2(id, change.wrapping_add(7919)))
    }
}

impl Effect for Tiles {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.across = p.float("across").round().max(1.0);
        self.fill = p.float("fill");
        self.speed = p.float("speed");
        self.pace = p.choice("pace");
        self.fade = p.float("fade");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.changed = (self.changed + ctx.dt * self.speed).rem_euclid(65536.0);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let beats = Self::BEATS[self.pace.min(3)];
        let changes = if beats > 0.0 && ctx.audio.bpm > 0.0 {
            (ctx.beats / beats).rem_euclid(65536.0) as f32
        } else {
            self.changed
        };
        let (change, into) = (changes.floor() as u32, changes.fract());
        // Square tiles: as many rows as fit the height.
        let col = x * self.across;
        let row = if ctx.flat {
            0.0
        } else {
            y * self.across / ctx.aspect.max(0.05)
        };
        let id = (col as u32).wrapping_mul(131).wrapping_add(row as u32);
        // A thin dark line between tiles.
        let from_edge = (col.fract() - 0.5)
            .abs()
            .max(if ctx.flat { 0.0 } else { (row.fract() - 0.5).abs() });
        let grout = 1.0 - smoothstep(0.42, 0.5, from_edge);
        let now = self.tile(id, change);
        let colour = if self.fade > 0.0 && into < self.fade {
            self.tile(id, change.wrapping_sub(1)).lerp(now, into / self.fade)
        } else {
            now
        };
        colour.scale(grout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioFeatures;
    use crate::effects::find;
    use crate::params::{ParamValue, Params};

    fn ctx(time: f64, audio: AudioFeatures, flat: bool) -> FrameCtx {
        FrameCtx {
            time,
            dt: 1.0 / 60.0,
            audio,
            beats: audio.beat_count as f64 + audio.beat_phase as f64,
            aspect: 1.0,
            flat,
            flash_cap: f32::INFINITY,
        }
    }

    fn music(beat_count: u32, beat_phase: f32) -> AudioFeatures {
        AudioFeatures {
            bpm: 120.0,
            beat_count,
            beat_phase,
            active: true,
            ..AudioFeatures::default()
        }
    }

    fn make(id: &str, pairs: &[(&str, ParamValue)]) -> Box<dyn Effect> {
        let def = find(id).unwrap();
        let params: Params = pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        let mut fx = (def.make)();
        fx.configure(&ParamReader::new(&def.info.params, &params));
        fx
    }

    fn text(value: &str) -> ParamValue {
        ParamValue::Text(value.into())
    }

    fn bright(c: Rgb) -> f32 {
        c.r.max(c.g).max(c.b)
    }

    /// Where along a strip the brightest light is.
    fn peak(fx: &dyn Effect, c: &FrameCtx) -> f32 {
        (0..200)
            .map(|i| (i as f32 + 0.5) / 200.0)
            .max_by(|a, b| bright(fx.shade(*a, 0.5, c)).total_cmp(&bright(fx.shade(*b, 0.5, c))))
            .unwrap()
    }

    #[test]
    fn a_scanner_goes_to_and_fro_and_can_cross_once_a_beat() {
        let fx = make("scanner", &[("pace", text("1 beat"))]);
        // In time with the music: across during one beat, back during the next.
        let at = |beat: u32, phase: f32| peak(fx.as_ref(), &ctx(0.0, music(beat, phase), true));
        assert!((at(0, 0.25) - 0.25).abs() < 0.02, "{}", at(0, 0.25));
        assert!((at(0, 0.75) - 0.75).abs() < 0.02);
        assert!((at(1, 0.25) - 0.75).abs() < 0.02, "coming back: {}", at(1, 0.25));
        assert!((at(2, 0.5) - 0.5).abs() < 0.02);
        // The tail is behind it: going right, there is light to its left only.
        let going = ctx(0.0, music(0, 0.5), true);
        assert!(bright(fx.shade(0.4, 0.5, &going)) > 0.05);
        assert_eq!(fx.shade(0.6, 0.5, &going), Rgb::BLACK);
        // By its own clock it moves too, with no music at all.
        let mut fx = make("scanner", &[]);
        let quiet = ctx(0.0, AudioFeatures::default(), true);
        let start = peak(fx.as_ref(), &quiet);
        for _ in 0..30 {
            fx.update(&quiet);
        }
        assert!((peak(fx.as_ref(), &quiet) - start).abs() > 0.2);
    }

    #[test]
    fn a_marquee_lights_one_bulb_in_every_few_and_steps_on_the_beat() {
        let fx = make(
            "marquee",
            &[
                ("lights", ParamValue::Number(12.0)),
                ("spacing", ParamValue::Number(3.0)),
                ("pace", text("1 step a beat")),
            ],
        );
        let lit = |beat: u32| -> Vec<usize> {
            let c = ctx(0.0, music(beat, 0.2), true);
            (0..12)
                .filter(|i| bright(fx.shade((*i as f32 + 0.5) / 12.0, 0.5, &c)) > 0.0)
                .collect()
        };
        assert_eq!(lit(0), [0, 3, 6, 9]);
        assert_eq!(lit(1), [1, 4, 7, 10], "one step along on the next beat");
        assert_eq!(lit(3), lit(0), "and round again");
        // The bulbs between are dark, and a lit one fades towards its edge.
        let c = ctx(0.0, music(0, 0.0), true);
        assert_eq!(fx.shade(1.5 / 12.0, 0.5, &c), Rgb::BLACK);
        assert!(bright(fx.shade(0.5 / 12.0 + 0.04, 0.5, &c)) < bright(fx.shade(0.5 / 12.0, 0.5, &c)) * 0.1);
    }

    #[test]
    fn breathing_swells_and_fades_and_is_fullest_on_the_first_beat() {
        let fx = make("breathe", &[("pace", text("4 beats"))]);
        let level = |beat: u32| bright(fx.shade(0.5, 0.5, &ctx(0.0, music(beat, 0.0), true)));
        assert!(level(0) > 0.3, "{}", level(0));
        assert!(level(2) < level(0) * 0.05, "nearly out halfway: {}", level(2));
        assert!(level(1) > level(2) && level(1) < level(0));
        assert!((level(4) - level(0)).abs() < 1e-3);
        // With no depth it does not breathe at all.
        let fx = make(
            "breathe",
            &[("depth", ParamValue::Number(0.0)), ("pace", text("4 beats"))],
        );
        let level = |beat: u32| bright(fx.shade(0.5, 0.5, &ctx(0.0, music(beat, 0.0), true)));
        assert!((level(0) - level(2)).abs() < 1e-3);
    }

    #[test]
    fn peaks_hang_above_their_bars_then_fall() {
        let mut fx = make("peaks", &[("bars", ParamValue::Number(4.0))]);
        let loud = AudioFeatures {
            bands: [0.8; BANDS],
            level: 0.8,
            ..AudioFeatures::default()
        };
        let c = ctx(0.0, loud, false);
        fx.update(&c);
        // A bar reaches 0.8 of the way up, with its cap on top.
        assert!(bright(fx.shade(0.125, 1.0 - 0.5, &c)) > 0.3, "inside the bar");
        assert_eq!(fx.shade(0.125, 1.0 - 0.8, &c), Rgb::WHITE, "the cap");
        assert_eq!(fx.shade(0.125, 1.0 - 0.95, &c), Rgb::BLACK, "above it");
        // The sound stops: the bar drops at once, the cap hangs.
        let quiet = ctx(0.0, AudioFeatures::default(), false);
        fx.update(&quiet);
        assert_eq!(fx.shade(0.125, 1.0 - 0.5, &quiet), Rgb::BLACK);
        assert_eq!(fx.shade(0.125, 1.0 - 0.8, &quiet), Rgb::WHITE);
        // A second later it has fallen part of the way, and later still it is gone.
        for _ in 0..60 {
            fx.update(&quiet);
        }
        assert_eq!(fx.shade(0.125, 1.0 - 0.8, &quiet), Rgb::BLACK);
        let cap_at = (0..100)
            .map(|i| i as f32 / 100.0)
            .find(|h| fx.shade(0.125, 1.0 - h, &quiet) == Rgb::WHITE);
        assert!(cap_at.is_some_and(|h| h > 0.1 && h < 0.5), "{cap_at:?}");
        for _ in 0..120 {
            fx.update(&quiet);
        }
        assert!((0..100).all(|i| fx.shade(0.125, i as f32 / 100.0, &quiet) == Rgb::BLACK));
        // A strip is one bar from its start.
        let strip = ctx(0.0, loud, true);
        fx.update(&strip);
        assert!(bright(fx.shade(0.3, 0.5, &strip)) > 0.2);
        assert_eq!(fx.shade(0.95, 0.5, &strip), Rgb::BLACK);
    }

    #[test]
    fn fireworks_burst_on_a_hit_spread_and_die_away() {
        let mut fx = make("fireworks", &[("rate", ParamValue::Number(0.0))]);
        let lit = |fx: &dyn Effect, c: &FrameCtx| {
            (0..1600)
                .filter(|i| bright(fx.shade((i % 40) as f32 / 39.0, (i / 40) as f32 / 39.0, c)) > 0.05)
                .count()
        };
        let quiet = ctx(0.0, AudioFeatures::default(), false);
        fx.update(&quiet);
        assert_eq!(lit(fx.as_ref(), &quiet), 0, "nothing by itself at a rate of nothing");
        let hit = AudioFeatures {
            onset: 1.0,
            ..AudioFeatures::default()
        };
        fx.update(&ctx(1.0, hit, false));
        let early = lit(fx.as_ref(), &ctx(1.05, AudioFeatures::default(), false));
        fx.update(&ctx(1.4, AudioFeatures::default(), false));
        let spread = lit(fx.as_ref(), &ctx(1.4, AudioFeatures::default(), false));
        assert!(early > 0, "a flash where it starts");
        assert!(spread > early, "then sparks over a wider area: {early} then {spread}");
        fx.update(&ctx(3.0, AudioFeatures::default(), false));
        assert_eq!(
            lit(fx.as_ref(), &ctx(3.0, AudioFeatures::default(), false)),
            0,
            "and gone"
        );
        // Without hits, and asked to, it lets them off by itself.
        let mut fx = make(
            "fireworks",
            &[("rate", ParamValue::Number(2.0)), ("on_hits", ParamValue::Bool(false))],
        );
        let mut seen = 0;
        for frame in 0..120 {
            let c = ctx(frame as f64 / 60.0, AudioFeatures::default(), true);
            fx.update(&c);
            seen += usize::from((0..60).any(|i| bright(fx.shade(i as f32 / 59.0, 0.5, &c)) > 0.05));
        }
        assert!(seen > 30, "lit on {seen} of 120 frames");
    }

    #[test]
    fn rings_spread_from_the_middle_one_for_each_beat() {
        let fx = make(
            "rings",
            &[("pace", text("A ring each beat")), ("rings", ParamValue::Number(2.0))],
        );
        // Where the brightest ring is, measured from the middle along the right-hand half.
        let ring_at = |beat: u32, phase: f32| {
            let c = ctx(0.0, music(beat, phase), false);
            (0..100)
                .map(|i| i as f32 / 200.0)
                .max_by(|a, b| bright(fx.shade(0.5 + a, 0.5, &c)).total_cmp(&bright(fx.shade(0.5 + b, 0.5, &c))))
                .unwrap()
        };
        let (a, b) = (ring_at(0, 0.1), ring_at(0, 0.4));
        assert!(b > a + 0.05, "it moved outwards: {a} then {b}");
        // A beat later the picture is the same, one ring on.
        let c0 = ctx(0.0, music(4, 0.3), false);
        let c1 = ctx(0.0, music(5, 0.3), false);
        let shape = |c: &FrameCtx| -> Vec<bool> {
            (0..50)
                .map(|i| bright(fx.shade(0.5 + i as f32 / 100.0, 0.5, c)) > 0.5)
                .collect()
        };
        // (All but a light sitting exactly on a ring's edge.)
        let differing = shape(&c0).iter().zip(shape(&c1)).filter(|(a, b)| **a != *b).count();
        assert!(differing <= 1, "{differing} lights differ");
        assert!(shape(&c0).iter().any(|lit| *lit) && shape(&c0).iter().any(|lit| !*lit));
        // A ring is the same all the way round.
        let c = ctx(0.0, music(0, 0.3), false);
        assert_eq!(fx.shade(0.5 + 0.3, 0.5, &c), fx.shade(0.5, 0.5 + 0.3, &c));
    }

    #[test]
    fn a_pinwheel_has_its_arms_and_turns() {
        let mut fx = make("pinwheel", &[("arms", ParamValue::Number(4.0))]);
        let quiet = ctx(0.0, AudioFeatures::default(), false);
        // Four arms: a quarter of the way round, the colour changes; all the way round, it is back.
        let round = |fx: &dyn Effect, turn: f32| {
            let a = turn * TAU;
            fx.shade(0.5 + 0.3 * a.cos(), 0.5 + 0.3 * a.sin(), &quiet)
        };
        let colours: Vec<Rgb> = (0..4).map(|i| round(fx.as_ref(), 0.06 + i as f32 / 4.0)).collect();
        for i in 0..4 {
            assert_ne!(colours[i], colours[(i + 1) % 4], "arm {i}");
        }
        let before = round(fx.as_ref(), 0.06);
        // A quarter turn at 0.2 turns a second takes 1.25 seconds.
        for _ in 0..75 {
            fx.update(&quiet);
        }
        let after = round(fx.as_ref(), 0.06 + 0.25);
        assert!((before.r - after.r).abs() + (before.g - after.g).abs() + (before.b - after.b).abs() < 0.05);
    }

    #[test]
    fn tiles_reshuffle_on_the_beat_and_hold_between() {
        let fx = make("tiles", &[("fade", ParamValue::Number(0.0))]);
        let floor = |beat: u32, phase: f32| -> Vec<Rgb> {
            let c = ctx(0.0, music(beat, phase), false);
            (0..36)
                .map(|i| fx.shade((i % 6) as f32 / 6.0 + 0.08, (i / 6) as f32 / 6.0 + 0.08, &c))
                .collect()
        };
        assert_eq!(floor(3, 0.1), floor(3, 0.9), "still within a beat");
        assert_ne!(floor(3, 0.5), floor(4, 0.5), "changed on the next");
        // About six in ten are lit, and a tile is one colour all over.
        let lit = floor(3, 0.5).iter().filter(|c| bright(**c) > 0.0).count();
        assert!((12..=30).contains(&lit), "{lit} of 36");
        let c = ctx(0.0, music(3, 0.5), false);
        assert_eq!(fx.shade(0.05, 0.05, &c), fx.shade(0.1, 0.1, &c));
        // The line between two tiles is dark.
        assert_eq!(fx.shade(1.0 / 6.0, 0.08, &c), Rgb::BLACK);
    }
}
