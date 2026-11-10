//! Audio features that effects react to.
//!
//! The analysis that fills this in lives in `ols-audio`. Every value is
//! already normalised, so effects never deal with raw sample levels.

use serde::Serialize;

/// Number of frequency bands, spaced evenly on a mel scale from low to high.
pub const BANDS: usize = 32;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AudioFeatures {
    /// Overall loudness, 0..1, after automatic gain.
    pub level: f32,
    /// Loudness of the lowest bands (kick, bass), 0..1.
    pub bass: f32,
    /// Loudness of the middle bands (vocals, guitars), 0..1.
    pub mid: f32,
    /// Loudness of the highest bands (hi-hats, cymbals), 0..1.
    pub treble: f32,
    /// Per-band loudness, low to high, 0..1.
    pub bands: [f32; BANDS],
    /// Jumps to 1 when a new sound starts, then fades.
    pub onset: f32,
    /// Jumps to 1 on each beat, then fades.
    pub beat: f32,
    /// Position inside the current beat, 0..1.
    pub beat_phase: f32,
    /// Estimated tempo in beats per minute, 0 when unknown.
    pub bpm: f32,
    /// Number of beats since capture started.
    pub beat_count: u32,
    /// False when the input is silent.
    pub active: bool,
}

impl Default for AudioFeatures {
    fn default() -> Self {
        Self {
            level: 0.0,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            bands: [0.0; BANDS],
            onset: 0.0,
            beat: 0.0,
            beat_phase: 0.0,
            bpm: 0.0,
            beat_count: 0,
            active: false,
        }
    }
}

impl AudioFeatures {
    /// Band loudness at a position 0..1 across the spectrum, interpolated.
    pub fn band_at(&self, t: f32) -> f32 {
        let f = t.clamp(0.0, 1.0) * (BANDS - 1) as f32;
        let i = f.floor() as usize;
        let j = (i + 1).min(BANDS - 1);
        let k = f - i as f32;
        self.bands[i] * (1.0 - k) + self.bands[j] * k
    }
}
