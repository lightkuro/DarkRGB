//! Effects ported from LedFx.
//!
//! LedFx (https://github.com/LedFx/LedFx) is free software under the GNU
//! General Public License, version 3. The effects in this file are
//! translations of LedFx's own effects from Python into Rust, kept as
//! close to the originals as darkRGB's way of drawing allows, and so are
//! a work based on LedFx: copyright the LedFx contributors (its creator
//! Austin Hodges, and the developers and contributors listed in its
//! AUTHORS.rst), with changes for darkRGB. The ripple a raindrop makes is
//! LedFx's own animation (`effects/droplets/ripple.npy`), copied as numbers.
//!
//! What changed in the translation, for all of them:
//!
//! - LedFx draws onto as many pixels as a light has. These draw onto
//!   [`N`] pixels, and each light shows them stretched to its own length,
//!   so an effect looks the same on a short strip and a long one.
//! - LedFx updates its sound sixty times a second and draws at the
//!   light's own rate. These do both once a frame, and every rate (a
//!   filter's, a scroll's, a decay's) is scaled to the time a frame takes,
//!   so they move as fast at 30 frames a second as at 120.
//! - LedFx's sound comes from its own analysis. Here its "melbank" is
//!   darkRGB's 32 bands, its lows, mids and highs are bass, mids and
//!   treble, its beat clock is darkRGB's, its "onset" is a hit, and its
//!   pitch is the note darkRGB hears.
//! - Colours are 0 to 1, not 0 to 255.
//!
//! Changes to single effects are noted where they are.

use crate::audio::{AudioFeatures, BANDS};
use crate::color::{Pal, Rgb, palette};
use crate::effect::{Effect, EffectDef, EffectInfo, FlashUse, FrameCtx, HitWatch, hash2};
use crate::params::{ParamReader, ParamSpec};

/// The picker's section for these.
pub const FROM_LEDFX: &str = "From LedFx";

/// Pixels these effects are drawn on, before each light stretches them.
const N: usize = 150;

/// LedFx's update rate: its filters and steps are written for it.
const RATE: f32 = 60.0;

pub(crate) fn all() -> Vec<EffectDef> {
    vec![
        Energy::def(),
        Scroll::def(),
        Wavelength::def(),
        Power::def(),
        Rain::def(),
        Bands::def(),
        Blocks::def(),
        Magnitude::def(),
        PitchSpectrum::def(),
        SpectrumLf::def(),
        Bar::def(),
        MultiBar::def(),
        BpmStrobe::def(),
        StrobeLf::def(),
        Scan::def(),
        Energy2::def(),
        Melt::def(),
        Marching::def(),
        Crawler::def(),
        BlockReflections::def(),
        Gradient::def(),
        Fade::def(),
        RainbowLf::def(),
    ]
}

// ------------------------------------------------------------ the helpers

/// How much of a step written for sixty a second to take in `dt`.
fn per_frame(dt: f32) -> f32 {
    (dt * RATE).clamp(0.0, 8.0)
}

/// LedFx's smoothing filter: rises by one share, falls by another, for
/// each of its updates.
#[derive(Clone, Copy)]
struct ExpFilter {
    decay: f32,
    rise: f32,
    value: Option<f32>,
}

impl ExpFilter {
    fn new(decay: f32, rise: f32) -> Self {
        Self {
            decay: decay.clamp(0.001, 0.999),
            rise: rise.clamp(0.001, 0.999),
            value: None,
        }
    }

    fn step(alpha: f32, dt: f32) -> f32 {
        1.0 - (1.0 - alpha).powf(per_frame(dt))
    }

    fn update(&mut self, value: f32, dt: f32) -> f32 {
        let next = match self.value {
            None => value,
            Some(old) => {
                let alpha = if value > old { self.rise } else { self.decay };
                old + (value - old) * Self::step(alpha, dt)
            }
        };
        self.value = Some(next);
        next
    }

    /// The same, for every channel of every pixel of a picture at once.
    fn update_pixels(&self, held: &mut [Rgb], target: &[Rgb], dt: f32) {
        let (rise, decay) = (Self::step(self.rise, dt), Self::step(self.decay, dt));
        let towards = |old: f32, new: f32| old + (new - old) * if new > old { rise } else { decay };
        for (h, t) in held.iter_mut().zip(target) {
            *h = Rgb::new(towards(h.r, t.r), towards(h.g, t.g), towards(h.b, t.b));
        }
    }
}

/// LedFx's melbank: darkRGB's bands, stretched to `out.len()` places.
fn melbank(audio: &AudioFeatures, out: &mut [f32]) {
    let n = out.len();
    for (i, v) in out.iter_mut().enumerate() {
        let at = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 } * (BANDS - 1) as f32;
        let (lo, f) = (at as usize, at.fract());
        let hi = (lo + 1).min(BANDS - 1);
        *v = (audio.bands[lo] * (1.0 - f) + audio.bands[hi] * f).clamp(0.0, 1.0);
    }
}

/// The bands split as LedFx splits them: the lowest fifth, up to half,
/// and the rest.
fn thirds(audio: &AudioFeatures) -> [&[f32]; 3] {
    let (a, b) = (BANDS / 5, BANDS / 2);
    [&audio.bands[..a], &audio.bands[a..b], &audio.bands[b..]]
}

fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

fn max(v: &[f32]) -> f32 {
    v.iter().copied().fold(0.0, f32::max)
}

/// LedFx's "frequency range" choices and what each reads here.
const POWERS: [&str; 5] = ["Lows (beat+bass)", "Beat", "Bass", "Mids", "High"];

fn power(audio: &AudioFeatures, which: usize) -> f32 {
    match which {
        1 => audio.beat,
        2 => audio.bass,
        3 => audio.mid,
        4 => audio.treble,
        _ => audio.bass,
    }
    .clamp(0.0, 1.0)
}

/// Where in the bar the music is, 0 to just under 4, as LedFx's bar
/// oscillator: from the beat clock, or at 120 a minute without one.
fn bar_oscillator(ctx: &FrameCtx) -> f32 {
    if ctx.audio.bpm > 0.0 {
        ((ctx.audio.bar_beat % 4) as f32 + ctx.audio.beat_phase.clamp(0.0, 0.999)) % 4.0
    } else {
        (ctx.time * 2.0).rem_euclid(4.0) as f32
    }
}

/// A picture of [`N`] pixels as it is shown: stretched over the light,
/// and with LedFx's mirror, which puts the start of the picture in the
/// middle and its end at both ends.
fn sample(picture: &[Rgb], x: f32, mirror: bool) -> Rgb {
    let n = picture.len();
    if n == 0 {
        return Rgb::BLACK;
    }
    let u = if mirror { (2.0 * x - 1.0).abs() } else { x };
    let at = u.clamp(0.0, 1.0) * (n - 1) as f32;
    let (i, f) = (at as usize, at.fract());
    picture[i].lerp(picture[(i + 1).min(n - 1)], f)
}

