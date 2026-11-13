//! Audio features that effects react to.
//!
//! The analysis that fills this in lives in `darkrgb-audio`. Every value is
//! already normalised, so effects never deal with raw sample levels.

use serde::{Deserialize, Serialize};

/// Number of frequency bands, spaced evenly on a mel scale from low to high.
pub const BANDS: usize = 32;

/// Lowest and highest frequencies the bands cover, in Hz.
pub const BAND_MIN_HZ: f32 = 40.0;
pub const BAND_MAX_HZ: f32 = 16_000.0;
/// Bands centred below this are bass; above the next, treble.
pub const BASS_MAX_HZ: f32 = 250.0;
pub const TREBLE_MIN_HZ: f32 = 2_500.0;

/// The three groups the bands are sorted into.
pub const BASS: usize = 0;
pub const MID: usize = 1;
pub const TREBLE: usize = 2;

pub fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

pub fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

/// Frequency at the lower edge of band `i`; `band_edge_hz(BANDS)` is the
/// top of the last band. `top_hz` lets the analysis lower the top edge for
/// low sample rates.
pub fn band_edge_hz(i: usize, top_hz: f32) -> f32 {
    let (lo, hi) = (hz_to_mel(BAND_MIN_HZ), hz_to_mel(top_hz));
    mel_to_hz(lo + (hi - lo) * i as f32 / BANDS as f32)
}

/// Which group (bass, mid or treble) band `b` belongs to.
pub fn group_of_band(b: usize) -> usize {
    let centre = (band_edge_hz(b, BAND_MAX_HZ) + band_edge_hz(b + 1, BAND_MAX_HZ)) / 2.0;
    if centre < BASS_MAX_HZ {
        BASS
    } else if centre > TREBLE_MIN_HZ {
        TREBLE
    } else {
        MID
    }
}

/// First band and one past the last band of a group.
pub fn group_bands(group: usize) -> (usize, usize) {
    let first = (0..BANDS).find(|b| group_of_band(*b) == group).unwrap_or(0);
    let end = (first..BANDS).find(|b| group_of_band(*b) != group).unwrap_or(BANDS);
    (first, end)
}

/// The part of the spectrum a layer listens to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Band {
    /// Everything.
    #[default]
    All,
    Bass,
    Mid,
    Treble,
    /// A custom range. `low` and `high` are positions across the bands,
    /// 0 at the lowest and 1 at the highest.
    Range {
        low: f32,
        high: f32,
    },
}

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
    /// The same, for hits in the bass, mids and treble separately.
    pub onsets: [f32; 3],
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
    /// How big this part of the song is against the last minute or so,
    /// 0..1: low in a breakdown, high in a chorus or after a drop. Unlike
    /// `level`, it is not evened out, so it tells quiet parts from loud.
    pub intensity: f32,
    /// Goes up by one each time the song moves to a clearly louder or
    /// quieter part.
    pub section: u32,
    /// Jumps to 1 when the song moves to a louder part, then fades over
    /// a couple of seconds.
    pub drop: f32,
    /// Which beat of the bar this is, 0 for the first. Bars are four
    /// beats long.
    pub bar_beat: u8,
    /// The beat is being kept from memory: the music's own beat cannot be
    /// heard just now (a breakdown, a pause), and the clock runs on at
    /// the tempo it had.
    pub coasting: bool,
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
            onsets: [0.0; 3],
            beat: 0.0,
            beat_phase: 0.0,
            bpm: 0.0,
            beat_count: 0,
            active: false,
            // With nothing known about the song, nothing is held back.
            intensity: 1.0,
            section: 0,
            drop: 0.0,
            bar_beat: 0,
            coasting: false,
        }
    }
}

impl AudioFeatures {
    /// What a layer that ignores the music hears: nothing, and no beat.
    /// Only hits fired by hand reach it.
    pub fn silenced(&self) -> AudioFeatures {
        AudioFeatures {
            // Present, so effects do not fall back to their no-sound look.
            active: true,
            ..AudioFeatures::default()
        }
    }

