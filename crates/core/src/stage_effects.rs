//! Effects added to close the gap with other light-show programs: lights
//! that take turns, beams that sway like moving heads, colour from the
//! tune, and a handful of pictures for matrices. They follow the same
//! rules as the ones in `effects.rs`.

use crate::audio::BANDS;
use crate::color::{Pal, Rgb, palette};
use crate::effect::{
    AMBIENT, Effect, EffectDef, EffectInfo, FOLLOWS_SOUND, FlashUse, FrameCtx, HITS_AND_BEATS, HitWatch, hash2,
    smoothstep,
};
use crate::movie::{self, Movie};
use crate::params::{ParamReader, ParamSpec};
use std::sync::Arc;

pub(crate) fn follows() -> Vec<EffectDef> {
    vec![Notes::def(), Waterfall::def()]
}

pub(crate) fn hits_and_beats() -> Vec<EffectDef> {
    vec![Alternate::def(), Beams::def(), Glitch::def()]
}

pub(crate) fn ambient() -> Vec<EffectDef> {
    vec![Film::def(), Lava::def(), CodeRain::def(), Life::def()]
}

// ------------------------------------------------------------- Alternate

struct Alternate {
    colour: Rgb,
    second: Rgb,
    whole_lights: bool,
    parts: f32,
    pace: usize,
    speed: f32,
    soft: f32,
    swap: bool,
    /// Turns taken on the effect's own clock.
    own: f64,
    /// How many turns have been taken, and how far into this one, as the
    /// frame being drawn sees it.
    turn: i64,
    within: f32,
}

impl Alternate {
    const PACES: [&'static str; 4] = ["Every beat", "Twice a beat", "Every 2 beats", "Own speed"];
    const PER_BEAT: [f32; 4] = [1.0, 2.0, 0.5, 0.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "alternate",
                name: "Take turns",
                description: "Halves of a light, or whole lights, flashing in turn: one on while the other waits.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::Capped,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ff2030"),
                    ParamSpec::color("second", "Other colour", "#2050ff"),
                    ParamSpec::choice("what", "Taking turns", &["Parts of the pattern", "Whole lights"]),
                    ParamSpec::float("parts", "How many take turns", 2.0, 16.0, 2.0),
                    ParamSpec::choice("pace", "A turn lasts", &Self::PACES),
                    ParamSpec::float("speed", "Own speed", 0.2, 10.0, 2.0),
                    ParamSpec::float("soft", "Fade", 0.0, 1.0, 0.0).advanced(),
                    ParamSpec::bool("swap", "The one waiting shows the other colour", false).advanced(),
                ],
            },
            make: || {
                Box::new(Alternate {
                    colour: Rgb::new(1.0, 0.12, 0.19),
                    second: Rgb::new(0.12, 0.31, 1.0),
                    whole_lights: false,
                    parts: 2.0,
                    pace: 0,
                    speed: 2.0,
                    soft: 0.0,
                    swap: false,
                    own: 0.0,
                    turn: 0,
                    within: 0.0,
                })
            },
        }
    }
}

impl Effect for Alternate {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.second = p.color("second");
        self.whole_lights = p.choice("what") == 1;
        self.parts = p.float("parts").round().clamp(2.0, 16.0);
        self.pace = p.choice("pace");
        self.speed = p.float("speed");
        self.soft = p.float("soft");
        self.swap = p.bool("swap");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // Each part is lit once in every round of turns, so that is how
        // often it flashes; the flash limit holds the turns to that.
        let most = ctx.flash_cap * self.parts;
        self.own += (ctx.dt * self.speed.min(most)) as f64;
        let mut per_beat = Self::PER_BEAT[self.pace.min(3)];
        let at = if per_beat > 0.0 && ctx.audio.bpm > 0.0 {
            while per_beat * ctx.audio.bpm / 60.0 > most && per_beat > 1.0 / 16.0 {
                per_beat /= 2.0;
            }
            ctx.beats * per_beat as f64
        } else {
            self.own
        };
        self.turn = at.floor() as i64;
        self.within = (at - at.floor()) as f32;
    }

    fn shade(&self, x: f32, _y: f32, ctx: &FrameCtx) -> Rgb {
        let parts = self.parts as i64;
        let part = if self.whole_lights {
            ctx.light as i64 % parts
        } else {
            ((x * self.parts) as i64).min(parts - 1)
        };
        // The parts are lit one after another, round and round.
        let lit = (self.turn - part).rem_euclid(parts) == 0;
        let own = if part % 2 == 0 { self.colour } else { self.second };
        let other = if part % 2 == 0 { self.second } else { self.colour };
        let waiting = if self.swap { other } else { Rgb::BLACK };
        // With a fade, a part comes up as its turn begins and dies away
        // as it ends.
        let edge = self.soft.clamp(0.0, 1.0) * 0.5;
        let up = if edge > 0.0 {
            smoothstep(0.0, edge, self.within) * (1.0 - smoothstep(1.0 - edge, 1.0, self.within))
        } else {
            1.0
        };
        match (lit, up >= 1.0) {
            (true, true) => own,
            (true, false) => waiting.lerp(own, up),
            (false, _) => waiting,
        }
    }
}

// ----------------------------------------------------------------- Beams

struct Beams {
    palette: Pal,
    count: f32,
    width: f32,
    movement: usize,
    pace: usize,
    speed: f32,
    punch: f32,
    below: bool,
    /// Sweeps made on the effect's own clock.
    own: f64,
    /// Where each beam points, from straight ahead: -1 to 1.
    aim: [f32; 8],
    beat: f32,
}

impl Beams {
    const MOVES: [&'static str; 4] = ["Sway together", "Cross over", "Fan in and out", "Jump on the beat"];
    const PACES: [&'static str; 4] = ["4 beats", "2 beats", "8 beats", "Own speed"];
    const BEATS: [f32; 4] = [4.0, 2.0, 8.0, 0.0];
    /// How far a beam swings to either side, in radians.
    const SWING: f32 = 0.65;

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "beams",
                name: "Stage beams",
                description: "Beams of light that sway across the stage like moving heads, in time with the music.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("count", "Beams", 1.0, 8.0, 4.0),
                    ParamSpec::float("width", "Width", 0.03, 0.6, 0.16),
                    ParamSpec::choice("move", "Movement", &Self::MOVES),
                    ParamSpec::choice("pace", "One sweep takes", &Self::PACES),
                    ParamSpec::float("speed", "Own speed", 0.05, 2.0, 0.3),
                    ParamSpec::float("punch", "Flash on the beat", 0.0, 1.0, 0.3),
                    ParamSpec::choice("from", "Hung", &["Above", "Below"]).advanced(),
                ],
            },
            make: || {
                Box::new(Beams {
                    palette: Pal::Borrowed(palette("neon")),
                    count: 4.0,
                    width: 0.16,
                    movement: 0,
                    pace: 0,
                    speed: 0.3,
                    punch: 0.3,
                    below: false,
                    own: 0.0,
                    aim: [0.0; 8],
                    beat: 0.0,
                })
            },
        }
    }
}