/// LedFx's blur: a Gaussian of `sigma` pixels, the ends reflected.
fn blur(picture: &mut [Rgb], sigma: f32, scratch: &mut Vec<Rgb>) {
    if sigma < 0.05 || picture.len() < 2 {
        return;
    }
    let radius = (4.0 * sigma).ceil() as isize;
    let weights: Vec<f32> = (-radius..=radius)
        .map(|k| (-(k * k) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f32 = weights.iter().sum();
    let n = picture.len() as isize;
    scratch.clear();
    scratch.extend_from_slice(picture);
    for (i, out) in picture.iter_mut().enumerate() {
        let mut sum = Rgb::BLACK;
        for (k, w) in (-radius..=radius).zip(&weights) {
            let mut j = i as isize + k;
            // Reflect at the ends, as LedFx does.
            while j < 0 || j >= n {
                j = if j < 0 { -j - 1 } else { 2 * n - j - 1 };
            }
            sum = sum.add(scratch[j as usize].scale(*w));
        }
        *out = sum.scale(1.0 / total);
    }
}

/// A gradient as LedFx uses one: a palette, slowly rolled along if asked.
struct Grad {
    pal: Pal,
    roll: f32,
    offset: f32,
}

impl Grad {
    fn new(id: &'static str) -> Self {
        Self {
            pal: Pal::Borrowed(palette(id)),
            roll: 0.0,
            offset: 0.0,
        }
    }

    fn configure(&mut self, p: &ParamReader) {
        self.pal = p.palette("gradient");
        self.roll = p.float("roll");
    }

    /// For the effects that LedFx gives no roll.
    fn configure_fixed(&mut self, p: &ParamReader) {
        self.pal = p.palette("gradient");
        self.roll = 0.0;
    }

    fn advance(&mut self, dt: f32) {
        self.offset = (self.offset + self.roll / N as f32 * per_frame(dt)).rem_euclid(1.0);
    }

    fn at(&self, t: f32) -> Rgb {
        let t = if self.roll > 0.0 {
            (t - self.offset).rem_euclid(1.0)
        } else {
            t.clamp(0.0, 1.0)
        };
        self.pal.sample(t)
    }
}

fn gradient_params(default: &'static str) -> [ParamSpec; 2] {
    [
        ParamSpec::palette("gradient", "Gradient", default),
        ParamSpec::float("roll", "Roll the gradient", 0.0, 10.0, 0.0).advanced(),
    ]
}

fn info(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    audio: bool,
    flash: FlashUse,
    params: Vec<ParamSpec>,
) -> EffectInfo {
    EffectInfo {
        id,
        name,
        description,
        category: FROM_LEDFX,
        audio,
        flash,
        params,
    }
}

fn colour_triple() -> [ParamSpec; 3] {
    [
        ParamSpec::color("color_lows", "Colour of the lows", "#ff0000"),
        ParamSpec::color("color_mids", "Colour of the mids", "#00ff00"),
        ParamSpec::color("color_high", "Colour of the highs", "#0000ff"),
    ]
}

/// The "HSV" family of LedFx effects: each pixel is given a hue (a place
/// in the gradient), a saturation and a value, and turned into a colour.
struct Hsv {
    h: Vec<f32>,
    s: Vec<f32>,
    v: Vec<f32>,
    out: Vec<Rgb>,
    /// LedFx's "fix hues": a curve that spreads hues more evenly to the eye.
    fix_hues: bool,
}

impl Hsv {
    fn new() -> Self {
        Self {
            h: vec![0.0; N],
            s: vec![1.0; N],
            v: vec![0.0; N],
            out: vec![Rgb::BLACK; N],
            fix_hues: true,
        }
    }

    fn colour(&mut self, grad: &Grad) {
        for i in 0..N {
            let mut h = self.h[i].rem_euclid(1.0);
            if self.fix_hues {
                h = 0.5 * ((h - 0.5) / 2.0 * std::f32::consts::TAU).sin() + 0.5;
            }
            let c = grad.at(h.clamp(0.0, 1.0));
            let top = c.r.max(c.g).max(c.b);
            let unsat = 1.0 - self.s[i].clamp(0.0, 1.0);
            let lift = |ch: f32| ch + (top - ch) * unsat;
            let v = self.v[i].clamp(0.0, 1.0);
            self.out[i] = Rgb::new(lift(c.r) * v, lift(c.g) * v, lift(c.b) * v);
        }
    }
}

/// LedFx's clock for these: a sawtooth from 0 to 1 every 65.536 seconds
/// divided by `modifier`.
fn sawtooth(modifier: f32, seconds: f64) -> f32 {
    let period = 65.536 / modifier.max(1e-6) as f64;
    (seconds.rem_euclid(period) / period) as f32
}

fn wave(x: f32) -> f32 {
    0.5 * (x * std::f32::consts::TAU).sin() + 0.5
}

fn triangle(x: f32) -> f32 {
    1.0 - 2.0 * (x.rem_euclid(1.0) - 0.5).abs()
}

fn hsv_params(speed: f32, reactivity: f32) -> Vec<ParamSpec> {
    let [g, r] = gradient_params("lf_rainbow");
    vec![
        g,
        ParamSpec::float("speed", "Speed", 0.001, 1.0, speed),
        ParamSpec::float("reactivity", "Reacts to the lows", 0.0001, 1.0, reactivity),
        r,
        ParamSpec::bool("fix_hues", "Even out the hues", true).advanced(),
    ]
}

// ------------------------------------------------------------------ Energy

struct Energy {
    colours: [Rgb; 3],
    sensitivity: f32,
    blur: f32,
    mirror: bool,
    overlap: bool,
    cycler: bool,
    watch: HitWatch,
    cycled: usize,
    filter: ExpFilter,
    target: Vec<Rgb>,
    out: Vec<Rgb>,
    shown: Vec<Rgb>,
    scratch: Vec<Rgb>,
}

impl Energy {
    fn def() -> EffectDef {
        let [lows, mids, high] = colour_triple();
        EffectDef {
            info: info(
                "lf_energy",
                "Energy",
                "LedFx's classic: bars of colour grow out from the middle with the lows, mids and highs.",
                true,
                FlashUse::None,
                vec![
                    lows,
                    mids,
                    high,
                    ParamSpec::float("sensitivity", "Responsiveness", 0.3, 0.99, 0.6),
                    ParamSpec::float("blur", "Blur", 0.0, 10.0, 4.0),
                    ParamSpec::bool("mirror", "Mirror", true),
                    ParamSpec::choice("mixing", "Mixing", &["Additive", "Overlap"]).advanced(),
                    ParamSpec::bool("cycler", "Change colours on the beat", false).advanced(),
                ],
            ),
            make: || {
                Box::new(Energy {
                    colours: [
                        Rgb::new(1.0, 0.0, 0.0),
                        Rgb::new(0.0, 1.0, 0.0),
                        Rgb::new(0.0, 0.0, 1.0),
                    ],
                    sensitivity: 0.6,
                    blur: 4.0,
                    mirror: true,
                    overlap: false,
                    cycler: false,
                    watch: HitWatch::default(),
                    cycled: 0,
                    filter: ExpFilter::new(0.35, 0.6),
                    target: vec![Rgb::BLACK; N],
                    out: vec![Rgb::BLACK; N],
                    shown: vec![Rgb::BLACK; N],
                    scratch: Vec::new(),
                })
            },
        }
    }
}

impl Effect for Energy {
    fn configure(&mut self, p: &ParamReader) {
        if !self.cycler {
            self.colours = [p.color("color_lows"), p.color("color_mids"), p.color("color_high")];
        }
        self.sensitivity = p.float("sensitivity");
        self.blur = p.float("blur");
        self.mirror = p.bool("mirror");
        self.overlap = p.choice("mixing") == 1;
        self.cycler = p.bool("cycler");
        self.filter = ExpFilter::new((self.sensitivity - 0.1) * 0.7, self.sensitivity);
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // Change: LedFx picks its new colour from the user's saved colours;
        // here it is a full colour at a hue picked by chance.
        if self.cycler && self.watch.fired(&ctx.audio, false) {
            self.cycled = (self.cycled + 1) % 3;
            let hue = hash2(ctx.audio.beat_count, 0x1ed);
            self.colours[self.cycled] = Rgb::from_hsv(hue, 1.0, 1.0);
        }
        let multiplier = 1.6 - self.blur / 17.0;
        let lengths = thirds(&ctx.audio).map(|t| ((multiplier * N as f32 * mean(t)) as usize).min(N));
        for (i, px) in self.target.iter_mut().enumerate() {
            let mut c = Rgb::BLACK;
            for (k, len) in lengths.iter().enumerate() {
                if i < *len {
                    c = if self.overlap || k == 0 {
                        self.colours[k]
                    } else {
                        c.add(self.colours[k])
                    };
                }
            }
            *px = c.clamp();
        }
        self.filter.update_pixels(&mut self.out, &self.target, ctx.dt);
        self.shown.clone_from(&self.out);
        blur(&mut self.shown, self.blur, &mut self.scratch);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, self.mirror)
    }
}

// ------------------------------------------------------------------ Scroll

struct Scroll {
    colours: [Rgb; 3],
    blur: f32,
    mirror: bool,
    speed: f32,
    decay: f32,
    threshold: f32,
    /// The picture as it scrolls, newest at the start.
    trail: Vec<Rgb>,
    /// Pixels owed to the scroll and not yet moved.
    carry: f32,
    shown: Vec<Rgb>,
    scratch: Vec<Rgb>,
}

impl Scroll {
    fn def() -> EffectDef {
        let [lows, mids, high] = colour_triple();
        EffectDef {
            info: info(
                "lf_scroll",
                "Scroll",
                "LedFx's classic: the lows, mids and highs flow out from the middle as colours, fading as they go.",
                true,
                FlashUse::None,
                vec![
                    lows,
                    mids,
                    high,
                    ParamSpec::float("speed", "Speed", 1.0, 10.0, 3.0),
                    ParamSpec::float("decay", "Fades by", 0.8, 1.0, 0.97),
                    ParamSpec::float("threshold", "Ignore quiet sounds", 0.0, 1.0, 0.0),
                    ParamSpec::float("blur", "Blur", 0.0, 10.0, 3.0),
                    ParamSpec::bool("mirror", "Mirror", true),
                ],
            ),
            make: || {
                Box::new(Scroll {
                    colours: [
                        Rgb::new(1.0, 0.0, 0.0),
                        Rgb::new(0.0, 1.0, 0.0),
                        Rgb::new(0.0, 0.0, 1.0),
                    ],
                    blur: 3.0,
                    mirror: true,
                    speed: 3.0,
                    decay: 0.97,
                    threshold: 0.0,
                    trail: vec![Rgb::BLACK; N],
                    carry: 0.0,
                    shown: vec![Rgb::BLACK; N],
                    scratch: Vec::new(),
                })
            },
        }
    }
}

impl Effect for Scroll {
    fn configure(&mut self, p: &ParamReader) {
        self.colours = [p.color("color_lows"), p.color("color_mids"), p.color("color_high")];
        self.speed = p.float("speed").round();
        self.decay = p.float("decay");
        self.threshold = p.float("threshold");
        self.blur = p.float("blur");
        self.mirror = p.bool("mirror");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let cutoffs = [self.threshold / 10.0, self.threshold / 8.0, self.threshold / 7.0];
        let mut head = Rgb::BLACK;
        for ((third, cutoff), colour) in thirds(&ctx.audio).iter().zip(cutoffs).zip(self.colours) {
            let level = max(third).powi(2).clamp(0.0, 1.0);
            if level >= cutoff {
                head = head.add(colour.scale(level));
            }
        }
        let head = head.clamp();
        self.carry += self.speed * per_frame(ctx.dt);
        let steps = (self.carry as usize).min(N);
        self.carry -= steps as f32;
        if steps > 0 {
            self.trail.rotate_right(steps);
            for px in &mut self.trail[..steps] {
                *px = head;
            }
        }
        let fade = self.decay.powf(per_frame(ctx.dt));
        for px in self.trail.iter_mut().skip(steps) {
            *px = px.scale(fade);
        }
        self.shown.clone_from(&self.trail);
        blur(&mut self.shown, self.blur, &mut self.scratch);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, self.mirror)
    }
}

// -------------------------------------------------------------- Wavelength

struct Wavelength {
    grad: Grad,
    blur: f32,
    bands: Vec<f32>,
    shown: Vec<Rgb>,
    scratch: Vec<Rgb>,
}

