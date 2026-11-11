//! The effect interface.
//!
//! An effect is asked for a colour at each pixel's position. Positions are
//! normalised to 0..1 across everything the layer targets, so one effect can
//! sweep across several fixtures and never needs to know about wiring.

use serde::Serialize;

use crate::audio::AudioFeatures;
use crate::color::Rgb;
use crate::params::{ParamReader, ParamSpec};

/// What an effect knows about the current frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameCtx {
    /// Seconds since the engine started.
    pub time: f64,
    /// Seconds since the previous frame.
    pub dt: f32,
    pub audio: AudioFeatures,
    /// Position in the music, counted in beats: whole beats plus how far
    /// into the current one. Only steady while `audio.bpm` is above zero.
    pub beats: f64,
    /// Width divided by height of the area being drawn on. Lets radial
    /// effects draw circles instead of ellipses.
    pub aspect: f32,
    /// True when the targets form a line (a single strip, say), so there is
    /// no second dimension to draw bars or rings in.
    pub flat: bool,
    /// The most flashes a second this effect may make by itself while the
    /// show's flash limit is on. Infinite when there is no limit. Only
    /// effects whose job is to flash need to look at it.
    pub flash_cap: f32,
}

impl FrameCtx {
    /// Distance from the centre, 0 at the centre and about 1 at the far
    /// edge of the longer side, corrected for aspect.
    pub fn radius(&self, x: f32, y: f32) -> f32 {
        if self.flat {
            return (x - 0.5).abs() * 2.0;
        }
        let (mut dx, mut dy) = (x - 0.5, y - 0.5);
        if self.aspect >= 1.0 {
            dy /= self.aspect;
        } else {
            dx *= self.aspect;
        }
        (dx * dx + dy * dy).sqrt() * 2.0
    }
}

pub trait Effect: Send {
    /// Reads parameters. Called before every frame, so keep it cheap.
    fn configure(&mut self, params: &ParamReader);

    /// Advances any state once per frame, before `shade` is called.
    fn update(&mut self, _ctx: &FrameCtx) {}

    /// Colour at a position, both coordinates 0..1. `y` is 0 at the top.
    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb;
}

/// What the UI needs to list an effect and build its controls.
#[derive(Clone, Debug, Serialize)]
pub struct EffectInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// The group the effect is listed under in the picker.
    pub category: &'static str,
    /// True when the effect does something with sound.
    pub audio: bool,
    /// What the effect has to do with fast flashing.
    pub flash: FlashUse,
    pub params: Vec<ParamSpec>,
}

/// How an effect relates to the show's flash limit. The limit softens
/// whatever flashes too fast, whichever effect made it; this says what an
/// effect does about that itself, so nothing has to be designed around
/// the limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashUse {
    /// Does not set out to flash.
    #[default]
    None,
    /// Flashes on purpose, and slows itself to the limit while it is on,
    /// so its flashes stay crisp and evenly spaced.
    Capped,
    /// Only makes sense flashing fast. Switched off while the limit is on.
    Needed,
}

/// Picker groups, in the order they are shown.
pub const FOLLOWS_SOUND: &str = "Follows the sound";
pub const HITS_AND_BEATS: &str = "Hits and beats";
pub const AMBIENT: &str = "Ambient";

pub struct EffectDef {
    pub info: EffectInfo,
    pub make: fn() -> Box<dyn Effect>,
}

/// Cheap repeatable noise in 0..1 from two integers.
pub fn hash2(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth noise in 0..1 over a plane: neighbouring points get similar
/// values, unlike `hash2`. Good for flames and clouds.
pub fn noise2(x: f32, y: f32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (xi, yi) = (xf as i32 as u32, yf as i32 as u32);
    // Ease between the four corners of the cell.
    let (u, v) = (x - xf, y - yf);
    let (u, v) = (u * u * (3.0 - 2.0 * u), v * v * (3.0 - 2.0 * v));
    let top = hash2(xi, yi) * (1.0 - u) + hash2(xi.wrapping_add(1), yi) * u;
    let bottom = hash2(xi, yi.wrapping_add(1)) * (1.0 - u) + hash2(xi.wrapping_add(1), yi.wrapping_add(1)) * u;
    top * (1.0 - v) + bottom * v
}

/// Tells effects that wait for a hit when one has landed: on each beat,
/// or on any hit in the sound. Remembers what it needs between frames.
#[derive(Clone, Copy, Debug)]
pub struct HitWatch {
    last_beat: u32,
    armed: bool,
}

impl Default for HitWatch {
    fn default() -> Self {
        Self {
            last_beat: 0,
            armed: true,
        }
    }
}

impl HitWatch {
    /// True on the frame a beat begins (`any_hit` false) or a hit lands
    /// (`any_hit` true). Fires once per hit, however long it rings.
    pub fn fired(&mut self, audio: &AudioFeatures, any_hit: bool) -> bool {
        let beat = audio.beat_count != self.last_beat;
        self.last_beat = audio.beat_count;
        let hit = self.armed && audio.onset > 0.9;
        if hit {
            self.armed = false;
        } else if audio.onset < 0.5 {
            self.armed = true;
        }
        if any_hit { hit } else { beat }
    }
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() < f32::EPSILON {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