impl Effect for Beams {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.count = p.float("count").round().clamp(1.0, 8.0);
        self.width = p.float("width");
        self.movement = p.choice("move");
        self.pace = p.choice("pace");
        self.speed = p.float("speed");
        self.punch = p.float("punch");
        self.below = p.choice("from") == 1;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.own += (ctx.dt * self.speed) as f64;
        let beats = Self::BEATS[self.pace.min(3)];
        let sweeps = if beats > 0.0 && ctx.audio.bpm > 0.0 {
            ctx.beats / beats as f64
        } else {
            self.own
        };
        let swing = (sweeps.fract() as f32 * std::f32::consts::TAU).sin();
        let n = self.count as usize;
        for i in 0..n {
            let place = if n > 1 {
                i as f32 / (n - 1) as f32 * 2.0 - 1.0
            } else {
                0.0
            };
            self.aim[i] = match self.movement {
                0 => swing,
                1 => {
                    if i % 2 == 0 {
                        swing
                    } else {
                        -swing
                    }
                }
                2 => place * (0.5 - 0.5 * (sweeps.fract() as f32 * std::f32::consts::TAU).cos()),
                _ => {
                    // A new place for every beam on each beat, reached in
                    // the beat's first third.
                    let (beat, within) = if ctx.audio.bpm > 0.0 {
                        (ctx.beats.floor() as i64, ctx.beats.fract() as f32)
                    } else {
                        ((self.own * 4.0).floor() as i64, (self.own * 4.0).fract() as f32)
                    };
                    let aim = |beat: i64| hash2(beat as u32, i as u32 + 7) * 2.0 - 1.0;
                    let glide = smoothstep(0.0, 0.33, within);
                    aim(beat - 1) + (aim(beat) - aim(beat - 1)) * glide
                }
            };
        }
        self.beat = ctx.audio.beat;
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let n = self.count as usize;
        let mut sum = Rgb::BLACK;
        let y = if self.below { 1.0 - y } else { y };
        for i in 0..n {
            let home = (i as f32 + 0.5) / n as f32;
            let colour = self.palette.sample(if n > 1 { i as f32 / n as f32 } else { 0.0 });
            let angle = self.aim[i] * Self::SWING;
            let strength = if ctx.flat {
                // A line of lights shows where each beam lands: a pool of
                // light that slides to and fro.
                let lands = home + angle.tan() * 0.35;
                let away = (x - lands).abs() / (self.width * 0.5).max(1e-3);
                (1.0 - away * away).max(0.0)
            } else {
                // A cone from the lamp: widening as it goes, fading a little.
                let (dx, dy) = ((x - home) * ctx.aspect, y);
                let (sin, cos) = angle.sin_cos();
                let along = dx * sin + dy * cos;
                if along <= 0.0 {
                    0.0
                } else {
                    let across = (dx * cos - dy * sin).abs();
                    let half = self.width * 0.5 * (0.25 + along) * ctx.aspect.max(1.0);
                    let edge = (1.0 - across / half.max(1e-3)).clamp(0.0, 1.0);
                    edge * edge.sqrt() * (1.0 - 0.3 * along.min(1.0))
                }
            };
            sum = sum.add(colour.scale(strength));
        }
        let punch = self.punch.clamp(0.0, 1.0) * 0.6;
        sum.scale(1.0 - punch + punch * self.beat).clamp()
    }
}

// ----------------------------------------------------------------- Notes

/// How many steps of a trail are remembered, end to end.
const TRAIL: usize = 240;

struct Notes {
    mode: usize,
    speed: f32,
    calm: f32,
    loud: bool,
    floor: f32,
    shift: f32,
    /// The colour of the note, as a place round the colour wheel, and how
    /// pure it is: drums alone wash it out.
    hue: f32,
    pure: f32,
    now: Rgb,
    trail: Vec<Rgb>,
    /// Where the newest step of the trail is in `trail`.
    head: usize,
    /// How far towards the next step the trail has moved.
    carry: f32,
}

impl Notes {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "notes",
                name: "Note colours",
                description: "Colour that follows the tune: each note of the scale has its own, so a melody paints the lights.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::choice(
                        "mode",
                        "Shown as",
                        &["A wash", "A trail from the middle", "A trail from one end"],
                    ),
                    ParamSpec::float("speed", "Trail speed", 0.1, 3.0, 0.8),
                    ParamSpec::float("calm", "Smoothness", 0.0, 1.0, 0.3),
                    ParamSpec::bool("loud", "Brightness follows the sound", true),
                    ParamSpec::float("floor", "Glow when quiet", 0.0, 1.0, 0.15).advanced(),
                    ParamSpec::float("shift", "Colour of the note C", 0.0, 360.0, 0.0).advanced(),
                ],
            },
            make: || {
                Box::new(Notes {
                    mode: 0,
                    speed: 0.8,
                    calm: 0.3,
                    loud: true,
                    floor: 0.15,
                    shift: 0.0,
                    hue: 0.0,
                    pure: 0.0,
                    now: Rgb::BLACK,
                    trail: vec![Rgb::BLACK; TRAIL],
                    head: 0,
                    carry: 0.0,
                })
            },
        }
    }
}

impl Effect for Notes {
    fn configure(&mut self, p: &ParamReader) {
        self.mode = p.choice("mode");
        self.speed = p.float("speed");
        self.calm = p.float("calm");
        self.loud = p.bool("loud");
        self.floor = p.float("floor");
        self.shift = p.float("shift");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let audio = &ctx.audio;
        // Glide to the new note the short way round the wheel.
        let seconds = 0.02 + self.calm.clamp(0.0, 1.0) * 0.6;
        let rate = 1.0 - (-ctx.dt / seconds).exp();
        if audio.tonal > 0.05 {
            let gap = (audio.pitch - self.hue + 0.5).rem_euclid(1.0) - 0.5;
            self.hue = (self.hue + gap * rate).rem_euclid(1.0);
        }
        self.pure += (audio.tonal - self.pure) * rate.max(0.05);
        let level = if self.loud { audio.level } else { 1.0 };
        let floor = self.floor.clamp(0.0, 1.0);
        let value = if audio.active || !self.loud {
            floor + (1.0 - floor) * level
        } else {
            0.0
        };
        self.now = Rgb::from_hsv(
            self.hue + self.shift / 360.0,
            0.3 + 0.7 * self.pure.clamp(0.0, 1.0),
            value.clamp(0.0, 1.0),
        );
        // The trail moves on by whole steps; what is left over waits.
        self.carry += ctx.dt * self.speed * TRAIL as f32;
        while self.carry >= 1.0 {
            self.carry -= 1.0;
            self.head = (self.head + 1) % TRAIL;
            self.trail[self.head] = self.now;
        }
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let age = match self.mode {
            1 => (x - 0.5).abs() * 2.0,
            2 => x,
            _ => return self.now,
        };
        let back = ((age.clamp(0.0, 1.0) * (TRAIL - 1) as f32) as usize).min(TRAIL - 1);
        self.trail[(self.head + TRAIL - back) % TRAIL]
    }
}

// ------------------------------------------------------------- Waterfall