impl Wavelength {
    fn def() -> EffectDef {
        let [g, r] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_wavelength",
                "Wavelength",
                "LedFx's classic: the sound from low to high laid along the lights, each part lit as loud as it is.",
                true,
                FlashUse::None,
                vec![g, ParamSpec::float("blur", "Blur", 0.0, 10.0, 3.0), r],
            ),
            make: || {
                Box::new(Wavelength {
                    grad: Grad::new("lf_rainbow"),
                    blur: 3.0,
                    bands: vec![0.0; N],
                    shown: vec![Rgb::BLACK; N],
                    scratch: Vec::new(),
                })
            },
        }
    }
}

impl Effect for Wavelength {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.blur = p.float("blur");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        melbank(&ctx.audio, &mut self.bands);
        for (i, px) in self.shown.iter_mut().enumerate() {
            *px = self.grad.at(i as f32 / (N - 1) as f32).scale(self.bands[i]);
        }
        blur(&mut self.shown, self.blur, &mut self.scratch);
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, false)
    }
}

// ------------------------------------------------------------------- Power

struct Power {
    grad: Grad,
    blur: f32,
    mirror: bool,
    sparks_colour: Rgb,
    bass_decay: f32,
    sparks_decay: f32,
    bass_filter: ExpFilter,
    watch: HitWatch,
    sparks: Vec<Rgb>,
    bass: Vec<Rgb>,
    bands: Vec<f32>,
    shown: Vec<Rgb>,
    scratch: Vec<Rgb>,
    seed: u32,
}

impl Power {
    fn def() -> EffectDef {
        let [g, r] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_power",
                "Power",
                "LedFx's classic: the sound's colours along the lights, a bar that grows with the bass, and sparks on every hit.",
                true,
                FlashUse::None,
                vec![
                    g,
                    ParamSpec::color("sparks", "Colour of the sparks", "#ffffff"),
                    ParamSpec::float("bass_decay", "The bass bar fades by", 0.0, 1.0, 0.05),
                    ParamSpec::float("sparks_decay", "Sparks fade by", 0.0, 1.0, 0.15),
                    ParamSpec::bool("mirror", "Mirror", true),
                    ParamSpec::float("blur", "Blur", 0.0, 10.0, 0.0),
                    r,
                ],
            ),
            make: || {
                Box::new(Power {
                    grad: Grad::new("lf_rainbow"),
                    blur: 0.0,
                    mirror: true,
                    sparks_colour: Rgb::WHITE,
                    bass_decay: 0.05,
                    sparks_decay: 0.15,
                    bass_filter: ExpFilter::new(0.1, 0.8),
                    watch: HitWatch::default(),
                    sparks: vec![Rgb::BLACK; N],
                    bass: vec![Rgb::BLACK; N],
                    bands: vec![0.0; N],
                    shown: vec![Rgb::BLACK; N],
                    scratch: Vec::new(),
                    seed: 1,
                })
            },
        }
    }
}

impl Effect for Power {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.sparks_colour = p.color("sparks");
        self.bass_decay = p.float("bass_decay");
        self.sparks_decay = p.float("sparks_decay");
        self.mirror = p.bool("mirror");
        self.blur = p.float("blur");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let steps = per_frame(ctx.dt);
        let (sparks_fade, bass_fade) = (
            (1.0 - self.sparks_decay).powf(steps),
            (1.0 - self.bass_decay).powf(steps),
        );
        for px in &mut self.sparks {
            *px = px.scale(sparks_fade);
        }
        if self.watch.fired(&ctx.audio, true) {
            // A twentieth of the pixels, picked by chance.
            for _ in 0..N / 20 {
                self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                self.sparks[(self.seed >> 8) as usize % N] = self.sparks_colour;
            }
        }
        for px in &mut self.bass {
            *px = px.scale(bass_fade);
        }
        let level = self.bass_filter.update(ctx.audio.bass, ctx.dt);
        let reach = ((level * N as f32) as usize).min(N);
        let colour = self.grad.at(level);
        for px in &mut self.bass[..reach] {
            *px = colour;
        }
        melbank(&ctx.audio, &mut self.bands);
        for i in 0..N {
            let bg = self.grad.at(i as f32 / (N - 1) as f32).scale(self.bands[i]);
            self.shown[i] = bg.add(self.bass[i]).add(self.sparks[i]).clamp();
        }
        blur(&mut self.shown, self.blur, &mut self.scratch);
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, self.mirror)
    }
}

// -------------------------------------------------------------------- Rain

include!("ledfx_ripple.rs");

struct Drop {
    at: usize,
    frame: f32,
    colour: Rgb,
}

struct Rain {
    colours: [Rgb; 3],
    sensitivity: [f32; 3],
    mirror: bool,
    pulse: usize,
    filter: [ExpFilter; 3],
    drops: Vec<Drop>,
    pulse_colour: Rgb,
    shown: Vec<Rgb>,
    seed: u32,
}

impl Rain {
    fn def() -> EffectDef {
        EffectDef {
            info: info(
                "lf_rain",
                "Rain",
                "LedFx's classic: drops land and ripple out wherever the lows, mids or highs jump.",
                true,
                FlashUse::None,
                vec![
                    ParamSpec::color("lows_color", "Colour of the lows", "#ffffff"),
                    ParamSpec::color("mids_color", "Colour of the mids", "#ff0000"),
                    ParamSpec::color("high_color", "Colour of the highs", "#0000ff"),
                    ParamSpec::float("lows_sensitivity", "Sensitivity to the lows", 0.03, 0.3, 0.1),
                    ParamSpec::float("mids_sensitivity", "Sensitivity to the mids", 0.03, 0.3, 0.05),
                    ParamSpec::float("high_sensitivity", "Sensitivity to the highs", 0.03, 0.3, 0.1),
                    ParamSpec::choice(
                        "pulse",
                        "Pulse the whole light with",
                        &["Nothing", "The lows", "The mids", "The highs"],
                    ),
                    ParamSpec::bool("mirror", "Mirror", true),
                ],
            ),
            make: || {
                Box::new(Rain {
                    colours: [Rgb::WHITE, Rgb::new(1.0, 0.0, 0.0), Rgb::new(0.0, 0.0, 1.0)],
                    sensitivity: [0.1, 0.05, 0.1],
                    mirror: true,
                    pulse: 0,
                    filter: [ExpFilter::new(0.5, 0.99); 3],
                    drops: Vec::new(),
                    pulse_colour: Rgb::BLACK,
                    shown: vec![Rgb::BLACK; N],
                    seed: 7,
                })
            },
        }
    }
}

impl Effect for Rain {
    fn configure(&mut self, p: &ParamReader) {
        self.colours = [p.color("lows_color"), p.color("mids_color"), p.color("high_color")];
        self.sensitivity = [
            p.float("lows_sensitivity"),
            p.float("mids_sensitivity"),
            p.float("high_sensitivity"),
        ];
        self.pulse = p.choice("pulse");
        self.mirror = p.bool("mirror");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let steps = per_frame(ctx.dt);
        for drop in &mut self.drops {
            drop.frame += steps;
        }
        self.drops.retain(|d| (d.frame as usize) < RIPPLE.len() - 1);
        for (k, third) in thirds(&ctx.audio).iter().enumerate() {
            let level = max(third);
            let before = self.filter[k].value.unwrap_or(level);
            if level - before > self.sensitivity[k] {
                if self.pulse == k + 1 {
                    self.pulse_colour = self.colours[k];
                } else if self.drops.len() < 64 {
                    self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    self.drops.push(Drop {
                        at: (self.seed >> 8) as usize % N,
                        frame: 1.0,
                        colour: self.colours[k],
                    });
                }
            }
            self.filter[k].update(level, ctx.dt);
        }
        // Each drop's ripple, LedFx's own, centred where it landed.
        let width = RIPPLE[0].len();
        for px in &mut self.shown {
            *px = Rgb::BLACK;
        }
        for drop in &self.drops {
            let row = &RIPPLE[(drop.frame as usize).min(RIPPLE.len() - 1)];
            for (k, w) in row.iter().enumerate() {
                let at = drop.at as isize + k as isize - (width / 2) as isize;
                if (0..N as isize).contains(&at) && *w > 0.0 {
                    let px = &mut self.shown[at as usize];
                    *px = px.add(drop.colour.scale(*w));
                }
            }
        }
        for px in &mut self.shown {
            *px = px.add(self.pulse_colour).clamp();
        }
        self.pulse_colour = self.pulse_colour.scale(0.9f32.powf(steps));
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, self.mirror)
    }
}

// ------------------------------------------------------------------- Bands

struct Bands {
    grad: Grad,
    count: usize,
    align: usize,
    bands: Vec<f32>,
    shown: Vec<Rgb>,
}

impl Bands {
    fn def() -> EffectDef {
        let [g, r] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_bands",
                "Bands",
                "LedFx's classic: the lights split into level meters, one for each part of the sound, low to high.",
                true,
                FlashUse::None,
                vec![
                    g,
                    ParamSpec::float("count", "Bands", 1.0, 16.0, 6.0),
                    ParamSpec::choice(
                        "align",
                        "Each band grows from",
                        &["The left", "The right", "Both ends", "The middle"],
                    ),
                    r,
                ],
            ),
            make: || {
                Box::new(Bands {
                    grad: Grad::new("lf_rainbow"),
                    count: 6,
                    align: 0,
                    bands: vec![0.0; N],
                    shown: vec![Rgb::BLACK; N],
                })
            },
        }
    }
}

/// Splits `n` into `parts` runs as numpy's `array_split` does: the first
/// ones one longer when it does not divide.
fn split(n: usize, parts: usize) -> impl Iterator<Item = (usize, usize)> {
    let (base, extra) = (n / parts, n % parts);
    (0..parts).scan(0usize, move |start, i| {
        let len = base + usize::from(i < extra);
        let run = (*start, *start + len);
        *start += len;
        Some(run)
    })
}