    /// Band loudness at a position 0..1 across the spectrum, interpolated.
    pub fn band_at(&self, t: f32) -> f32 {
        let f = t.clamp(0.0, 1.0) * (BANDS - 1) as f32;
        let i = f.floor() as usize;
        let j = (i + 1).min(BANDS - 1);
        let k = f - i as f32;
        self.bands[i] * (1.0 - k) + self.bands[j] * k
    }

    /// What a layer that listens only to `band` hears, with `gain` applied.
    ///
    /// Everything an effect reads follows the choice: `level` becomes the
    /// loudness of that part of the spectrum, `bands` are that part spread
    /// across the full width, groups outside it go silent, and `onset`
    /// fires on hits inside it. The beat is left alone, because it follows
    /// the tempo of the whole song.
    pub fn routed(&self, band: Band, gain: f32) -> AudioFeatures {
        let gain = if gain.is_finite() { gain.clamp(0.0, 8.0) } else { 1.0 };
        let (low, high) = match band {
            Band::All => (0.0, 1.0),
            Band::Bass | Band::Mid | Band::Treble => {
                let group = match band {
                    Band::Bass => BASS,
                    Band::Mid => MID,
                    _ => TREBLE,
                };
                let (first, end) = group_bands(group);
                (first as f32 / (BANDS - 1) as f32, (end - 1) as f32 / (BANDS - 1) as f32)
            }
            Band::Range { low, high } => {
                let low = if low.is_finite() { low.clamp(0.0, 1.0) } else { 0.0 };
                let high = if high.is_finite() { high.clamp(0.0, 1.0) } else { 1.0 };
                (low.min(high), low.max(high))
            }
        };

        let mut out = *self;
        if band != Band::All {
            // Bands inside the range, by index.
            let first = (low * (BANDS - 1) as f32).round() as usize;
            let last = ((high * (BANDS - 1) as f32).round() as usize).max(first);
            let heard = [BASS, MID, TREBLE].map(|g| {
                let (a, b) = group_bands(g);
                first < b && last >= a
            });

            out.level = match band {
                Band::Bass => self.bass,
                Band::Mid => self.mid,
                Band::Treble => self.treble,
                _ => {
                    let inside = &self.bands[first..=last];
                    let max = inside.iter().copied().fold(0.0f32, f32::max);
                    let mean = inside.iter().sum::<f32>() / inside.len() as f32;
                    0.6 * max + 0.4 * mean
                }
            };
            for (i, slot) in out.bands.iter_mut().enumerate() {
                *slot = self.band_at(low + (high - low) * i as f32 / (BANDS - 1) as f32);
            }
            if !heard[BASS] {
                out.bass = 0.0;
            }
            if !heard[MID] {
                out.mid = 0.0;
            }
            if !heard[TREBLE] {
                out.treble = 0.0;
            }
            out.onset = (0..3).filter(|g| heard[*g]).map(|g| self.onsets[g]).fold(0.0, f32::max);
            for (onset, heard) in out.onsets.iter_mut().zip(heard) {
                if !heard {
                    *onset = 0.0;
                }
            }
        }

        if gain != 1.0 {
            let scale = |v: &mut f32| *v = (*v * gain).clamp(0.0, 1.0);
            scale(&mut out.level);
            scale(&mut out.bass);
            scale(&mut out.mid);
            scale(&mut out.treble);
            out.bands.iter_mut().for_each(scale);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AudioFeatures {
        let mut bands = [0.0f32; BANDS];
        for (i, b) in bands.iter_mut().enumerate() {
            *b = i as f32 / (BANDS - 1) as f32;
        }
        AudioFeatures {
            level: 0.5,
            bass: 0.2,
            mid: 0.4,
            treble: 0.9,
            bands,
            onset: 1.0,
            onsets: [1.0, 0.0, 0.3],
            beat: 1.0,
            beat_phase: 0.25,
            bpm: 120.0,
            beat_count: 7,
            active: true,
            intensity: 0.6,
            section: 2,
            drop: 0.4,
            bar_beat: 3,
            coasting: false,
        }
    }

    #[test]
    fn the_groups_split_the_bands_in_order_with_none_left_out() {
        let (b0, b1) = group_bands(BASS);
        let (m0, m1) = group_bands(MID);
        let (t0, t1) = group_bands(TREBLE);
        assert_eq!(b0, 0);
        assert_eq!(b1, m0);
        assert_eq!(m1, t0);
        assert_eq!(t1, BANDS);
        assert!(b1 >= 2 && m1 - m0 >= 4 && t1 - t0 >= 4, "{b1} {m0}..{m1} {t0}..{t1}");
    }

    #[test]
    fn band_edges_rise_from_40_hz_to_16_khz() {
        assert!((band_edge_hz(0, BAND_MAX_HZ) - 40.0).abs() < 0.5);
        assert!((band_edge_hz(BANDS, BAND_MAX_HZ) - 16_000.0).abs() < 5.0);
        assert!((0..BANDS).all(|i| band_edge_hz(i, BAND_MAX_HZ) < band_edge_hz(i + 1, BAND_MAX_HZ)));
    }

    #[test]
    fn listening_to_everything_changes_nothing() {
        let a = sample();
        let r = a.routed(Band::All, 1.0);
        assert_eq!(r.level, a.level);
        assert_eq!(r.bands, a.bands);
        assert_eq!(r.onset, a.onset);
    }

    #[test]
    fn listening_to_highs_hears_only_highs() {
        let a = sample();
        let r = a.routed(Band::Treble, 1.0);
        assert_eq!(r.level, 0.9);
        assert_eq!((r.bass, r.mid, r.treble), (0.0, 0.0, 0.9));
        // The onset is the treble's, not the kick's.
        assert_eq!(r.onset, 0.3);
        assert_eq!(r.onsets, [0.0, 0.0, 0.3]);
        // The treble's bands are spread across the full width.
        let (first, end) = group_bands(TREBLE);
        assert!((r.bands[0] - a.bands[first]).abs() < 1e-5);
        assert!((r.bands[BANDS - 1] - a.bands[end - 1]).abs() < 1e-5);
        // The beat still follows the whole song.
        assert_eq!((r.beat, r.beat_count, r.bpm), (1.0, 7, 120.0));
    }

    #[test]
    fn listening_to_bass_fires_on_the_kick() {
        let r = sample().routed(Band::Bass, 1.0);
        assert_eq!(r.level, 0.2);
        assert_eq!(r.onset, 1.0);
        assert_eq!((r.mid, r.treble), (0.0, 0.0));
    }

    #[test]
    fn a_custom_range_hears_the_groups_it_touches() {
        let a = sample();
        // The top quarter of the spectrum: treble only.
        let r = a.routed(Band::Range { low: 0.75, high: 1.0 }, 1.0);
        assert_eq!((r.bass, r.mid), (0.0, 0.0));
        assert_eq!(r.treble, 0.9);
        assert!(
            r.level > 0.8,
            "the loud top bands should give a high level, got {}",
            r.level
        );
        assert!((r.bands[0] - 0.75).abs() < 0.03 && (r.bands[BANDS - 1] - 1.0).abs() < 1e-5);
        // Reversed or out-of-range ends are put right.
        let swapped = a.routed(Band::Range { low: 9.0, high: 0.75 }, 1.0);
        assert_eq!(swapped.bands, r.bands);
        // A range of one band still works.
        let one = a.routed(Band::Range { low: 0.5, high: 0.5 }, 1.0);
        assert!(one.level.is_finite() && one.bands.iter().all(|b| b.is_finite()));
        let nan = a.routed(
            Band::Range {
                low: f32::NAN,
                high: f32::NAN,
            },
            f32::NAN,
        );
        assert!(nan.level.is_finite());
    }

    #[test]
    fn gain_scales_and_clamps() {
        let a = sample();
        let quiet = a.routed(Band::All, 0.5);
        assert_eq!(quiet.level, 0.25);
        assert_eq!(quiet.treble, 0.45);
        let loud = a.routed(Band::All, 4.0);
        assert_eq!(loud.level, 1.0);
        assert!(loud.bands.iter().all(|b| *b <= 1.0));
    }
}