/// Rows of the past a waterfall remembers.
const FALL_ROWS: usize = 64;

struct Waterfall {
    palette: Pal,
    speed: f32,
    gain: f32,
    rows: Vec<[f32; BANDS]>,
    head: usize,
    carry: f32,
}

impl Waterfall {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "waterfall",
                name: "Waterfall",
                description: "The sound's low to high notes across a matrix, falling away as time passes. On a strip it shows the newest row.",
                category: FOLLOWS_SOUND,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "fire"),
                    ParamSpec::float("speed", "Speed", 0.1, 4.0, 1.0),
                    ParamSpec::float("gain", "Strength", 0.2, 3.0, 1.0).advanced(),
                ],
            },
            make: || {
                Box::new(Waterfall {
                    palette: Pal::Borrowed(palette("fire")),
                    speed: 1.0,
                    gain: 1.0,
                    rows: vec![[0.0; BANDS]; FALL_ROWS],
                    head: 0,
                    carry: 0.0,
                })
            },
        }
    }

    fn row(&self, back: usize, x: f32) -> f32 {
        let row = &self.rows[(self.head + FALL_ROWS - back.min(FALL_ROWS - 1)) % FALL_ROWS];
        let at = x.clamp(0.0, 1.0) * (BANDS - 1) as f32;
        let (i, f) = (at as usize, at.fract());
        row[i] * (1.0 - f) + row[(i + 1).min(BANDS - 1)] * f
    }
}

impl Effect for Waterfall {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.speed = p.float("speed");
        self.gain = p.float("gain");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // The newest row always shows the sound as it is now.
        self.rows[self.head] = ctx.audio.bands;
        // A screen's worth of rows passes in about two seconds at speed one.
        self.carry += ctx.dt * self.speed * FALL_ROWS as f32 * 0.5;
        while self.carry >= 1.0 {
            self.carry -= 1.0;
            self.head = (self.head + 1) % FALL_ROWS;
            self.rows[self.head] = ctx.audio.bands;
        }
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let level = if ctx.flat {
            self.row(0, x)
        } else {
            // Between two rows, so the fall is smooth and not in steps.
            let at = y.clamp(0.0, 1.0) * (FALL_ROWS - 1) as f32 + self.carry;
            let (back, f) = (at as usize, at.fract());
            self.row(back, x) * (1.0 - f) + self.row(back + 1, x) * f
        };
        let level = (level * self.gain).clamp(0.0, 1.0);
        self.palette.sample(level).scale(smoothstep(0.0, 0.25, level))
    }
}

// ---------------------------------------------------------------- Glitch

struct Glitch {
    palette: Pal,
    amount: f32,
    hold: f32,
    any_hit: bool,
    watch: HitWatch,
    /// Changes with every glitch, so each one looks different.
    seed: u32,
    /// How much of the glitch is left, 1 down to 0.
    left: f32,
    /// Seconds since the last one, for the flash limit.
    since: f32,
}

impl Glitch {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "glitch",
                name: "Glitch",
                description: "Torn bars of colour that jump in on a hit and are gone again, like a picture breaking up.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::Capped,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("amount", "How much breaks up", 0.05, 1.0, 0.45),
                    ParamSpec::float("hold", "Lasts", 0.03, 1.0, 0.18),
                    ParamSpec::choice("on", "Set off by", &["Any hit", "Each beat"]),
                ],
            },
            make: || {
                Box::new(Glitch {
                    palette: Pal::Borrowed(palette("neon")),
                    amount: 0.45,
                    hold: 0.18,
                    any_hit: true,
                    watch: HitWatch::default(),
                    seed: 1,
                    left: 0.0,
                    since: f32::MAX,
                })
            },
        }
    }
}

impl Effect for Glitch {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.amount = p.float("amount");
        self.hold = p.float("hold");
        self.any_hit = p.choice("on") == 0;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.since += ctx.dt;
        self.left = (self.left - ctx.dt / self.hold.max(0.01)).max(0.0);
        let fired = self.watch.fired(&ctx.audio, self.any_hit);
        if fired && self.since >= 1.0 / ctx.flash_cap {
            self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.left = 1.0;
            self.since = 0.0;
        }
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        if self.left <= 0.0 {
            return Rgb::BLACK;
        }
        // Bars across the picture; each is torn into a few pieces, and
        // only some pieces light. A strip is one bar torn into more.
        let bar = if ctx.flat { 0 } else { (y * 9.0) as u32 };
        let pieces = if ctx.flat {
            14.0
        } else {
            3.0 + (hash2(bar, self.seed) * 5.0).floor()
        };
        let slip = hash2(bar ^ 0x9E37, self.seed);
        let piece = ((x + slip) * pieces) as u32;
        let dice = hash2(bar * 31 + piece, self.seed ^ 0x5bd1);
        if dice > self.amount {
            return Rgb::BLACK;
        }
        self.palette
            .sample(hash2(piece, bar ^ self.seed))
            .scale(self.left.sqrt())
    }
}

// ----------------------------------------------------------------- Movie

struct Film {
    /// The id the movie was looked up by, to see when it changes.
    source: String,
    movie: Option<Arc<Movie>>,
    /// Frames since the movie was last looked for, while it is missing.
    waited: u32,
    speed: f32,
    pace: usize,
    keep_shape: bool,
    bounce: bool,
    /// Seconds into the showing, on the effect's own clock.
    played: f32,
    frame: usize,
}

impl Film {
    const PACES: [&'static str; 5] = ["Its own time", "1 beat", "2 beats", "4 beats", "8 beats"];
    const BEATS: [f32; 5] = [0.0, 1.0, 2.0, 4.0, 8.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "movie",
                name: "Moving picture",
                description: "A GIF or a piece of video on a matrix, played round and round, at its own speed or in time with the music.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::movie("movie", "Moving picture"),
                    ParamSpec::float("speed", "Speed", 0.1, 4.0, 1.0),
                    ParamSpec::choice("pace", "One showing takes", &Self::PACES),
                    ParamSpec::choice("fit", "Fit", &["Stretch to fill", "Keep its shape"]),
                    ParamSpec::bool("bounce", "There and back", false).advanced(),
                ],
            },
            make: || {
                Box::new(Film {
                    source: String::new(),
                    movie: None,
                    waited: 0,
                    speed: 1.0,
                    pace: 0,
                    keep_shape: false,
                    bounce: false,
                    played: 0.0,
                    frame: 0,
                })
            },
        }
    }
}