impl Effect for Bands {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.count = p.float("count").round().clamp(1.0, 16.0) as usize;
        self.align = p.choice("align");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        melbank(&ctx.audio, &mut self.bands);
        for px in &mut self.shown {
            *px = Rgb::BLACK;
        }
        for (i, (start, end)) in split(N, self.count).enumerate() {
            let width = end - start;
            let level = max(&self.bands[start..end]);
            let lit = ((level * width as f32) as usize).min(width);
            let colour = self.grad.at(i as f32 / self.count as f32);
            let band = &mut self.shown[start..end];
            for px in &mut band[..lit] {
                *px = colour;
            }
            match self.align {
                // From the right.
                1 => band.reverse(),
                // Half at each end: LedFx's "invert".
                2 => band.rotate_left(lit / 2),
                // In the middle.
                3 => band.rotate_right((width - lit) / 2),
                _ => {}
            }
        }
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, false)
    }
}

// ------------------------------------------------------------------ Blocks

struct Blocks {
    grad: Grad,
    count: usize,
    bands: Vec<f32>,
    shown: Vec<Rgb>,
}

impl Blocks {
    fn def() -> EffectDef {
        let [g, r] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_blocks",
                "Blocks",
                "LedFx's classic: a few blocks of colour, each lit by its own part of the sound.",
                true,
                FlashUse::None,
                vec![g, ParamSpec::float("count", "Blocks", 1.0, 10.0, 4.0), r],
            ),
            make: || {
                Box::new(Blocks {
                    grad: Grad::new("lf_rainbow"),
                    count: 4,
                    bands: vec![0.0; N],
                    shown: vec![Rgb::BLACK; N],
                })
            },
        }
    }
}

impl Effect for Blocks {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.count = p.float("count").round().clamp(1.0, 10.0) as usize;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        melbank(&ctx.audio, &mut self.bands);
        for (i, (start, end)) in split(N, self.count).enumerate() {
            let level = max(&self.bands[start..end]);
            let colour = self.grad.at(i as f32 / self.count as f32);
            for j in start..end {
                self.shown[j] = colour.scale(self.bands[j] * level);
            }
        }
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, false)
    }
}

// --------------------------------------------------------------- Magnitude

struct Magnitude {
    grad: Grad,
    which: usize,
    level: f32,
}

impl Magnitude {
    fn def() -> EffectDef {
        let [g, r] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_magnitude",
                "Magnitude",
                "LedFx's classic: the whole gradient, as bright as one part of the sound is loud.",
                true,
                FlashUse::None,
                vec![g, ParamSpec::choice("range", "Follows", &POWERS), r],
            ),
            make: || {
                Box::new(Magnitude {
                    grad: Grad::new("lf_rainbow"),
                    which: 0,
                    level: 0.0,
                })
            },
        }
    }
}

impl Effect for Magnitude {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.which = p.choice("range");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.level = power(&ctx.audio, self.which);
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.grad.at(x).scale(self.level)
    }
}

// ---------------------------------------------------------- Pitch Spectrum

struct PitchSpectrum {
    grad: Grad,
    blur: f32,
    mirror: bool,
    fade: f32,
    responsiveness: f32,
    note: Option<f32>,
    bands: Vec<f32>,
    picture: Vec<Rgb>,
    shown: Vec<Rgb>,
    scratch: Vec<Rgb>,
}

impl PitchSpectrum {
    fn def() -> EffectDef {
        let [g, r] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_pitch_spectrum",
                "Pitch Spectrum",
                "LedFx's classic: the sound laid along the lights, coloured by the note being played.",
                true,
                FlashUse::None,
                vec![
                    g,
                    ParamSpec::float("fade", "Notes fade by", 0.0, 1.0, 0.15),
                    ParamSpec::float("responsiveness", "Follows a new note by", 0.0, 1.0, 0.15),
                    ParamSpec::bool("mirror", "Mirror", true),
                    ParamSpec::float("blur", "Blur", 0.0, 10.0, 1.0),
                    r,
                ],
            ),
            make: || {
                Box::new(PitchSpectrum {
                    grad: Grad::new("lf_rainbow"),
                    blur: 1.0,
                    mirror: true,
                    fade: 0.15,
                    responsiveness: 0.15,
                    note: None,
                    bands: vec![0.0; N],
                    picture: vec![Rgb::BLACK; N],
                    shown: vec![Rgb::BLACK; N],
                    scratch: Vec::new(),
                })
            },
        }
    }
}

impl Effect for PitchSpectrum {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.fade = p.float("fade");
        self.responsiveness = p.float("responsiveness");
        self.mirror = p.bool("mirror");
        self.blur = p.float("blur");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let steps = per_frame(ctx.dt);
        // Change: LedFx follows the note as a MIDI number between two
        // limits; here it is the note's place in the octave, which goes
        // round the gradient once from C to B.
        if ctx.audio.tonal > 0.05 {
            let heard = ctx.audio.pitch;
            let follow = 1.0 - (1.0 - self.responsiveness).powf(steps);
            self.note = Some(match self.note {
                None => heard,
                Some(old) => old + ((heard - old + 0.5).rem_euclid(1.0) - 0.5) * follow,
            });
        }
        let colour = self.note.map_or(Rgb::BLACK, |n| self.grad.at(n.rem_euclid(1.0)));
        melbank(&ctx.audio, &mut self.bands);
        let keep = (1.0 - self.fade).powf(steps);
        for i in 0..N {
            let b = self.bands[i];
            self.picture[i] = self.picture[i].scale(1.0 - b).add(colour.scale(b)).scale(keep);
        }
        self.shown.clone_from(&self.picture);
        blur(&mut self.shown, self.blur, &mut self.scratch);
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, self.mirror)
    }
}

// ---------------------------------------------------------------- Spectrum

struct SpectrumLf {
    mix: usize,
    filter: Vec<f32>,
    previous: Vec<f32>,
    bands: Vec<f32>,
    shown: Vec<Rgb>,
}

impl SpectrumLf {
    const MIXES: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];

    fn def() -> EffectDef {
        EffectDef {
            info: info(
                "lf_spectrum",
                "Spectrum (LedFx)",
                "LedFx's classic: red for the sound now, green for how fast it changes, blue for how it has been.",
                true,
                FlashUse::None,
                vec![ParamSpec::float("mix", "Colour order", 0.0, 5.0, 0.0)],
            ),
            make: || {
                Box::new(SpectrumLf {
                    mix: 0,
                    filter: vec![0.0; N],
                    previous: vec![0.0; N],
                    bands: vec![0.0; N],
                    shown: vec![Rgb::BLACK; N],
                })
            },
        }
    }
}

impl Effect for SpectrumLf {
    fn configure(&mut self, p: &ParamReader) {
        self.mix = p.float("mix").round().clamp(0.0, 5.0) as usize;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        melbank(&ctx.audio, &mut self.bands);
        let (rise, decay) = (ExpFilter::step(0.5, ctx.dt), ExpFilter::step(0.1, ctx.dt));
        let order = Self::MIXES[self.mix];
        for i in 0..N {
            let y = self.bands[i];
            let f = &mut self.filter[i];
            *f += (y - *f) * if y > *f { rise } else { decay };
            // LedFx multiplies by a thousand and lets the output clip.
            let channels = [y, (y - self.previous[i]).abs() * 8.0, *f].map(|v| (v * 3.9).clamp(0.0, 1.0));
            let mut c = [0.0; 3];
            for k in 0..3 {
                c[order[k]] = channels[k];
            }
            self.shown[i] = Rgb::new(c[0], c[1], c[2]);
            self.previous[i] = y;
        }
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, false)
    }
}

// --------------------------------------------------------------------- Bar

/// LedFx's ways for a bar to speed up and slow down over a beat.
const EASES: [&str; 4] = ["Slows at the end", "Speeds up", "Both", "Steady"];

fn ease(kind: usize, x: f32) -> f32 {
    match kind {
        1 => x * x,
        2 => 0.5 * (std::f32::consts::PI * (x - 0.5)).sin() + 0.5,
        3 => x,
        _ => 1.0 - (x - 1.0) * (x - 1.0),
    }
}

struct Bar {
    grad: Grad,
    mode: usize,
    ease: usize,
    step: f32,
    skip: usize,
    offset: f32,
    every: usize,
    last: f32,
    phase: bool,
    colour_at: f32,
    oscillator: f32,
    bar: usize,
}

impl Bar {
    fn def() -> EffectDef {
        let [g, _] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_bar",
                "Bar",
                "LedFx's classic: a bar of colour wipes or bounces across the lights once a beat, a new colour each time.",
                true,
                FlashUse::None,
                vec![
                    g,
                    ParamSpec::choice("mode", "Movement", &["Wipe", "Bounce", "In and out"]),
                    ParamSpec::choice("ease", "Speed through the beat", &EASES),
                    ParamSpec::float("step", "Colour change each beat", 0.0625, 0.5, 0.125),
                    ParamSpec::choice("skip", "Skip beats", &["None", "Odd", "Even"]).advanced(),
                    ParamSpec::float("offset", "Shift against the beat", 0.0, 1.0, 0.0).advanced(),
                    ParamSpec::choice("every", "When skipping, skip every", &["1", "2"]).advanced(),
                ],
            ),
            make: || {
                Box::new(Bar {
                    grad: Grad::new("lf_rainbow"),
                    mode: 0,
                    ease: 0,
                    step: 0.125,
                    skip: 0,
                    offset: 0.0,
                    every: 1,
                    last: 0.0,
                    phase: false,
                    colour_at: 0.0,
                    oscillator: 0.0,
                    bar: 0,
                })
            },
        }
    }
}

