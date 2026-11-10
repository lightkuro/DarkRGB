//! Built-in effects.

use std::sync::OnceLock;

use crate::color::{palette, Palette, Rgb};
use crate::effect::{hash2, smoothstep, Effect, EffectDef, EffectInfo, FrameCtx};
use crate::params::{ParamReader, ParamSpec};

/// The effect a new show starts with. It moves without sound, so a first
/// run never looks broken, and it reacts once music plays.
pub const DEFAULT_EFFECT: &str = "plasma";

pub fn registry() -> &'static [EffectDef] {
    static REGISTRY: OnceLock<Vec<EffectDef>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        vec![
            Plasma::def(),
            Spectrum::def(),
            Energy::def(),
            BeatPulse::def(),
            Scroll::def(),
            Ripple::def(),
            Sparkle::def(),
            Meter::def(),
            Flow::def(),
            Solid::def(),
        ]
    })
}

pub fn find(id: &str) -> Option<&'static EffectDef> {
    registry().iter().find(|d| d.info.id == id)
}

fn sensitivity() -> ParamSpec {
    ParamSpec::float("sensitivity", "Sensitivity", 0.2, 3.0, 1.0)
}

fn boosted(v: f32, sensitivity: f32) -> f32 {
    (v * sensitivity).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------- Plasma

struct Plasma {
    palette: &'static Palette,
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
                audio: true,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("speed", "Speed", 0.0, 2.0, 0.5),
                    ParamSpec::float("scale", "Detail", 0.3, 4.0, 1.2),
                    ParamSpec::float("react", "React to sound", 0.0, 1.0, 0.6),
                ],
            },
            make: || {
                Box::new(Plasma {
                    palette: palette("neon"),
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
    palette: &'static Palette,
    sensitivity: f32,
    mirror: bool,
}

impl Spectrum {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "spectrum",
                name: "Spectrum",
                description: "Bass on one side, treble on the other. Bars on a matrix, brightness on a strip.",
                audio: true,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "rainbow"),
                    sensitivity(),
                    ParamSpec::bool("mirror", "Mirror from centre", false),
                ],
            },
            make: || {
                Box::new(Spectrum {
                    palette: palette("rainbow"),
                    sensitivity: 1.0,
                    mirror: false,
                })
            },
        }
    }
}

impl Effect for Spectrum {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.sensitivity = p.float("sensitivity");
        self.mirror = p.bool("mirror");
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let t = if self.mirror { (x - 0.5).abs() * 2.0 } else { x };
        let v = boosted(ctx.audio.band_at(t), self.sensitivity);
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
    sensitivity: f32,
    soft: f32,
}