impl Effect for Film {
    fn configure(&mut self, p: &ParamReader) {
        let source = p.movie("movie");
        // Compared first: this runs every frame and the movie seldom
        // changes. One that is not there is looked for again about once
        // a second, in case it has just been sent.
        self.waited += 1;
        if self.source != source || (self.movie.is_none() && self.waited > 60) {
            self.waited = 0;
            if self.source != source {
                self.played = 0.0;
            }
            self.source = source.to_string();
            self.movie = movie::get(source);
        }
        self.speed = p.float("speed");
        self.pace = p.choice("pace");
        self.keep_shape = p.choice("fit") == 1;
        self.bounce = p.bool("bounce");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.played += ctx.dt * self.speed;
        let Some(movie) = &self.movie else {
            return;
        };
        let length = movie.seconds().max(1e-3);
        let beats = Self::BEATS[self.pace.min(4)];
        // How far through one showing, 0..1: by the music's own count
        // when there is one to go by, else by the effect's clock.
        let through = if beats > 0.0 && ctx.audio.bpm > 0.0 {
            (ctx.beats / beats as f64).rem_euclid(1.0) as f32
        } else {
            (self.played / length).rem_euclid(1.0)
        };
        let through = if self.bounce {
            1.0 - (2.0 * through - 1.0).abs()
        } else {
            through
        };
        self.frame = movie.frame_at(through.min(0.999_99) * length);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // A movie that is not kept on this computer shows as the built-in
        // one, dimmed, so the lights say something is missing.
        let (movie, level) = match &self.movie {
            Some(movie) => (movie.clone(), 1.0),
            None => match movie::get(movie::BUILT_IN) {
                Some(stand_in) => (stand_in, 0.25),
                None => return Rgb::BLACK,
            },
        };
        let frame = if self.movie.is_some() {
            self.frame
        } else {
            movie.frame_at(self.played)
        };
        let (mut u, mut v) = (x, y);
        if ctx.flat {
            // A strip has one row of lights: it shows the middle row.
            v = 0.5;
        } else if self.keep_shape {
            let (own, room) = (movie.width as f32 / movie.height as f32, ctx.aspect.max(0.01));
            if own > room {
                v = (y - 0.5) * (own / room) + 0.5;
            } else {
                u = (x - 0.5) * (room / own) + 0.5;
            }
        }
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return Rgb::BLACK;
        }
        let [r, g, b] = movie.dot(frame, (u * movie.width as f32) as u32, (v * movie.height as f32) as u32);
        Rgb::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0).scale(level)
    }
}

// ------------------------------------------------------------------ Lava

struct Lava {
    palette: Pal,
    speed: f32,
    size: f32,
    count: f32,
    react: f32,
    drift: f32,
    swell: f32,
}

impl Lava {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "lava",
                name: "Lava lamp",
                description: "Slow blobs of colour that drift, meet and part. Bass makes them swell.",
                category: AMBIENT,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "sunset"),
                    ParamSpec::float("speed", "Speed", 0.02, 2.0, 0.25),
                    ParamSpec::float("size", "Blob size", 0.3, 2.0, 1.0),
                    ParamSpec::float("count", "Blobs", 2.0, 8.0, 5.0).advanced(),
                    ParamSpec::float("react", "Swell with the bass", 0.0, 1.0, 0.3),
                ],
            },
            make: || {
                Box::new(Lava {
                    palette: Pal::Borrowed(palette("sunset")),
                    speed: 0.25,
                    size: 1.0,
                    count: 5.0,
                    react: 0.3,
                    drift: 0.0,
                    swell: 0.0,
                })
            },
        }
    }
}

impl Effect for Lava {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.speed = p.float("speed");
        self.size = p.float("size");
        self.count = p.float("count").round().clamp(2.0, 8.0);
        self.react = p.float("react");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.drift += ctx.dt * self.speed;
        self.swell += (ctx.audio.bass - self.swell) * (1.0 - (-ctx.dt / 0.12).exp());
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let aspect = if ctx.flat { 1.0 } else { ctx.aspect };
        let y = if ctx.flat { 0.5 } else { y };
        let radius = 0.16 * self.size * (1.0 + 0.6 * self.react * self.swell);
        let mut field = 0.0f32;
        for i in 0..self.count as u32 {
            // Each blob wanders on a path of its own.
            let (a, b) = (hash2(i, 11) * 6.3, hash2(i, 23) * 6.3);
            let (fa, fb) = (0.6 + hash2(i, 37), 0.5 + hash2(i, 41));
            let bx = 0.5 + 0.42 * (self.drift * fa + a).sin();
            let by = 0.5 + 0.42 * (self.drift * fb + b).cos();
            let (dx, dy) = ((x - bx) * aspect, y - by);
            field += radius * radius / (dx * dx + dy * dy + 1e-4);
        }
        // One where a blob's edge is; more inside it, less outside.
        let inside = smoothstep(0.7, 1.3, field);
        self.palette.sample((field * 0.35).clamp(0.0, 1.0)).scale(inside)
    }
}

// ------------------------------------------------------------- Code rain

struct CodeRain {
    colour: Rgb,
    speed: f32,
    columns: f32,
    tail: f32,
    react: f32,
    fallen: f32,
}

impl CodeRain {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "code_rain",
                name: "Code rain",
                description: "Bright drops falling down a matrix in columns, each trailing a fading tail. Bass makes it pour.",
                category: AMBIENT,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#20ff50"),
                    ParamSpec::float("speed", "Speed", 0.05, 3.0, 0.6),
                    ParamSpec::float("columns", "Columns", 2.0, 64.0, 16.0),
                    ParamSpec::float("tail", "Tail", 0.05, 1.0, 0.45),
                    ParamSpec::float("react", "Pour with the bass", 0.0, 1.0, 0.4).advanced(),
                ],
            },
            make: || {
                Box::new(CodeRain {
                    colour: Rgb::new(0.12, 1.0, 0.31),
                    speed: 0.6,
                    columns: 16.0,
                    tail: 0.45,
                    react: 0.4,
                    fallen: 0.0,
                })
            },
        }
    }
}

impl Effect for CodeRain {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.speed = p.float("speed");
        self.columns = p.float("columns").round().clamp(2.0, 64.0);
        self.tail = p.float("tail");
        self.react = p.float("react");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.fallen += ctx.dt * self.speed * (1.0 + 2.0 * self.react * ctx.audio.bass);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // A strip is one column, with the drops running along it.
        let (column, down) = if ctx.flat {
            (0u32, x)
        } else {
            (((x * self.columns) as u32).min(self.columns as u32 - 1), y)
        };
        let mut best = 0.0f32;
        let mut head = 0.0f32;
        // Two drops to a column, each at its own speed, one after the other.
        for drop in 0..2u32 {
            let pace = 0.5 + hash2(column, drop + 3);
            let span = 1.0 + self.tail;
            let at = (self.fallen * pace + hash2(column, drop + 17) * span).rem_euclid(span);
            let behind = at - down;
            if (0.0..self.tail).contains(&behind) {
                let glow = 1.0 - behind / self.tail;
                best = best.max(glow * glow);
                head = head.max(1.0 - behind / 0.04);
            }
        }
        // The leading dot of a drop is nearly white.
        self.colour.scale(best).lerp(Rgb::WHITE, head.clamp(0.0, 1.0) * 0.7)
    }
}

// ------------------------------------------------------------------ Life

/// Columns of the board; rows follow from the shape of the lights.
const LIFE_COLS: usize = 28;

struct Life {
    palette: Pal,
    speed: f32,
    react: bool,
    watch: HitWatch,
    cols: usize,
    rows: usize,
    /// Generations each cell has been alive for; nought is dead.
    cells: Vec<u8>,
    /// The board one generation ago, for fading between the two.
    before: Vec<u8>,
    /// How far into the next generation the clock is, 0..1.
    carry: f32,
    /// Generations in a row the number of living cells has not changed.
    still: u32,
    rng: u32,
}