impl Effect for Bar {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure_fixed(p);
        self.mode = p.choice("mode");
        self.ease = p.choice("ease");
        self.step = p.float("step");
        self.skip = p.choice("skip");
        self.offset = p.float("offset");
        self.every = p.choice("every") + 1;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let bar = bar_oscillator(ctx);
        self.bar = bar as usize % 4;
        self.oscillator = (bar.fract() + self.offset).rem_euclid(1.0);
        if self.last > 0.2 && self.oscillator < 0.1 {
            self.phase = !self.phase;
            if !self.phase {
                self.colour_at = (self.colour_at + self.step).rem_euclid(1.0);
            }
        }
        self.last = self.oscillator;
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let e = ease(self.ease, self.oscillator);
        const LENGTH: f32 = 0.3;
        let (start, end) = match (self.mode, self.phase) {
            (0, false) => (0.0, e),
            (0, true) => (e, 1.0),
            (1, false) => {
                let at = e * (1.0 - LENGTH);
                (at, at + LENGTH)
            }
            (1, true) => {
                let at = e * (1.0 - LENGTH);
                (1.0 - (at + LENGTH), 1.0 - at)
            }
            (_, false) => (0.0, e),
            (_, true) => (0.0, 1.0 - e),
        };
        let skipped = self.skip != 0 && {
            let odd = if self.every == 2 {
                self.bar < 2
            } else {
                self.bar.is_multiple_of(2)
            };
            odd == (self.skip == 2)
        };
        if skipped || x < start || x >= end {
            Rgb::BLACK
        } else {
            self.grad.at(self.colour_at)
        }
    }
}

// ---------------------------------------------------------- Multicolor Bar

struct MultiBar {
    grad: Grad,
    cascade: bool,
    ease: usize,
    step: f32,
    watch: HitWatch,
    phase: bool,
    colour_at: f32,
    oscillator: f32,
    /// The last beat counted on the effect's own clock, without a tempo.
    own_beat: i64,
}

impl MultiBar {
    fn def() -> EffectDef {
        let [g, _] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_multibar",
                "Multicolor Bar",
                "LedFx's classic: each beat a new colour sweeps across the lights over the last one.",
                true,
                FlashUse::None,
                vec![
                    g,
                    ParamSpec::choice("mode", "Movement", &["Wipe", "Cascade"]),
                    ParamSpec::choice("ease", "Speed through the beat", &EASES),
                    ParamSpec::float("step", "Colour change each beat", 0.0625, 0.5, 0.125),
                ],
            ),
            make: || {
                Box::new(MultiBar {
                    grad: Grad::new("lf_rainbow"),
                    cascade: false,
                    ease: 3,
                    step: 0.125,
                    watch: HitWatch::default(),
                    phase: false,
                    colour_at: 0.0,
                    oscillator: 0.0,
                    own_beat: 0,
                })
            },
        }
    }
}

impl Effect for MultiBar {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure_fixed(p);
        self.cascade = p.choice("mode") == 1;
        self.ease = p.choice("ease");
        self.step = p.float("step");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.oscillator = bar_oscillator(ctx).fract();
        let beat = if ctx.audio.bpm > 0.0 {
            self.watch.fired(&ctx.audio, false)
        } else {
            // Without a tempo, a beat at 120 a minute.
            let whole = (ctx.time * 2.0).floor() as i64;
            std::mem::replace(&mut self.own_beat, whole) != whole
        };
        if beat {
            self.phase = !self.phase;
            self.colour_at = (self.colour_at + self.step).rem_euclid(1.0);
        }
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let e = ease(self.ease, self.oscillator);
        let (mut front, mut back) = (
            self.grad.at(self.colour_at),
            self.grad.at((self.colour_at + self.step).rem_euclid(1.0)),
        );
        let edge = if !self.cascade && self.phase {
            std::mem::swap(&mut front, &mut back);
            1.0 - e
        } else {
            e
        };
        if x < edge { back } else { front }
    }
}

// -------------------------------------------------------------- BPM Strobe

struct BpmStrobe {
    grad: Grad,
    per_beat: f32,
    strobe_decay: f32,
    beat_decay: f32,
    pattern: [bool; 4],
    level: f32,
    colour: Rgb,
}

impl BpmStrobe {
    const FREQUENCIES: [&'static str; 6] = ["1 a beat", "2 a beat", "4 a beat", "8 a beat", "16 a beat", "32 a beat"];
    const PATTERNS: [&'static str; 5] = ["Every beat", "Beats 1 and 3", "Beats 2 and 4", "Beat 1", "Beat 4"];

    fn def() -> EffectDef {
        let [g, _] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_bpm_strobe",
                "BPM Strobe",
                "LedFx's classic: flashes locked to the tempo, a set number to a beat, fading through each.",
                true,
                FlashUse::Capped,
                vec![
                    g,
                    ParamSpec::choice("frequency", "Flashes", &Self::FREQUENCIES),
                    ParamSpec::choice("pattern", "On", &Self::PATTERNS),
                    ParamSpec::float("strobe_decay", "Each flash fades by", 1.0, 10.0, 1.5),
                    ParamSpec::float("beat_decay", "Fading through the beat", 0.0, 10.0, 2.0),
                ],
            ),
            make: || {
                Box::new(BpmStrobe {
                    grad: Grad::new("lf_rainbow"),
                    per_beat: 1.0,
                    strobe_decay: 1.5,
                    beat_decay: 2.0,
                    pattern: [true; 4],
                    level: 0.0,
                    colour: Rgb::BLACK,
                })
            },
        }
    }
}

impl Effect for BpmStrobe {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure_fixed(p);
        self.per_beat = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0][p.choice("frequency").min(5)];
        self.pattern = match p.choice("pattern") {
            1 => [true, false, true, false],
            2 => [false, true, false, true],
            3 => [true, false, false, false],
            4 => [false, false, false, true],
            _ => [true; 4],
        };
        self.strobe_decay = p.float("strobe_decay");
        self.beat_decay = p.float("beat_decay");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let bar = bar_oscillator(ctx);
        let o = bar.fract();
        // Change: with the flash limit on, fewer flashes to a beat, so
        // they stay crisp instead of being smeared by the limit.
        let beats_a_second = if ctx.audio.bpm > 0.0 { ctx.audio.bpm / 60.0 } else { 2.0 };
        let mut per_beat = self.per_beat;
        while per_beat * beats_a_second > ctx.flash_cap && per_beat > 1.0 / 16.0 {
            per_beat /= 2.0;
        }
        let step = 1.0 / per_beat;
        let within = (-o).rem_euclid(step) * per_beat;
        let mut level = within.powf(self.strobe_decay) * (1.0 - o).powf(self.beat_decay);
        if per_beat < 1.0 {
            // Fewer than one a beat: only on the beats that start a group.
            let beat = (ctx.audio.beat_count as f32 * per_beat).fract();
            if beat > 1e-3 {
                level = 0.0;
            }
        }
        if !self.pattern[bar as usize % 4] {
            level = 0.0;
        }
        self.level = level * level;
        self.colour = self.grad.at(bar / 4.0);
    }

    fn shade(&self, _x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.colour.scale(self.level)
    }
}

// ------------------------------------------------------------------ Strobe

struct StrobeLf {
    grad: Grad,
    step: f32,
    bass_decay: f32,
    colour: Rgb,
    width: f32,
    decay: f32,
    shift_every: f32,
    bass_colour: Rgb,
    colour_at: f32,
    since_shift: f32,
    since_bass: f32,
    since_hit: f32,
    beat: HitWatch,
    hit: HitWatch,
    bass: Rgb,
    hits: Vec<Rgb>,
    seed: u32,
}

impl StrobeLf {
    fn def() -> EffectDef {
        let [g, _] = gradient_params("lf_dancefloor");
        EffectDef {
            info: info(
                "lf_strobe",
                "Strobe (LedFx)",
                "LedFx's classic: the whole light flashes a colour on each beat, and white blocks flash on every hit.",
                true,
                FlashUse::Capped,
                vec![
                    g,
                    ParamSpec::float("step", "Colour change each beat", 0.0, 0.25, 0.0625),
                    ParamSpec::float("bass_decay", "Beat flashes fade by", 0.0, 1.0, 0.5),
                    ParamSpec::color("colour", "Colour of the hits", "#ffffff"),
                    ParamSpec::float("width", "Width of a hit", 0.0, 1.0, 0.07),
                    ParamSpec::float("decay", "Hits fade by", 0.0, 1.0, 0.5),
                    ParamSpec::float("shift", "Seconds between colour changes", 0.0, 1.0, 1.0).advanced(),
                ],
            ),
            make: || {
                Box::new(StrobeLf {
                    grad: Grad::new("lf_dancefloor"),
                    step: 0.0625,
                    bass_decay: 0.5,
                    colour: Rgb::WHITE,
                    width: 0.07,
                    decay: 0.5,
                    shift_every: 1.0,
                    bass_colour: Rgb::BLACK,
                    colour_at: 0.0,
                    since_shift: 0.0,
                    since_bass: f32::MAX,
                    since_hit: f32::MAX,
                    beat: HitWatch::default(),
                    hit: HitWatch::default(),
                    bass: Rgb::BLACK,
                    hits: vec![Rgb::BLACK; N],
                    seed: 3,
                })
            },
        }
    }
}

