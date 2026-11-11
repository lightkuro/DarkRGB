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
    pub params: Vec<ParamSpec>,
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

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() < f32::EPSILON {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