impl Life {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "life",
                name: "Life",
                description: "Cells that are born, live and die by their neighbours, forever making new shapes. Hits scatter new life.",
                category: AMBIENT,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours, by age", "aurora"),
                    ParamSpec::float("speed", "Generations a second", 1.0, 20.0, 6.0),
                    ParamSpec::bool("react", "Hits scatter new life", true),
                ],
            },
            make: || {
                Box::new(Life {
                    palette: Pal::Borrowed(palette("aurora")),
                    speed: 6.0,
                    react: true,
                    watch: HitWatch::default(),
                    cols: 0,
                    rows: 0,
                    cells: Vec::new(),
                    before: Vec::new(),
                    carry: 0.0,
                    still: 0,
                    rng: 0x2545_F491,
                })
            },
        }
    }

    fn chance(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Brings to life about `share` of the cells in a patch of the board.
    fn scatter(&mut self, share: f32, whole: bool) {
        let (cols, rows) = (self.cols, self.rows);
        let (left, top, wide, high) = if whole {
            (0, 0, cols, rows)
        } else {
            let (wide, high) = ((cols / 3).max(3), (rows / 3).max(3).min(rows));
            let left = (self.chance() * cols as f32) as usize;
            let top = (self.chance() * rows as f32) as usize;
            (left, top, wide, high)
        };
        for row in 0..high {
            for col in 0..wide {
                if self.chance() < share {
                    let at = ((top + row) % rows) * cols + (left + col) % cols;
                    if self.cells[at] == 0 {
                        self.cells[at] = 1;
                    }
                }
            }
        }
    }

    fn generation(&mut self) {
        let (cols, rows) = (self.cols, self.rows);
        self.before.clone_from(&self.cells);
        let alive = |col: usize, row: usize| u8::from(self.before[row * cols + col] > 0);
        let mut living = 0u32;
        for row in 0..rows {
            for col in 0..cols {
                // The board wraps round at its edges.
                let (l, r) = ((col + cols - 1) % cols, (col + 1) % cols);
                let (u, d) = ((row + rows - 1) % rows, (row + 1) % rows);
                let around = alive(l, u)
                    + alive(col, u)
                    + alive(r, u)
                    + alive(l, row)
                    + alive(r, row)
                    + alive(l, d)
                    + alive(col, d)
                    + alive(r, d);
                let age = self.before[row * cols + col];
                self.cells[row * cols + col] = match (age > 0, around) {
                    (true, 2 | 3) => age.saturating_add(1),
                    (false, 3) => 1,
                    _ => 0,
                };
                living += u32::from(self.cells[row * cols + col] > 0);
            }
        }
        let before: u32 = self.before.iter().map(|c| u32::from(*c > 0)).sum();
        self.still = if living == before { self.still + 1 } else { 0 };
        // A board that has died out, or settled into something that never
        // changes, is given new life.
        if living < (cols * rows / 25) as u32 || self.still > 24 {
            self.scatter(0.3, true);
            self.still = 0;
        }
    }
}