impl Effect for StrobeLf {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure_fixed(p);
        self.step = p.float("step");
        self.bass_decay = p.float("bass_decay");
        self.colour = p.color("colour");
        // Change: LedFx gives the width in pixels (10 of them); here it is
        // a share of the light, which is about the same on a 150 strip.
        self.width = p.float("width");
        self.decay = p.float("decay");
        self.shift_every = p.float("shift");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let steps = per_frame(ctx.dt);
        for t in [&mut self.since_shift, &mut self.since_bass, &mut self.since_hit] {
            *t = (*t + ctx.dt).min(1e6);
        }
        if self.since_shift > self.shift_every {
            self.colour_at = (self.colour_at + self.step).rem_euclid(1.0);
            self.bass_colour = self.grad.at(self.colour_at);
            self.since_shift = 0.0;
        }
        self.bass = self.bass.scale((1.0 - self.bass_decay).powf(steps));
        for px in &mut self.hits {
            *px = px.scale((1.0 - self.decay).powf(steps));
        }
        // No two flashes closer than LedFx's fifth of a second, nor than
        // the flash limit allows.
        let gap = 0.2f32.max(1.0 / ctx.flash_cap);
        if self.beat.fired(&ctx.audio, false) && self.since_bass > gap && self.bass_decay > 0.0 {
            self.bass = self.bass_colour;
            self.since_bass = 0.0;
        }
        if self.hit.fired(&ctx.audio, true) && self.since_hit > 1.0 / ctx.flash_cap {
            let width = ((self.width * N as f32) as usize).clamp(1, N);
            self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let at = if width >= N {
                0
            } else {
                (self.seed >> 8) as usize % (N - width)
            };
            for px in &mut self.hits[at..at + width] {
                *px = self.colour;
            }
            self.since_hit = 0.0;
        }
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.bass.add(sample(&self.hits, x, false)).clamp()
    }
}

// -------------------------------------------------------------------- Scan

struct Scan {
    grad: Grad,
    colour: Rgb,
    use_grad: bool,
    which: usize,
    multiplier: f32,
    speed: f32,
    width: f32,
    count: usize,
    bounce: bool,
    intensity: bool,
    mirror: bool,
    blur: f32,
    at: f32,
    returning: bool,
    power: f32,
    shown: Vec<Rgb>,
    scratch: Vec<Rgb>,
}

impl Scan {
    fn def() -> EffectDef {
        let [g, r] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_scan",
                "Scan",
                "LedFx's classic: a bar sweeps to and fro, pushed faster and lit brighter by the lows.",
                true,
                FlashUse::None,
                vec![
                    ParamSpec::color("colour", "Colour", "#ff0000"),
                    ParamSpec::choice("range", "Pushed by", &POWERS),
                    ParamSpec::float("speed", "Speed", 0.0, 100.0, 50.0),
                    ParamSpec::float("width", "Width", 1.0, 100.0, 30.0),
                    ParamSpec::float("multiplier", "How hard the sound pushes", 0.0, 5.0, 3.0),
                    ParamSpec::float("count", "Bars", 1.0, 10.0, 1.0).advanced(),
                    ParamSpec::bool("bounce", "Bounce at the ends", true).advanced(),
                    ParamSpec::bool("intensity", "Brighter when louder", true).advanced(),
                    ParamSpec::bool("use_grad", "Colour from the gradient", false).advanced(),
                    g,
                    ParamSpec::bool("mirror", "Mirror", false).advanced(),
                    ParamSpec::float("blur", "Blur", 0.0, 10.0, 3.0).advanced(),
                    r,
                ],
            ),
            make: || {
                Box::new(Scan {
                    grad: Grad::new("lf_rainbow"),
                    colour: Rgb::new(1.0, 0.0, 0.0),
                    use_grad: false,
                    which: 0,
                    multiplier: 3.0,
                    speed: 50.0,
                    width: 30.0,
                    count: 1,
                    bounce: true,
                    intensity: true,
                    mirror: false,
                    blur: 3.0,
                    at: 0.0,
                    returning: false,
                    power: 0.0,
                    shown: vec![Rgb::BLACK; N],
                    scratch: Vec::new(),
                })
            },
        }
    }
}

impl Effect for Scan {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.colour = p.color("colour");
        self.which = p.choice("range");
        self.speed = p.float("speed");
        self.width = p.float("width");
        self.multiplier = p.float("multiplier");
        self.count = p.float("count").round().clamp(1.0, 10.0) as usize;
        self.bounce = p.bool("bounce");
        self.intensity = p.bool("intensity");
        self.use_grad = p.bool("use_grad");
        self.mirror = p.bool("mirror");
        self.blur = p.float("blur");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.power = power(&ctx.audio, self.which) * 2.0;
        let block = N as f32 / self.count as f32;
        let width = (block / 100.0 * self.width).max(1.0);
        let step = ctx.dt * N as f32 / 100.0 * self.speed * self.power * self.multiplier;
        self.at += if self.returning { -step } else { step };
        if self.bounce {
            if self.at > N as f32 - width {
                self.returning = true;
                self.at = N as f32 - width;
            }
            if self.at < 0.0 {
                self.returning = false;
                self.at = 0.0;
            }
        } else {
            self.at = self.at.rem_euclid(N as f32);
            self.returning = false;
        }
        let mut colour = if self.use_grad {
            self.grad.at((self.at / N as f32).rem_euclid(1.0))
        } else {
            self.colour
        };
        if self.intensity {
            colour = colour.scale(self.power.min(1.0));
        }
        for px in &mut self.shown {
            *px = Rgb::BLACK;
        }
        for k in 0..self.count {
            let start = block * k as f32 + self.at;
            for i in start as usize..(start + width) as usize {
                // A bar that runs off the end comes round to the start.
                self.shown[i % N] = colour;
            }
        }
        blur(&mut self.shown, self.blur, &mut self.scratch);
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        sample(&self.shown, x, self.mirror)
    }
}

// ------------------------------------------------- the HSV family's state

/// What every effect of the HSV family keeps: its gradient and pixels,
/// how fast and how much it reacts, and the lows it hears.
struct HsvState {
    grad: Grad,
    hsv: Hsv,
    speed: f32,
    reactivity: f32,
    lows: ExpFilter,
    level: f32,
}

impl HsvState {
    fn new(decay: f32, rise: f32, speed: f32, reactivity: f32) -> Self {
        Self {
            grad: Grad::new("lf_rainbow"),
            hsv: Hsv::new(),
            speed,
            reactivity,
            lows: ExpFilter::new(decay, rise),
            level: 0.0,
        }
    }

    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
        self.speed = p.float("speed");
        self.reactivity = p.float("reactivity");
        self.hsv.fix_hues = p.bool("fix_hues");
    }

    fn hear(&mut self, ctx: &FrameCtx) {
        self.level = self.lows.update(ctx.audio.bass, ctx.dt);
    }

    fn finish(&mut self, dt: f32) {
        self.hsv.colour(&self.grad);
        self.grad.advance(dt);
    }

    fn shade(&self, x: f32) -> Rgb {
        sample(&self.hsv.out, x, false)
    }
}

fn hsv_def(id: &'static str, name: &'static str, description: &'static str, speed: f32, reactivity: f32) -> EffectInfo {
    info(
        id,
        name,
        description,
        true,
        FlashUse::None,
        hsv_params(speed, reactivity),
    )
}

fn place(i: usize) -> f32 {
    i as f32 / (N - 1) as f32
}

// ---------------------------------------------------------------- Energy 2

struct Energy2 {
    state: HsvState,
}

impl Energy2 {
    fn def() -> EffectDef {
        EffectDef {
            info: hsv_def(
                "lf_energy2",
                "Energy 2",
                "LedFx's atmospheric: soft waves of colour that roll along, pushed and whitened by the lows.",
                0.1,
                0.2,
            ),
            make: || {
                Box::new(Energy2 {
                    state: HsvState::new(0.1, 0.1, 0.1, 0.2),
                })
            },
        }
    }
}