impl Energy {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "energy",
                name: "Energy",
                description: "Three colours grow from the centre: one each for bass, mids and treble.",
                audio: true,
                params: vec![
                    ParamSpec::color("low", "Bass colour", "#ff2a2a"),
                    ParamSpec::color("mid", "Mid colour", "#22ff66"),
                    ParamSpec::color("high", "Treble colour", "#3a6bff"),
                    sensitivity(),
                    ParamSpec::float("soft", "Soft edges", 0.0, 1.0, 0.3),
                ],
            },
            make: || {
                Box::new(Energy {
                    low: Rgb::WHITE,
                    mid: Rgb::WHITE,
                    high: Rgb::WHITE,
                    sensitivity: 1.0,
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
        self.sensitivity = p.float("sensitivity");
        self.soft = p.float("soft");
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let r = ctx.radius(x, y);
        let edge = 0.02 + self.soft * 0.4;
        let blob = |v: f32| {
            let v = boosted(v, self.sensitivity);
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

struct BeatPulse {
    palette: &'static Palette,
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
                audio: true,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "candy"),
                    ParamSpec::float("decay", "Fade time (s)", 0.05, 1.5, 0.35),
                    ParamSpec::float("floor", "Resting brightness", 0.0, 0.5, 0.04),
                    ParamSpec::choice("trigger", "Flash on", &["Beat", "Any hit"]),
                ],
            },
            make: || {
                Box::new(BeatPulse {
                    palette: palette("candy"),
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
            self.envelope *= (-ctx.dt / self.decay.max(0.01) * 3.0).exp();
        }
    }

    fn shade(&self, _x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let level = self.floor + self.envelope * (1.0 - self.floor);
        self.palette.sample(self.hue).scale(level)
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
    sensitivity: f32,
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
                audio: true,
                params: vec![
                    ParamSpec::color("low", "Bass colour", "#ff3d2e"),
                    ParamSpec::color("mid", "Mid colour", "#2eff8a"),
                    ParamSpec::color("high", "Treble colour", "#3d7bff"),
                    ParamSpec::float("speed", "Speed", 0.1, 3.0, 1.0),
                    ParamSpec::choice("origin", "Starts at", &["Centre", "Left", "Right"]),
                    sensitivity(),
                ],
            },
            make: || {
                Box::new(Scroll {
                    low: Rgb::WHITE,
                    mid: Rgb::WHITE,
                    high: Rgb::WHITE,
                    speed: 1.0,
                    origin: 0,
                    sensitivity: 1.0,
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
        self.sensitivity = p.float("sensitivity");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let a = &ctx.audio;
        let s = self.sensitivity;
        let now = self
            .low
            .scale(boosted(a.bass, s).powi(2))
            .add(self.mid.scale(boosted(a.mid, s).powi(2)))
            .add(self.high.scale(boosted(a.treble, s).powi(2)))
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
    palette: &'static Palette,
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
                audio: true,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "ocean"),
                    ParamSpec::float("speed", "Speed", 0.2, 3.0, 1.0),
                    ParamSpec::float("width", "Ring width", 0.02, 0.5, 0.12),
                    ParamSpec::choice("trigger", "New ring on", &["Beat", "Any hit"]),
                ],
            },
            make: || {
                Box::new(Ripple {
                    palette: palette("ocean"),
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
            let t = if self.palette.cyclic { self.hue } else { 0.35 + self.hue * 0.65 };
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
                description: "Random twinkles. More of them when the treble is busy.",
                audio: true,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ffffff"),
                    ParamSpec::float("density", "Amount", 0.0, 1.0, 0.25),
                    ParamSpec::float("rate", "Twinkle speed", 1.0, 20.0, 6.0),
                    ParamSpec::bool("react", "React to sound", true),
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
            self.density * (0.15 + 1.6 * a.treble + 0.6 * a.onset)
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
    palette: &'static Palette,
    sensitivity: f32,
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
                audio: true,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "fire"),
                    sensitivity(),
                    ParamSpec::bool("hold", "Peak marker", true),
                ],
            },
            make: || {
                Box::new(Meter {
                    palette: palette("fire"),
                    sensitivity: 1.0,
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
        self.sensitivity = p.float("sensitivity");
        self.hold = p.bool("hold");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let level = boosted(ctx.audio.level, self.sensitivity);
        self.peak = (self.peak - ctx.dt * 0.35).max(level);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // Run along the longer side: upward on tall layouts, rightward otherwise.
        let pos = if !ctx.flat && ctx.aspect < 1.0 { 1.0 - y } else { x };
        let level = boosted(ctx.audio.level, self.sensitivity);
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
    palette: &'static Palette,
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
                audio: false,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "rainbow"),
                    ParamSpec::float("speed", "Speed", -2.0, 2.0, 0.2),
                    ParamSpec::float("scale", "Repeats", 0.1, 4.0, 1.0),
                    ParamSpec::float("angle", "Angle", 0.0, 360.0, 0.0),
                ],
            },
            make: || {
                Box::new(Flow {
                    palette: palette("rainbow"),
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
    pulse: f32,
}

impl Solid {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "solid",
                name: "Solid colour",
                description: "One colour everywhere. Optionally breathes with loudness.",
                audio: false,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ff7a18"),
                    ParamSpec::float("pulse", "Pulse with sound", 0.0, 1.0, 0.0),
                ],
            },
            make: || {
                Box::new(Solid {
                    colour: Rgb::WHITE,
                    pulse: 0.0,
                })
            },
        }
    }
}

impl Effect for Solid {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.pulse = p.float("pulse");
    }

    fn shade(&self, _x: f32, _y: f32, ctx: &FrameCtx) -> Rgb {
        self.colour.scale(1.0 - self.pulse + self.pulse * ctx.audio.level)
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
            aspect: if flat { 1.0 } else { 2.0 },
            flat,
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
            beat: 1.0,
            beat_phase: 0.0,
            bpm: 120.0,
            beat_count,
            active: true,
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
                    let audio = if frame < 60 { AudioFeatures::default() } else { loud(frame / 30) };
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
}