impl Effect for Life {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.speed = p.float("speed");
        self.react = p.bool("react");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let aspect = if ctx.flat { 3.5 } else { ctx.aspect.clamp(0.2, 8.0) };
        let rows = ((LIFE_COLS as f32 / aspect).round() as usize).clamp(6, 48);
        if self.rows != rows || self.cells.is_empty() {
            (self.cols, self.rows) = (LIFE_COLS, rows);
            self.cells = vec![0; LIFE_COLS * rows];
            self.scatter(0.35, true);
            self.before = self.cells.clone();
        }
        if self.react && self.watch.fired(&ctx.audio, true) {
            self.scatter(0.45, false);
        }
        self.carry += ctx.dt * self.speed;
        // Never more than a few at once, however long a frame took.
        let mut due = 0;
        while self.carry >= 1.0 && due < 4 {
            self.carry -= 1.0;
            self.generation();
            due += 1;
        }
        self.carry = self.carry.min(1.0);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        if self.cells.is_empty() {
            return Rgb::BLACK;
        }
        let col = ((x * self.cols as f32) as usize).min(self.cols - 1);
        // A strip shows the middle row of the board.
        let row = if ctx.flat {
            self.rows / 2
        } else {
            ((y * self.rows as f32) as usize).min(self.rows - 1)
        };
        let at = row * self.cols + col;
        let colour = |age: u8| {
            if age == 0 {
                Rgb::BLACK
            } else {
                self.palette.sample(((age - 1) as f32 / 12.0).min(1.0))
            }
        };
        // Cells fade in and out over a generation instead of blinking.
        colour(self.before[at]).lerp(colour(self.cells[at]), self.carry.clamp(0.0, 1.0))
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
            light: 0,
            lights: 1,
        }
    }

    fn make(id: &str, params: &Params) -> Box<dyn Effect> {
        let def = find(id).unwrap();
        let mut fx = (def.make)();
        fx.configure(&ParamReader::new(&def.info.params, params));
        fx
    }

    fn with(pairs: &[(&str, ParamValue)]) -> Params {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    fn brightness(c: Rgb) -> f32 {
        c.r.max(c.g).max(c.b)
    }

    /// A tempo of 120 with the beat counted from the time.
    fn music(t: f64) -> AudioFeatures {
        AudioFeatures {
            bpm: 120.0,
            beat_count: (t * 2.0) as u32,
            beat_phase: (t * 2.0).fract() as f32,
            active: true,
            level: 0.6,
            ..AudioFeatures::default()
        }
    }

    #[test]
    fn halves_take_turns_on_the_beat_and_so_can_whole_lights() {
        let mut fx = make("alternate", &with(&[]));
        let at = |fx: &mut Box<dyn Effect>, t: f64, light: u32| {
            let mut c = ctx(t, music(t), true);
            c.light = light;
            fx.update(&c);
            (fx.shade(0.2, 0.5, &c), fx.shade(0.8, 0.5, &c))
        };
        let (red, blue) = (Rgb::from_hex("#ff2030").unwrap(), Rgb::from_hex("#2050ff").unwrap());
        // First beat: the left half is lit in its colour, the right is dark.
        assert_eq!(at(&mut fx, 0.1, 0), (red, Rgb::BLACK));
        assert_eq!(at(&mut fx, 0.4, 0), (red, Rgb::BLACK), "steady within a beat");
        // Second beat: the other way round.
        assert_eq!(at(&mut fx, 0.6, 0), (Rgb::BLACK, blue));
        assert_eq!(at(&mut fx, 1.1, 0), (red, Rgb::BLACK));

        // Whole lights: every place on a light does the same, and the
        // second light is lit when the first is not.
        let lights = with(&[("what", ParamValue::Text("Whole lights".into()))]);
        let mut fx = make("alternate", &lights);
        assert_eq!(at(&mut fx, 0.1, 0), (red, red));
        assert_eq!(at(&mut fx, 0.1, 1), (Rgb::BLACK, Rgb::BLACK));
        assert_eq!(at(&mut fx, 0.6, 1), (blue, blue));
        assert_eq!(
            at(&mut fx, 0.6, 2),
            (Rgb::BLACK, Rgb::BLACK),
            "the third light goes with the first"
        );

        // Three taking turns is a chase from light to light.
        let three = with(&[
            ("what", ParamValue::Text("Whole lights".into())),
            ("parts", ParamValue::Number(3.0)),
        ]);
        let mut fx = make("alternate", &three);
        let lit = |fx: &mut Box<dyn Effect>, t: f64| -> Vec<bool> {
            (0..3).map(|light| brightness(at(fx, t, light).0) > 0.5).collect()
        };
        assert_eq!(lit(&mut fx, 0.1), [true, false, false]);
        assert_eq!(lit(&mut fx, 0.6), [false, true, false]);
        assert_eq!(lit(&mut fx, 1.1), [false, false, true]);

        // The one waiting can show the other colour instead of going dark.
        let mut fx = make("alternate", &with(&[("swap", ParamValue::Bool(true))]));
        assert_eq!(at(&mut fx, 0.1, 0), (red, red));
        assert_eq!(at(&mut fx, 0.6, 0), (blue, blue));

        // With the flash limit on, no part flashes more than it allows:
        // twice a beat at 120 is two flashes a second for each half.
        let fast = with(&[("pace", ParamValue::Text("Twice a beat".into()))]);
        let mut fx = make("alternate", &fast);
        let mut flashes = 0;
        let mut was = false;
        for frame in 0..600 {
            let t = frame as f64 / 60.0;
            let mut c = ctx(t, music(t), true);
            c.flash_cap = 1.0;
            fx.update(&c);
            let on = brightness(fx.shade(0.2, 0.5, &c)) > 0.5;
            flashes += u32::from(on && !was);
            was = on;
        }
        assert!(
            flashes <= 10,
            "{flashes} flashes in ten seconds with a limit of one a second"
        );
        assert!(flashes >= 4, "still taking turns: {flashes}");
    }

    #[test]
    fn beams_hang_from_above_sway_and_land_as_pools_on_a_strip() {
        let one = with(&[("count", ParamValue::Number(1.0)), ("punch", ParamValue::Number(0.0))]);
        let mut fx = make("beams", &one);
        // Straight down at the start of a sweep: bright under the lamp,
        // dark off to the sides, and wider further from the lamp.
        let still = ctx(0.0, music(0.0), false);
        fx.update(&still);
        assert!(brightness(fx.shade(0.5, 0.5, &still)) > 0.6);
        assert!(brightness(fx.shade(0.1, 0.5, &still)) < 0.01);
        let reach = |fx: &dyn Effect, c: &FrameCtx, y: f32| {
            (0..200)
                .filter(|i| brightness(fx.shade(*i as f32 / 200.0, y, c)) > 0.05)
                .count()
        };
        assert!(reach(&*fx, &still, 0.9) > reach(&*fx, &still, 0.2));
        // A quarter of a sweep on (one beat of four) it leans right; three
        // quarters on, left.
        let lean = |fx: &mut Box<dyn Effect>, t: f64| {
            let c = ctx(t, music(t), false);
            fx.update(&c);
            let lit: Vec<f32> = (0..200)
                .map(|i| i as f32 / 200.0)
                .filter(|x| brightness(fx.shade(*x, 0.4, &c)) > 0.05)
                .collect();
            lit.iter().sum::<f32>() / lit.len().max(1) as f32
        };
        assert!(lean(&mut fx, 0.5) > 0.7, "{}", lean(&mut fx, 0.5));
        assert!(lean(&mut fx, 1.5) < 0.3, "{}", lean(&mut fx, 1.5));
        // Hung below, the lamp is at the bottom.
        let below = with(&[
            ("count", ParamValue::Number(1.0)),
            ("from", ParamValue::Text("Below".into())),
        ]);
        let mut fx = make("beams", &below);
        fx.update(&still);
        assert!(reach(&*fx, &still, 0.1) > reach(&*fx, &still, 0.8));

        // On a strip each beam is a pool of light that moves.
        let mut fx = make("beams", &one);
        let pool = |fx: &mut Box<dyn Effect>, t: f64| {
            let c = ctx(t, music(t), true);
            fx.update(&c);
            (0..200)
                .map(|i| i as f32 / 200.0)
                .max_by(|a, b| brightness(fx.shade(*a, 0.5, &c)).total_cmp(&brightness(fx.shade(*b, 0.5, &c))))
                .unwrap()
        };
        assert!((pool(&mut fx, 0.0) - 0.5).abs() < 0.02);
        assert!(pool(&mut fx, 0.5) > 0.6 && pool(&mut fx, 1.5) < 0.4);
        // Four beams crossing over: neighbours lean opposite ways.
        let cross = with(&[("move", ParamValue::Text("Cross over".into()))]);
        let mut fx = make("beams", &cross);
        let c = ctx(0.5, music(0.5), false);
        fx.update(&c);
        for i in 0..400 {
            let px = fx.shade(i as f32 / 400.0, 0.7, &c);
            assert!([px.r, px.g, px.b].iter().all(|v| (0.0..=1.0).contains(v)));
        }
    }

    #[test]
    fn a_tune_colours_the_lights_and_drums_wash_the_colour_out() {
        let heard = |pitch: f32, tonal: f32| AudioFeatures {
            active: true,
            level: 1.0,
            pitch,
            tonal,
            ..AudioFeatures::default()
        };
        let settle = |fx: &mut Box<dyn Effect>, audio: AudioFeatures| {
            let c = ctx(0.0, audio, true);
            for _ in 0..240 {
                fx.update(&c);
            }
            fx.shade(0.5, 0.5, &c)
        };
        let mut fx = make("notes", &with(&[]));
        // C is red, E (a third of the way round) is green, G sharp blue.
        let c = settle(&mut fx, heard(0.0, 1.0));
        assert!(c.r > 0.95 && c.g < 0.05 && c.b < 0.05, "{c:?}");
        let e = settle(&mut fx, heard(1.0 / 3.0, 1.0));
        assert!(e.g > 0.95 && e.r < 0.05 && e.b < 0.05, "{e:?}");
        let g = settle(&mut fx, heard(2.0 / 3.0, 1.0));
        assert!(g.b > 0.95 && g.r < 0.05, "{g:?}");
        // From B down to C goes the short way round, not back through
        // every colour: half way there it is still a red, not a green.
        let mut fx = make("notes", &with(&[("calm", ParamValue::Number(1.0))]));
        settle(&mut fx, heard(11.0 / 12.0, 1.0));
        let step = ctx(0.0, heard(0.0, 1.0), true);
        for _ in 0..20 {
            fx.update(&step);
            let px = fx.shade(0.5, 0.5, &step);
            assert!(px.r > 0.9 && px.g < 0.3, "{px:?}");
        }
        // Drums alone: the last colour, washed out towards white.
        let mut fx = make("notes", &with(&[]));
        settle(&mut fx, heard(0.0, 1.0));
        let pale = settle(&mut fx, heard(0.5, 0.0));
        assert!(pale.r > 0.95 && pale.g > 0.6 && pale.b > 0.6, "{pale:?}");
        // Silence is dark, unless brightness does not follow the sound.
        assert_eq!(settle(&mut fx, AudioFeatures::default()), Rgb::BLACK);

        // A trail: the newest note is at the near end, older ones further on.
        let trail = with(&[("mode", ParamValue::Text("A trail from one end".into()))]);
        let mut fx = make("notes", &trail);
        settle(&mut fx, heard(0.0, 1.0));
        let c = ctx(0.0, heard(1.0 / 3.0, 1.0), true);
        for _ in 0..30 {
            fx.update(&c);
        }
        let (near, far) = (fx.shade(0.02, 0.5, &c), fx.shade(0.95, 0.5, &c));
        assert!(near.g > 0.5, "the new note has arrived: {near:?}");
        assert!(
            far.r > 0.9 && far.g < 0.1,
            "the old one is still on its way out: {far:?}"
        );
        // From the middle, both ends show the same.
        let middle = with(&[("mode", ParamValue::Text("A trail from the middle".into()))]);
        let mut fx = make("notes", &middle);
        settle(&mut fx, heard(0.0, 1.0));
        for _ in 0..30 {
            fx.update(&c);
        }
        assert_eq!(fx.shade(0.1, 0.5, &c), fx.shade(0.9, 0.5, &c));
    }

    #[test]
    fn a_waterfall_shows_the_sound_now_at_the_top_and_earlier_below() {
        let mut low = AudioFeatures {
            active: true,
            ..AudioFeatures::default()
        };
        low.bands[2] = 1.0;
        let mut high = low;
        high.bands = [0.0; BANDS];
        high.bands[BANDS - 3] = 1.0;
        let mut fx = make("waterfall", &with(&[]));
        let (left, right) = (2.0 / (BANDS - 1) as f32, (BANDS - 3) as f32 / (BANDS - 1) as f32);
        // A second of bass, then a quarter of a second of treble.
        let c = ctx(0.0, low, false);
        for _ in 0..60 {
            fx.update(&c);
        }
        let c = ctx(0.0, high, false);
        for _ in 0..15 {
            fx.update(&c);
        }
        assert!(brightness(fx.shade(right, 0.0, &c)) > 0.8, "treble now, at the top");
        assert!(brightness(fx.shade(left, 0.0, &c)) < 0.05);
        assert!(brightness(fx.shade(left, 0.4, &c)) > 0.8, "bass earlier, further down");
        assert!(brightness(fx.shade(right, 0.4, &c)) < 0.05);
        // A strip shows the newest row.
        let strip = ctx(0.0, high, true);
        assert!(brightness(fx.shade(right, 0.5, &strip)) > 0.8);
        assert!(brightness(fx.shade(left, 0.5, &strip)) < 0.05);
    }

    #[test]
    fn a_glitch_jumps_in_on_a_hit_and_is_gone_and_keeps_to_the_flash_limit() {
        let mut fx = make("glitch", &with(&[("amount", ParamValue::Number(1.0))]));
        let quiet = ctx(0.0, AudioFeatures::default(), false);
        fx.update(&quiet);
        let lit = |fx: &dyn Effect, c: &FrameCtx| {
            (0..400)
                .filter(|i| brightness(fx.shade((i % 20) as f32 / 20.0, (i / 20) as f32 / 20.0, c)) > 0.05)
                .count()
        };
        assert_eq!(lit(&*fx, &quiet), 0, "nothing until something hits");
        let hit = AudioFeatures {
            active: true,
            onset: 1.0,
            ..AudioFeatures::default()
        };
        let struck = ctx(0.0, hit, false);
        fx.update(&struck);
        assert_eq!(lit(&*fx, &struck), 400, "everything breaks up at full amount");
        // A second later it has gone.
        for _ in 0..60 {
            fx.update(&quiet);
        }
        assert_eq!(lit(&*fx, &quiet), 0);
        // At a lower amount only some of it lights, and on a strip too.
        let mut fx = make("glitch", &with(&[]));
        fx.update(&quiet);
        fx.update(&struck);
        let some = lit(&*fx, &struck);
        assert!(some > 40 && some < 360, "{some} of 400");
        let strip = ctx(0.0, hit, true);
        let along = (0..200)
            .filter(|i| brightness(fx.shade(*i as f32 / 200.0, 0.5, &strip)) > 0.05)
            .count();
        assert!(along > 10 && along < 190, "{along} of 200");
        // Hits every tenth of a second with a limit of two flashes a
        // second: no more than two glitches a second.
        let mut fx = make("glitch", &with(&[("hold", ParamValue::Number(0.03))]));
        let mut glitches = 0;
        let mut was = false;
        for frame in 0..300 {
            let on_hit = frame % 6 == 0;
            let mut c = ctx(0.0, if on_hit { hit } else { AudioFeatures::default() }, true);
            c.flash_cap = 2.0;
            fx.update(&c);
            let now = (0..50).any(|i| brightness(fx.shade(i as f32 / 50.0, 0.5, &c)) > 0.05);
            glitches += u32::from(now && !was);
            was = now;
        }
        assert!((5..=10).contains(&glitches), "{glitches} in five seconds");
    }

    #[test]
    fn a_moving_picture_plays_its_frames_in_its_own_time_or_the_musics() {
        // Two frames of two dots: red then blue on the left, dark on the
        // right; the first up for a tenth of a second, the second three.
        let id = movie::put(Movie {
            name: "Blink".into(),
            width: 2,
            height: 1,
            delays: vec![100, 300],
            rgb: vec![255, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0],
        });
        let chosen = ("movie", ParamValue::Text(id.clone()));
        let mut fx = make("movie", &with(std::slice::from_ref(&chosen)));
        let quiet = ctx(0.0, AudioFeatures::default(), false);
        let (red, blue) = (Rgb::new(1.0, 0.0, 0.0), Rgb::new(0.0, 0.0, 1.0));
        let mut seen = Vec::new();
        for _ in 0..24 {
            fx.update(&quiet);
            seen.push(fx.shade(0.2, 0.5, &quiet));
            assert_eq!(fx.shade(0.8, 0.5, &quiet), Rgb::BLACK);
        }
        // Six frames of red (a tenth of a second at sixty a second),
        // eighteen of blue, and round again.
        assert_eq!(seen.iter().filter(|c| **c == red).count(), 6, "{seen:?}");
        assert_eq!(seen.iter().filter(|c| **c == blue).count(), 18);
        assert_eq!(seen[0], red);
        fx.update(&quiet);
        assert_eq!(fx.shade(0.2, 0.5, &quiet), red, "round again");

        // One showing to a beat: at 120 the red quarter lasts an eighth
        // of a second wherever the effect's own clock is.
        let timed = with(&[chosen.clone(), ("pace", ParamValue::Text("1 beat".into()))]);
        let mut fx = make("movie", &timed);
        let at = |fx: &mut Box<dyn Effect>, t: f64| {
            let c = ctx(t, music(t), false);
            fx.update(&c);
            fx.shade(0.2, 0.5, &c)
        };
        assert_eq!(at(&mut fx, 0.05), red);
        assert_eq!(at(&mut fx, 0.2), blue);
        assert_eq!(at(&mut fx, 0.55), red);
        assert_eq!(at(&mut fx, 0.05), red, "the music's count decides");

        // A strip shows the middle row; a movie that is not here shows
        // the stand-in, dimmed, until it arrives.
        let strip = ctx(0.0, AudioFeatures::default(), true);
        assert_eq!(fx.shade(0.2, 0.9, &strip), fx.shade(0.2, 0.1, &strip));
        let missing = with(&[("movie", ParamValue::Text("0123456789abcdef".into()))]);
        let mut fx = make("movie", &missing);
        let mut brightest = 0.0f32;
        for _ in 0..40 {
            fx.update(&quiet);
            for i in 0..256 {
                brightest = brightest.max(brightness(fx.shade(
                    (i % 16) as f32 / 16.0,
                    (i / 16) as f32 / 16.0,
                    &quiet,
                )));
            }
        }
        assert!(brightest > 0.1 && brightest <= 0.25, "{brightest}");
        movie::remove(&id);
    }

    #[test]
    fn lava_has_soft_blobs_with_dark_between_that_drift_and_swell() {
        let mut fx = make("lava", &with(&[]));
        let c = ctx(0.0, AudioFeatures::default(), false);
        fx.update(&c);
        let picture = |fx: &dyn Effect, c: &FrameCtx| -> Vec<f32> {
            (0..900)
                .map(|i| brightness(fx.shade((i % 30) as f32 / 30.0, (i / 30) as f32 / 30.0, c)))
                .collect()
        };
        let first = picture(&*fx, &c);
        let lit = first.iter().filter(|v| **v > 0.5).count();
        let dark = first.iter().filter(|v| **v < 0.02).count();
        assert!(lit > 40 && dark > 200, "{lit} lit, {dark} dark of 900");
        // It moves.
        for _ in 0..300 {
            fx.update(&c);
        }
        assert_ne!(picture(&*fx, &c), first);
        // Bass makes the blobs bigger.
        let mut fx = make(
            "lava",
            &with(&[("react", ParamValue::Number(1.0)), ("speed", ParamValue::Number(0.02))]),
        );
        fx.update(&c);
        let small = picture(&*fx, &c).iter().filter(|v| **v > 0.5).count();
        let bass = AudioFeatures {
            active: true,
            bass: 1.0,
            ..AudioFeatures::default()
        };
        let loud = ctx(0.0, bass, false);
        for _ in 0..60 {
            fx.update(&loud);
        }
        assert!(picture(&*fx, &loud).iter().filter(|v| **v > 0.5).count() > small);
        // A strip gets a slice of it.
        let strip = ctx(0.0, AudioFeatures::default(), true);
        assert!((0..200).any(|i| brightness(fx.shade(i as f32 / 200.0, 0.5, &strip)) > 0.5));
    }

    #[test]
    fn code_rain_falls_in_columns_with_a_bright_head_and_a_tail_above_it() {
        let mut fx = make("code_rain", &with(&[("columns", ParamValue::Number(4.0))]));
        let c = ctx(0.0, AudioFeatures::default(), false);
        for _ in 0..30 {
            fx.update(&c);
        }
        // One colour all the way across a column.
        for row in 0..40 {
            let y = row as f32 / 40.0;
            assert_eq!(fx.shade(0.02, y, &c), fx.shade(0.2, y, &c));
        }
        // Somewhere in the first column there is a head with a tail above
        // it that gets dimmer the further up it goes.
        let column = |fx: &dyn Effect, c: &FrameCtx| -> Vec<Rgb> {
            (0..200).map(|row| fx.shade(0.1, row as f32 / 200.0, c)).collect()
        };
        let now = column(&*fx, &c);
        let head = (0..200).max_by(|a, b| now[*a].r.total_cmp(&now[*b].r)).unwrap();
        assert!(now[head].r > 0.3, "the head is nearly white: {:?}", now[head]);
        if head >= 30 {
            assert!(now[head - 10].g > now[head - 30].g, "the tail fades upwards");
            assert!(now[head - 10].r < 0.2, "and is the rain's own colour");
        }
        // It falls: a moment later the head is further down, or has gone
        // off the bottom and a new one has started.
        for _ in 0..6 {
            fx.update(&c);
        }
        assert_ne!(column(&*fx, &c), now);
        // A strip is one column.
        let strip = ctx(0.0, AudioFeatures::default(), true);
        assert!((0..200).any(|i| brightness(fx.shade(i as f32 / 200.0, 0.5, &strip)) > 0.3));
    }

    #[test]
    fn life_goes_on_by_its_rules_and_never_dies_out_for_good() {
        let mut fx = make("life", &with(&[("react", ParamValue::Bool(false))]));
        let c = ctx(0.0, AudioFeatures::default(), false);
        let alive = |fx: &dyn Effect, c: &FrameCtx| -> usize {
            let mut n = 0;
            for row in 0..28 {
                for col in 0..28 {
                    let px = fx.shade((col as f32 + 0.5) / 28.0, (row as f32 + 0.5) / 28.0, c);
                    n += usize::from(brightness(px) > 0.05);
                }
            }
            n
        };
        fx.update(&c);
        let start = alive(&*fx, &c);
        assert!(start > 100, "a board scattered with life to begin with: {start}");
        // Ten minutes on there is still life, and it has changed.
        let mut counts = Vec::new();
        for frame in 0..36_000 {
            fx.update(&c);
            if frame % 600 == 0 {
                counts.push(alive(&*fx, &c));
            }
        }
        assert!(counts.iter().all(|n| *n > 10), "{counts:?}");
        assert!(counts.windows(2).any(|w| w[0] != w[1]), "{counts:?}");

        // The rules: a block of four stays as it is, a row of three turns.
        let mut life = Life {
            palette: Pal::Borrowed(palette("aurora")),
            speed: 1.0,
            react: false,
            watch: HitWatch::default(),
            cols: 8,
            rows: 8,
            cells: vec![0; 64],
            before: vec![0; 64],
            carry: 0.0,
            still: 0,
            rng: 1,
        };
        for at in [9, 10, 17, 18, 36, 37, 38] {
            life.cells[at] = 1;
        }
        life.generation();
        let living: Vec<usize> = (0..64).filter(|i| life.cells[*i] > 0).collect();
        assert_eq!(living, [9, 10, 17, 18, 29, 37, 45]);
        // The block's cells have aged; the blinker's middle too.
        assert_eq!((life.cells[9], life.cells[37], life.cells[29]), (2, 2, 1));
        // A strip shows a row of the board.
        let mut fx = make("life", &with(&[]));
        let strip = ctx(0.0, AudioFeatures::default(), true);
        let mut seen = false;
        for _ in 0..600 {
            fx.update(&strip);
            seen |= (0..100).any(|i| brightness(fx.shade(i as f32 / 100.0, 0.5, &strip)) > 0.3);
        }
        assert!(seen);
    }
}