impl Effect for Energy2 {
    fn configure(&mut self, p: &ParamReader) {
        self.state.configure(p);
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let s = &mut self.state;
        s.hear(ctx);
        let t1 = sawtooth(s.speed, ctx.time);
        let push = 2.0 * wave(t1 + s.reactivity * s.level);
        let limit = 0.9 - (s.reactivity + 0.3) * s.level;
        for i in 0..N {
            let v = triangle((place(i) + push).rem_euclid(1.0)).powi(2);
            s.hsv.v[i] = v;
            s.hsv.s[i] = f32::from(v < limit);
            s.hsv.h[i] = s.level + t1;
        }
        s.finish(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.state.shade(x)
    }
}

// -------------------------------------------------------------------- Melt

struct Melt {
    state: HsvState,
    clock: f64,
}

impl Melt {
    fn def() -> EffectDef {
        EffectDef {
            info: hsv_def(
                "lf_melt",
                "Melt",
                "LedFx's atmospheric: colours that melt into each other, flowing faster with the lows.",
                0.5,
                0.5,
            ),
            make: || {
                Box::new(Melt {
                    state: HsvState::new(0.1, 0.1, 0.5, 0.5),
                    clock: 0.0,
                })
            },
        }
    }
}

impl Effect for Melt {
    fn configure(&mut self, p: &ParamReader) {
        self.state.configure(p);
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let s = &mut self.state;
        s.hear(ctx);
        // The lows push its clock ahead, as in LedFx.
        self.clock += ctx.dt as f64 * (1.0 + (s.level * s.reactivity / s.speed.max(1e-3)) as f64 * RATE as f64 / 60.0);
        let t1 = sawtooth(s.speed * 5.0, self.clock);
        let t2 = sawtooth(s.speed * 6.5, self.clock);
        for i in 0..N {
            let c = 1.0 - place(i);
            let v = wave(wave(wave(c) + t1) + t1);
            s.hsv.h[i] = c + t2;
            s.hsv.s[i] = 1.0;
            s.hsv.v[i] = v * v;
        }
        s.finish(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.state.shade(x)
    }
}

// ---------------------------------------------------------------- Marching

struct Marching {
    state: HsvState,
}

impl Marching {
    fn def() -> EffectDef {
        EffectDef {
            info: hsv_def(
                "lf_marching",
                "Marching",
                "LedFx's atmospheric: bands of colour that march along the lights, stepping with the lows.",
                0.1,
                0.2,
            ),
            make: || {
                Box::new(Marching {
                    state: HsvState::new(0.05, 0.2, 0.1, 0.2),
                })
            },
        }
    }
}

impl Effect for Marching {
    fn configure(&mut self, p: &ParamReader) {
        self.state.configure(p);
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let s = &mut self.state;
        s.hear(ctx);
        let t1 = sawtooth(s.speed * 20.0, ctx.time);
        let t2 = sawtooth(s.speed, ctx.time) + s.reactivity * s.level * 20.0;
        for i in 0..N {
            let w2 = place(i);
            let w1 = wave(t1 + w2);
            s.hsv.h[i] = wave(wave(w1 - w2));
            s.hsv.s[i] = 1.0;
            s.hsv.v[i] = w1 - wave(t2 + 2.0 - w2 * 10.0);
        }
        s.finish(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.state.shade(x)
    }
}

// ----------------------------------------------------------------- Crawler

struct Crawler {
    state: HsvState,
    sway: f32,
    chop: f32,
    stretch: f32,
    clock: f64,
}

impl Crawler {
    fn def() -> EffectDef {
        let mut info = hsv_def(
            "lf_crawler",
            "Crawler",
            "LedFx's atmospheric: little creatures of colour that crawl and wriggle along the lights.",
            0.5,
            0.25,
        );
        info.params.extend([
            ParamSpec::float("sway", "Sway", 0.00001, 50.0, 20.0).advanced(),
            ParamSpec::float("chop", "Chop", 0.00001, 100.0, 30.0).advanced(),
            ParamSpec::float("stretch", "Stretch", 0.00001, 10.0, 2.5).advanced(),
        ]);
        EffectDef {
            info,
            make: || {
                Box::new(Crawler {
                    state: HsvState::new(0.1, 0.1, 0.5, 0.25),
                    sway: 20.0,
                    chop: 30.0,
                    stretch: 2.5,
                    clock: 0.0,
                })
            },
        }
    }
}

impl Effect for Crawler {
    fn configure(&mut self, p: &ParamReader) {
        self.state.configure(p);
        self.sway = p.float("sway");
        self.chop = p.float("chop");
        self.stretch = p.float("stretch");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let s = &mut self.state;
        s.hear(ctx);
        self.clock += ctx.dt as f64 * (1.0 + (s.level * s.reactivity * s.speed) as f64);
        let t1 = sawtooth(s.speed * self.sway, self.clock);
        let t3 = sawtooth(s.speed * self.chop + s.level * s.reactivity, ctx.time);
        for i in 0..N {
            let h = ((i as f32 + t3 * N as f32) / N as f32 * self.stretch).rem_euclid(self.stretch / 10.0)
                + place(i)
                + wave(t1);
            // As in LedFx, the brightness follows the hue itself.
            let v = wave(h);
            s.hsv.h[i] = h;
            s.hsv.s[i] = 1.0;
            s.hsv.v[i] = v * v;
        }
        s.finish(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.state.shade(x)
    }
}

// ------------------------------------------------------- Block Reflections

struct BlockReflections {
    state: HsvState,
}

impl BlockReflections {
    fn def() -> EffectDef {
        EffectDef {
            info: hsv_def(
                "lf_block_reflections",
                "Block Reflections",
                "LedFx's atmospheric: blocks of colour that fold and reflect into each other, nudged by the lows.",
                0.5,
                0.5,
            ),
            make: || {
                Box::new(BlockReflections {
                    state: HsvState::new(0.05, 0.05, 0.5, 0.5),
                })
            },
        }
    }
}

impl Effect for BlockReflections {
    fn configure(&mut self, p: &ParamReader) {
        self.state.configure(p);
    }

    fn update(&mut self, ctx: &FrameCtx) {
        let s = &mut self.state;
        s.hear(ctx);
        let pi2 = std::f32::consts::PI * std::f32::consts::PI;
        let t2 = sawtooth(s.speed, ctx.time) * pi2 + 0.8 * s.reactivity * s.level;
        let t1 = sawtooth(s.speed, ctx.time);
        let t3 = sawtooth(5.0 * s.speed, ctx.time) + s.reactivity * s.level;
        let t4 = sawtooth(2.0 * s.speed, ctx.time) * pi2;
        let m = 0.3 + triangle(t1) * 0.2;
        let c = triangle(t3) * 10.0 + 4.0 * wave(t4);
        for i in 0..N {
            let h = ((i as f32 - N as f32 / 2.0) / N as f32 * c).rem_euclid(m) + wave(t2);
            let v = (h.abs() + m.abs() + t1).rem_euclid(1.0);
            s.hsv.h[i] = h;
            s.hsv.s[i] = 1.0;
            s.hsv.v[i] = v * v;
        }
        s.finish(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.state.shade(x)
    }
}

// ---------------------------------------------- Gradient, Fade, Rainbow

struct Gradient {
    grad: Grad,
}

impl Gradient {
    fn def() -> EffectDef {
        let [g, _] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_gradient",
                "Gradient",
                "LedFx's non-reactive: a gradient laid along the lights, slowly rolling if asked. Does not react to sound.",
                false,
                FlashUse::None,
                vec![g, ParamSpec::float("roll", "Roll the gradient", 0.0, 10.0, 1.0)],
            ),
            make: || {
                Box::new(Gradient {
                    grad: Grad::new("lf_rainbow"),
                })
            },
        }
    }
}

impl Effect for Gradient {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure(p);
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.grad.advance(ctx.dt);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.grad.at(x)
    }
}

struct Fade {
    grad: Grad,
    speed: f32,
    at: f32,
}

impl Fade {
    fn def() -> EffectDef {
        let [g, _] = gradient_params("lf_rainbow");
        EffectDef {
            info: info(
                "lf_fade",
                "Fade",
                "LedFx's non-reactive: every light the same colour, fading slowly through the gradient and back. Does not react to sound.",
                false,
                FlashUse::None,
                vec![g, ParamSpec::float("speed", "Speed", 0.1, 10.0, 0.5)],
            ),
            make: || {
                Box::new(Fade {
                    grad: Grad::new("lf_rainbow"),
                    speed: 0.5,
                    at: 0.0,
                })
            },
        }
    }
}

impl Effect for Fade {
    fn configure(&mut self, p: &ParamReader) {
        self.grad.configure_fixed(p);
        self.speed = p.float("speed");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // LedFx steps 0.0015 ten times a second for each unit of speed,
        // there and back.
        self.at = (self.at + ctx.dt * 0.015 * self.speed).rem_euclid(2.0);
    }

    fn shade(&self, _x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        self.grad.at(if self.at < 1.0 { self.at } else { 2.0 - self.at })
    }
}

struct RainbowLf {
    speed: f32,
    frequency: f32,
    hue: f32,
}

impl RainbowLf {
    fn def() -> EffectDef {
        EffectDef {
            info: info(
                "lf_rainbow",
                "Rainbow (LedFx)",
                "LedFx's non-reactive: a rainbow that slowly moves along the lights. Does not react to sound.",
                false,
                FlashUse::None,
                vec![
                    ParamSpec::float("speed", "Speed", 0.1, 20.0, 1.0),
                    ParamSpec::float("frequency", "Rainbows across", 0.1, 64.0, 1.0),
                ],
            ),
            make: || {
                Box::new(RainbowLf {
                    speed: 1.0,
                    frequency: 1.0,
                    hue: 0.1,
                })
            },
        }
    }
}

impl Effect for RainbowLf {
    fn configure(&mut self, p: &ParamReader) {
        self.speed = p.float("speed");
        self.frequency = p.float("frequency");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // LedFx moves the hue on by a hundredth ten times a second for
        // each unit of speed.
        self.hue = (self.hue + ctx.dt * 0.1 * self.speed).rem_euclid(1.0);
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        Rgb::from_hsv(self.hue + x * self.frequency, 1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::find;
    use crate::params::{ParamValue, Params};

    fn ctx(time: f64, audio: AudioFeatures) -> FrameCtx {
        FrameCtx {
            time,
            dt: 1.0 / 60.0,
            audio,
            beats: audio.beat_count as f64 + audio.beat_phase as f64,
            aspect: 1.0,
            flat: true,
            flash_cap: f32::INFINITY,
            light: 0,
            lights: 1,
        }
    }

    fn make(id: &str, params: &Params) -> Box<dyn Effect> {
        let def = find(id).unwrap_or_else(|| panic!("{id} is registered"));
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

    fn lit(fx: &dyn Effect, c: &FrameCtx) -> usize {
        (0..200)
            .filter(|i| brightness(fx.shade(*i as f32 / 199.0, 0.5, c)) > 0.05)
            .count()
    }

    /// Sound with only the lowest fifth of the bands, or only the top half, loud.
    fn only(low: bool, level: f32) -> AudioFeatures {
        let mut a = AudioFeatures {
            active: true,
            ..AudioFeatures::default()
        };
        for (i, b) in a.bands.iter_mut().enumerate() {
            let in_lows = i < BANDS / 5;
            let in_highs = i >= BANDS / 2;
            *b = if (low && in_lows) || (!low && in_highs) {
                level
            } else {
                0.0
            };
        }
        a.bass = if low { level } else { 0.0 };
        a.treble = if low { 0.0 } else { level };
        a
    }

    fn run(fx: &mut Box<dyn Effect>, c: &FrameCtx, frames: usize) {
        for _ in 0..frames {
            fx.update(c);
        }
    }

    #[test]
    fn every_ported_effect_is_in_its_section_and_stays_in_range() {
        let defs = all();
        assert_eq!(defs.len(), 23);
        for def in &defs {
            assert_eq!(def.info.category, FROM_LEDFX);
            assert!(def.info.id.starts_with("lf_"), "{}", def.info.id);
            let mut fx = (def.make)();
            fx.configure(&ParamReader::new(&def.info.params, &Params::new()));
            for (t, audio) in [
                (0.0, AudioFeatures::default()),
                (1.3, only(true, 1.0)),
                (2.7, only(false, 0.8)),
            ] {
                let mut c = ctx(t, audio);
                c.audio.bpm = 120.0;
                c.audio.beat_count = (t * 2.0) as u32;
                c.audio.beat_phase = (t * 2.0).fract() as f32;
                c.audio.onset = 1.0;
                c.audio.beat = 1.0;
                for _ in 0..30 {
                    fx.update(&c);
                }
                for i in 0..50 {
                    let px = fx.shade(i as f32 / 49.0, 0.5, &c);
                    for v in [px.r, px.g, px.b] {
                        assert!((0.0..=1.0).contains(&v), "{} gave {px:?}", def.info.id);
                    }
                }
            }
        }
    }

    #[test]
    fn energy_grows_from_the_middle_in_the_colour_of_what_is_loud() {
        let mut fx = make("lf_energy", &with(&[("blur", ParamValue::Number(0.0))]));
        let quiet = ctx(0.0, AudioFeatures::default());
        run(&mut fx, &quiet, 30);
        assert_eq!(lit(fx.as_ref(), &quiet), 0, "dark in silence");
        let bass = ctx(0.0, only(true, 0.6));
        run(&mut fx, &bass, 60);
        // Lows only: red, from the middle outward, the ends dark.
        let middle = fx.shade(0.5, 0.5, &bass);
        assert!(middle.r > 0.9 && middle.g < 0.05 && middle.b < 0.05, "{middle:?}");
        assert!(brightness(fx.shade(0.0, 0.5, &bass)) < 0.05);
        assert_eq!(fx.shade(0.4, 0.5, &bass), fx.shade(0.6, 0.5, &bass), "mirrored");
        // Louder: further out.
        let reach = lit(fx.as_ref(), &bass);
        let louder = ctx(0.0, only(true, 1.0));
        run(&mut fx, &louder, 60);
        assert!(lit(fx.as_ref(), &louder) > reach);
        // Highs only: blue.
        let highs = ctx(0.0, only(false, 0.9));
        run(&mut fx, &highs, 120);
        let middle = fx.shade(0.5, 0.5, &highs);
        assert!(middle.b > 0.9 && middle.r < 0.05, "{middle:?}");
    }

    #[test]
    fn scroll_sends_the_sound_outward_and_lets_it_fade() {
        let mut fx = make("lf_scroll", &with(&[("blur", ParamValue::Number(0.0))]));
        let bass = ctx(0.0, only(true, 1.0));
        run(&mut fx, &bass, 3);
        let quiet = ctx(0.0, AudioFeatures::default());
        run(&mut fx, &quiet, 5);
        // The burst has left the middle and is on its way out, red.
        let middle = brightness(fx.shade(0.5, 0.5, &quiet));
        let out = (0..100)
            .map(|i| fx.shade(0.5 + i as f32 / 200.0, 0.5, &quiet))
            .max_by(|a, b| brightness(*a).total_cmp(&brightness(*b)))
            .unwrap();
        assert!(middle < 0.05 && out.r > 0.5 && out.b < 0.05, "{middle} {out:?}");
        run(&mut fx, &quiet, 400);
        assert_eq!(lit(fx.as_ref(), &quiet), 0, "gone in the end");
    }

    #[test]
    fn bar_wipes_once_a_beat_and_changes_colour_every_other() {
        let mut fx = make("lf_bar", &with(&[("ease", ParamValue::Text("Steady".into()))]));
        let music = |t: f64| AudioFeatures {
            bpm: 120.0,
            beat_count: (t * 2.0) as u32,
            beat_phase: (t * 2.0).fract() as f32,
            bar_beat: ((t * 2.0) as u32 % 4) as u8,
            active: true,
            ..AudioFeatures::default()
        };
        // Played frame by frame from the start, as the lights would.
        let mut clock = 0.0f64;
        let mut at = |fx: &mut Box<dyn Effect>, t: f64| {
            while clock < t {
                clock += 1.0 / 60.0;
                fx.update(&ctx(clock, music(clock)));
            }
            lit(fx.as_ref(), &ctx(clock, music(clock)))
        };
        // Through the first beat the bar grows from nothing to nearly all.
        let early = at(&mut fx, 0.05);
        let late = at(&mut fx, 0.45);
        assert!(early < 40 && late > 160, "{early} {late}");
        // In the next beat it shrinks away from the start.
        let early2 = at(&mut fx, 0.55);
        let late2 = at(&mut fx, 0.95);
        assert!(early2 > 160 && late2 < 40, "{early2} {late2}");
    }

    #[test]
    fn bpm_strobe_flashes_on_the_beat_and_keeps_to_the_flash_limit() {
        let mut fx = make(
            "lf_bpm_strobe",
            &with(&[("frequency", ParamValue::Text("4 a beat".into()))]),
        );
        let mut flashes = 0;
        let mut was = false;
        for frame in 0..600 {
            let t = frame as f64 / 60.0;
            let mut c = ctx(
                t,
                AudioFeatures {
                    bpm: 120.0,
                    beat_count: (t * 2.0) as u32,
                    beat_phase: (t * 2.0).fract() as f32,
                    bar_beat: ((t * 2.0) as u32 % 4) as u8,
                    active: true,
                    ..AudioFeatures::default()
                },
            );
            c.flash_cap = 3.0;
            fx.update(&c);
            let on = brightness(fx.shade(0.5, 0.5, &c)) > 0.3;
            flashes += u32::from(on && !was);
            was = on;
        }
        // Eight a second asked for; the limit allows three, so two a beat
        // is too many and it falls to one: twenty in ten seconds.
        assert!((15..=30).contains(&flashes), "{flashes}");
    }

    #[test]
    fn rain_drops_where_the_sound_jumps_and_ripples_out() {
        let mut fx = make("lf_rain", &with(&[("mirror", ParamValue::Bool(false))]));
        let quiet = ctx(0.0, AudioFeatures::default());
        run(&mut fx, &quiet, 10);
        assert_eq!(lit(fx.as_ref(), &quiet), 0);
        run(&mut fx, &ctx(0.0, only(true, 1.0)), 1);
        let first = lit(fx.as_ref(), &quiet);
        assert!(first > 0, "a drop has landed");
        run(&mut fx, &quiet, 12);
        assert!(lit(fx.as_ref(), &quiet) > first, "its ripple has spread");
        run(&mut fx, &quiet, 80);
        assert_eq!(lit(fx.as_ref(), &quiet), 0, "and died away");
    }

    #[test]
    fn bands_are_meters_side_by_side() {
        let mut fx = make("lf_bands", &with(&[("count", ParamValue::Number(2.0))]));
        let bass = ctx(0.0, only(true, 1.0));
        run(&mut fx, &bass, 2);
        // Two bands: the low one (left half) is lit from its start, the
        // high one (right half) is dark.
        assert!(brightness(fx.shade(0.02, 0.5, &bass)) > 0.5);
        assert!(brightness(fx.shade(0.75, 0.5, &bass)) < 0.05);
    }

    #[test]
    fn the_hsv_family_moves_and_the_lows_push_it() {
        for id in [
            "lf_melt",
            "lf_energy2",
            "lf_marching",
            "lf_crawler",
            "lf_block_reflections",
        ] {
            let mut quiet_fx = make(id, &Params::new());
            let mut loud_fx = make(id, &Params::new());
            let picture = |fx: &Box<dyn Effect>, c: &FrameCtx| -> Vec<Rgb> {
                (0..40).map(|i| fx.shade(i as f32 / 39.0, 0.5, c)).collect()
            };
            let quiet = ctx(10.0, AudioFeatures::default());
            run(&mut quiet_fx, &quiet, 1);
            let first = picture(&quiet_fx, &quiet);
            assert!(
                first.iter().any(|p| brightness(*p) > 0.1),
                "{id} shows something in silence"
            );
            let later = ctx(13.0, AudioFeatures::default());
            run(&mut quiet_fx, &later, 180);
            assert_ne!(picture(&quiet_fx, &later), first, "{id} moves");
            let loud = ctx(13.0, only(true, 1.0));
            run(&mut loud_fx, &ctx(10.0, AudioFeatures::default()), 1);
            run(&mut loud_fx, &loud, 180);
            assert_ne!(
                picture(&loud_fx, &loud),
                picture(&quiet_fx, &later),
                "{id} hears the lows"
            );
        }
    }

    #[test]
    fn ledfx_blur_softens_and_mirror_folds_from_the_middle() {
        let mut picture = vec![Rgb::BLACK; N];
        picture[N / 2] = Rgb::WHITE;
        let mut scratch = Vec::new();
        blur(&mut picture, 2.0, &mut scratch);
        let total: f32 = picture.iter().map(|p| p.r).sum();
        assert!((total - 1.0).abs() < 1e-3, "no light gained or lost: {total}");
        assert!(picture[N / 2 + 2].r > 0.05 && picture[N / 2 + 2].r < picture[N / 2].r);
        // Mirror: the start of the picture is in the middle, its end at both ends.
        let ramp: Vec<Rgb> = (0..N).map(|i| Rgb::new(i as f32 / (N - 1) as f32, 0.0, 0.0)).collect();
        assert!(sample(&ramp, 0.5, true).r < 0.01);
        assert!(sample(&ramp, 0.0, true).r > 0.99 && sample(&ramp, 1.0, true).r > 0.99);
        assert!((sample(&ramp, 0.25, true).r - sample(&ramp, 0.75, true).r).abs() < 1e-5);
    }
}
