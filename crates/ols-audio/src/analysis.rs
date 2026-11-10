//! Turns audio samples into the features effects react to.
//!
//! This is plain signal processing with no audio device involved, so it can
//! be tested with synthetic signals and reused later to analyse song files.
//!
//! Per step (about every 11 ms):
//!
//! 1. A windowed FFT gives the spectrum, which is summed into mel-spaced bands.
//! 2. Automatic gain follows the loudest recent band, so quiet and loud
//!    sources produce the same picture.
//! 3. The rise in band energy since the last step ("spectral flux") marks
//!    onsets: the starts of notes and drum hits.
//! 4. The rhythm of those onsets over the last few seconds gives the tempo,
//!    and a beat clock is kept in step with it.

use std::collections::VecDeque;
use std::sync::Arc;

use ols_core::{AudioFeatures, BANDS};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Lowest and highest frequencies covered by the bands, in Hz.
const BAND_MIN_HZ: f32 = 40.0;
const BAND_MAX_HZ: f32 = 16_000.0;
/// Bands centred below this are "bass", above the next are "treble".
const BASS_MAX_HZ: f32 = 250.0;
const TREBLE_MIN_HZ: f32 = 2_500.0;

/// A band this far below the loudest one shows as zero.
const BAND_RANGE_DB: f32 = 30.0;
const LEVEL_RANGE_DB: f32 = 24.0;
/// Input quieter than this (about -80 dBFS) counts as silence.
const SILENCE_RMS: f32 = 1e-4;
/// Gain never amplifies beyond what this reference allows, so background
/// hiss is not blown up into a full-scale display.
const MIN_REFERENCE: f32 = 3e-3;

const TEMPO_MIN_BPM: f32 = 70.0;
const TEMPO_MAX_BPM: f32 = 190.0;
/// Seconds of onset history the tempo estimate looks at.
const TEMPO_WINDOW_S: f32 = 6.0;
/// How strongly periodic the onsets must be before the beat clock is trusted.
const TEMPO_MIN_CONFIDENCE: f32 = 0.12;
/// Onsets needed inside the tempo window before a tempo is believed.
const TEMPO_MIN_ONSETS: usize = 6;

pub struct Analyzer {
    sample_rate: f32,
    n: usize,
    hop: usize,
    /// Analysis steps per second.
    step_rate: f32,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    /// Scales FFT magnitudes so a full-scale sine reads 1.0.
    magnitude_scale: f32,
    pending: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    /// First and one-past-last FFT bin of each band.
    band_bins: [(usize, usize); BANDS],
    /// 0 bass, 1 mid, 2 treble.
    band_class: [u8; BANDS],

    sensitivity: f32,
    band_ref: f32,
    level_ref: f32,
    prev_log: [f32; BANDS],
    quiet_steps: usize,

    flux_history: VecDeque<f32>,
    steps_since_onset: usize,
    /// How many steps ago each recent onset happened is derived from this
    /// running step counter.
    step_index: u64,
    recent_onsets: VecDeque<u64>,

    /// Onset strength per step, oldest first.
    onset_strength: VecDeque<f32>,
    steps_since_tempo: usize,
    /// Beat length in steps; 0 while unknown.
    period: f32,
    confidence: f32,
    candidate: f32,
    candidate_votes: u32,
    phase: f32,
    steps_since_beat: usize,

    out: AudioFeatures,
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(8_000) as f32;
        // About 43 ms of audio per FFT whatever the sample rate (2048
        // samples at 44.1 and 48 kHz), advancing a quarter of that each step.
        let n = ((sample_rate * 0.04) as usize).next_power_of_two().max(512);
        let hop = n / 4;
        let step_rate = sample_rate / hop as f32;

        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect();
        let magnitude_scale = 2.0 / window.iter().sum::<f32>();

        let nyquist = sample_rate / 2.0;
        let top = BAND_MAX_HZ.min(nyquist * 0.95);
        let (mel_lo, mel_hi) = (hz_to_mel(BAND_MIN_HZ), hz_to_mel(top));
        let edge_hz = |i: usize| mel_to_hz(mel_lo + (mel_hi - mel_lo) * i as f32 / BANDS as f32);
        let hz_per_bin = sample_rate / n as f32;
        let mut band_bins = [(0usize, 0usize); BANDS];
        let mut band_class = [1u8; BANDS];
        for b in 0..BANDS {
            let lo = ((edge_hz(b) / hz_per_bin).round() as usize).clamp(1, n / 2 - 1);
            let hi = ((edge_hz(b + 1) / hz_per_bin).round() as usize).clamp(lo + 1, n / 2);
            band_bins[b] = (lo, hi);
            let centre = (edge_hz(b) + edge_hz(b + 1)) / 2.0;
            band_class[b] = if centre < BASS_MAX_HZ {
                0
            } else if centre > TREBLE_MIN_HZ {
                2
            } else {
                1
            };
        }

        Self {
            sample_rate,
            n,
            hop,
            step_rate,
            fft: FftPlanner::new().plan_fft_forward(n),
            window,
            magnitude_scale,
            pending: Vec::with_capacity(n * 4),
            spectrum: vec![Complex::default(); n],
            scratch: vec![Complex::default(); n],
            band_bins,
            band_class,
            sensitivity: 1.0,
            band_ref: MIN_REFERENCE,
            level_ref: MIN_REFERENCE,
            prev_log: [0.0; BANDS],
            quiet_steps: usize::MAX / 2,
            flux_history: VecDeque::new(),
            steps_since_onset: usize::MAX / 2,
            step_index: 0,
            recent_onsets: VecDeque::new(),
            onset_strength: VecDeque::new(),
            steps_since_tempo: 0,
            period: 0.0,
            confidence: 0.0,
            candidate: 0.0,
            candidate_votes: 0,
            phase: 0.0,
            steps_since_beat: usize::MAX / 2,
            out: AudioFeatures::default(),
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Seconds of audio each analysis step covers.
    pub fn step_seconds(&self) -> f32 {
        1.0 / self.step_rate
    }

    /// Multiplier applied on top of automatic gain, 0.25..4.
    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity.clamp(0.25, 4.0);
    }

    pub fn features(&self) -> AudioFeatures {
        self.out
    }

    /// Feeds mono samples in. Returns how many analysis steps ran.
    pub fn push(&mut self, samples: &[f32]) -> usize {
        self.pending.extend_from_slice(samples);
        let mut steps = 0;
        let mut start = 0;
        while self.pending.len() - start >= self.n {
            self.step(start);
            start += self.hop;
            steps += 1;
        }
        if start > 0 {
            self.pending.drain(..start);
        }
        steps
    }

    /// Call when no audio has arrived for `seconds`. Some systems deliver
    /// nothing at all while nothing is playing; this fades the features out
    /// so the lights do not freeze on the last value.
    pub fn idle(&mut self, seconds: f32) {
        let steps = (seconds * self.step_rate).ceil() as usize;
        let silence = vec![0.0f32; self.hop];
        for _ in 0..steps.min(self.step_rate as usize) {
            self.pending.extend_from_slice(&silence);
        }
        // Drop the partial window so stale samples are not analysed later.
        let mut start = 0;
        while self.pending.len() - start >= self.n {
            self.step(start);
            start += self.hop;
        }
        self.pending.drain(..start);
    }

    fn step(&mut self, start: usize) {
        let frame = &self.pending[start..start + self.n];

        // Loudness of the newest samples only, so it responds immediately.
        let newest = &frame[self.n - self.hop..];
        let rms = (newest.iter().map(|s| s * s).sum::<f32>() / self.hop as f32).sqrt();

        for (slot, (sample, w)) in self.spectrum.iter_mut().zip(frame.iter().zip(&self.window)) {
            *slot = Complex::new(sample * w, 0.0);
        }
        self.fft.process_with_scratch(&mut self.spectrum, &mut self.scratch);

        let mut bands = [0.0f32; BANDS];
        for (b, (lo, hi)) in self.band_bins.iter().enumerate() {
            let power: f32 = self.spectrum[*lo..*hi].iter().map(|c| c.norm_sqr()).sum();
            bands[b] = power.sqrt() * self.magnitude_scale;
        }

        if rms < SILENCE_RMS {
            self.quiet_steps = self.quiet_steps.saturating_add(1);
        } else {
            self.quiet_steps = 0;
        }
        let active = (self.quiet_steps as f32) < self.step_rate * 0.4;

        // Automatic gain: rise quickly to a new peak, fall back over seconds.
        let release = (-1.0 / (self.step_rate * 6.0)).exp();
        let peak = bands.iter().copied().fold(0.0f32, f32::max);
        self.band_ref = follow(self.band_ref, peak, release);
        self.level_ref = follow(self.level_ref, rms, release);

        let mut flux = 0.0f32;
        // Rise per group (bass, mid, treble), for the tempo tracker.
        let mut group_flux = [0.0f32; 3];
        let mut group_size = [0u32; 3];
        for b in 0..BANDS {
            let relative = bands[b] / self.band_ref;
            let log = (1.0 + 50.0 * relative).ln();
            let rise = (log - self.prev_log[b]).max(0.0);
            self.prev_log[b] = log;
            flux += rise;
            group_flux[self.band_class[b] as usize] += rise;
            group_size[self.band_class[b] as usize] += 1;

            let target = if active {
                (to_unit(relative, BAND_RANGE_DB).powf(1.4) * self.sensitivity).min(1.0)
            } else {
                0.0
            };
            let current = &mut self.out.bands[b];
            let rate = if target > *current { 0.7 } else { 0.18 };
            *current += (target - *current) * rate;
        }

        let target = if active {
            (to_unit(rms / self.level_ref, LEVEL_RANGE_DB).powf(1.2) * self.sensitivity).min(1.0)
        } else {
            0.0
        };
        let rate = if target > self.out.level { 0.6 } else { 0.12 };
        self.out.level += (target - self.out.level) * rate;

        let mut sums = [(0.0f32, 0.0f32, 0u32); 3];
        for b in 0..BANDS {
            let slot = &mut sums[self.band_class[b] as usize];
            slot.0 += self.out.bands[b];
            slot.1 = slot.1.max(self.out.bands[b]);
            slot.2 += 1;
        }
        let blend = |(sum, max, count): (f32, f32, u32)| {
            if count == 0 {
                0.0
            } else {
                0.6 * max + 0.4 * sum / count as f32
            }
        };
        self.out.bass = blend(sums[0]);
        self.out.mid = blend(sums[1]);
        self.out.treble = blend(sums[2]);
        self.out.active = active;

        let onset = active && self.detect_onset(flux);
        self.step_index += 1;
        if onset {
            self.recent_onsets.push_back(self.step_index);
        }
        let horizon = (self.step_rate * TEMPO_WINDOW_S) as u64;
        while self.recent_onsets.front().is_some_and(|at| self.step_index - at > horizon) {
            self.recent_onsets.pop_front();
        }
        let fade = (-1.0 / (self.step_rate * 0.10)).exp();
        self.out.onset = if onset { 1.0 } else { self.out.onset * fade };

        // Beats are carried mostly by kick and bass, while hi-hats tick on
        // the off-beats too. Weighting the groups keeps a busy hi-hat from
        // being mistaken for the beat.
        let per_band = |g: usize| group_flux[g] / group_size[g].max(1) as f32;
        let rhythm = 2.0 * per_band(0) + per_band(1) + 0.4 * per_band(2);
        self.track_beat(if active { rhythm } else { 0.0 }, onset, fade);
    }

    /// True when this step's flux stands out from the last second and a
    /// half, and the previous onset was not a moment ago.
    fn detect_onset(&mut self, flux: f32) -> bool {
        let history = (self.step_rate * 1.5) as usize;
        let n = self.flux_history.len().max(1) as f32;
        let mean = self.flux_history.iter().sum::<f32>() / n;
        let var = self.flux_history.iter().map(|f| (f - mean) * (f - mean)).sum::<f32>() / n;
        self.flux_history.push_back(flux);
        while self.flux_history.len() > history {
            self.flux_history.pop_front();
        }
        self.steps_since_onset = self.steps_since_onset.saturating_add(1);
        let threshold = mean + 2.0 * var.sqrt() + 0.5;
        let rested = self.steps_since_onset as f32 > self.step_rate * 0.06;
        if flux > threshold && rested {
            self.steps_since_onset = 0;
            true
        } else {
            false
        }
    }

    fn track_beat(&mut self, strength: f32, onset: bool, fade: f32) {
        let window = (self.step_rate * TEMPO_WINDOW_S) as usize;
        self.onset_strength.push_back(strength);
        while self.onset_strength.len() > window {
            self.onset_strength.pop_front();
        }

        self.steps_since_tempo += 1;
        if self.steps_since_tempo as f32 >= self.step_rate / 4.0 {
            self.steps_since_tempo = 0;
            self.estimate_tempo(window);
        }

        self.steps_since_beat = self.steps_since_beat.saturating_add(1);
        // A couple of hits that happen to be evenly spaced are not a tempo.
        let enough_hits = self.recent_onsets.len() >= TEMPO_MIN_ONSETS;
        let locked =
            self.out.active && enough_hits && self.period > 0.0 && self.confidence >= TEMPO_MIN_CONFIDENCE;
        let mut beat = false;
        if locked {
            self.phase += 1.0 / self.period;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                // A phase correction can cross a beat boundary twice; never
                // fire two beats closer than a fraction of the period.
                beat = self.steps_since_beat as f32 > self.period * 0.4;
            }
            self.out.bpm = 60.0 * self.step_rate / self.period;
            self.out.beat_phase = self.phase;
        } else {
            // No steady tempo: let strong hits stand in for beats so
            // beat-driven effects still move.
            beat = onset && self.steps_since_beat as f32 > self.step_rate * 0.25;
            self.out.bpm = 0.0;
            self.out.beat_phase = (self.steps_since_beat as f32 / (self.step_rate * 0.5)).min(1.0);
        }
        if beat {
            self.steps_since_beat = 0;
            self.out.beat = 1.0;
            self.out.beat_count = self.out.beat_count.wrapping_add(1);
        } else {
            self.out.beat *= fade;
        }
    }

    /// Finds the beat length that best explains the recent onsets, then
    /// nudges the beat clock toward where those onsets say the beat is.
    fn estimate_tempo(&mut self, window: usize) {
        let len = self.onset_strength.len();
        if !self.out.active || len < window / 2 {
            if !self.out.active {
                self.period = 0.0;
                self.confidence = 0.0;
                self.candidate_votes = 0;
            }
            return;
        }
        let mean = self.onset_strength.iter().sum::<f32>() / len as f32;
        let x: Vec<f32> = self.onset_strength.iter().map(|v| (v - mean).max(0.0)).collect();
        let energy: f32 = x.iter().map(|v| v * v).sum::<f32>() / len as f32;
        if energy <= 1e-9 {
            self.confidence = 0.0;
            return;
        }

        let autocorr = |lag: usize| -> f32 {
            if lag >= len {
                return 0.0;
            }
            x[lag..].iter().zip(&x[..len - lag]).map(|(a, b)| a * b).sum::<f32>() / (len - lag) as f32
        };
        let lag_min = (self.step_rate * 60.0 / TEMPO_MAX_BPM).floor() as usize;
        let lag_max = ((self.step_rate * 60.0 / TEMPO_MIN_BPM).ceil() as usize).min(len / 2);
        if lag_min < 2 || lag_max <= lag_min {
            return;
        }
        let score = |lag: usize| -> f32 {
            let bpm = 60.0 * self.step_rate / lag as f32;
            // Prefer tempos near 120 when a pattern fits several equally.
            let octaves = (bpm / 120.0).log2();
            let prior = (-0.5 * (octaves / 0.9) * (octaves / 0.9)).exp();
            // A true beat also repeats at twice its length. Allow a step of
            // slack there, since the real length is rarely a whole step.
            let double = (lag * 2 - 1..=lag * 2 + 1).map(&autocorr).fold(f32::MIN, f32::max);
            (autocorr(lag) + 0.5 * double) * prior
        };
        let scores: Vec<f32> = (lag_min..=lag_max).map(score).collect();
        let (best_i, best) = scores
            .iter()
            .copied()
            .enumerate()
            .fold((0, f32::MIN), |acc, (i, s)| if s > acc.1 { (i, s) } else { acc });

        // A pattern that repeats every beat also repeats every two beats,
        // and the two score alike. When half the length fits nearly as
        // well, the shorter one is the beat.
        let mut best_i = best_i;
        let mut best = best;
        let half = (lag_min + best_i) / 2;
        if half >= lag_min {
            let around = half.saturating_sub(1).max(lag_min)..=(half + 1).min(lag_max);
            let (half_lag, half_ac) = around
                .map(|lag| (lag, autocorr(lag)))
                .fold((half, f32::MIN), |acc, c| if c.1 > acc.1 { c } else { acc });
            if half_ac > 0.8 * autocorr(lag_min + best_i) {
                best_i = half_lag - lag_min;
                best = scores[best_i];
            }
        }

        // Refine between whole steps with a parabola through the peak.
        let mut measured = (lag_min + best_i) as f32;
        if best_i > 0 && best_i + 1 < scores.len() {
            let (a, b, c) = (scores[best_i - 1], best, scores[best_i + 1]);
            let denom = a - 2.0 * b + c;
            if denom.abs() > 1e-12 {
                measured += (0.5 * (a - c) / denom).clamp(-0.5, 0.5);
            }
        }
        self.confidence = (best / energy).max(0.0);
        if self.confidence < TEMPO_MIN_CONFIDENCE {
            return;
        }

        let near = |a: f32, b: f32| b > 0.0 && (a - b).abs() / b < 0.06;
        if self.period <= 0.0 {
            self.period = measured;
            self.phase = self.measure_phase(&x);
        } else if near(measured, self.period) {
            self.period += (measured - self.period) * 0.3;
            self.candidate_votes = 0;
        } else {
            // A different tempo has to win several times in a row before
            // the clock jumps, so one odd bar does not derail it.
            if near(measured, self.candidate) {
                self.candidate_votes += 1;
            } else {
                self.candidate = measured;
                self.candidate_votes = 1;
            }
            if self.candidate_votes >= 4 {
                self.period = measured;
                self.candidate_votes = 0;
                self.phase = self.measure_phase(&x);
            }
            return;
        }

        let measured_phase = self.measure_phase(&x);
        let mut error = measured_phase - self.phase;
        if error > 0.5 {
            error -= 1.0;
        } else if error < -0.5 {
            error += 1.0;
        }
        self.phase = (self.phase + error * 0.25).rem_euclid(1.0);
    }

    /// Where in the beat we are now, 0..1, judged by lining a comb of
    /// evenly spaced teeth up with the recent onsets.
    fn measure_phase(&self, x: &[f32]) -> f32 {
        let len = x.len();
        let offsets = self.period.round().max(1.0) as usize;
        let mut best = (0usize, f32::MIN);
        for offset in 0..offsets {
            let mut sum = 0.0;
            let mut k = 0.0f32;
            loop {
                let back = offset as f32 + k * self.period;
                let i = len as isize - 1 - back.round() as isize;
                if i < 0 {
                    break;
                }
                // Allow each tooth one step of slack either side.
                let i = i as usize;
                let lo = i.saturating_sub(1);
                let hi = (i + 1).min(len - 1);
                sum += x[lo..=hi].iter().copied().fold(0.0f32, f32::max);
                k += 1.0;
            }
            if sum > best.1 {
                best = (offset, sum);
            }
        }
        (best.0 as f32 / self.period).rem_euclid(1.0)
    }
}

/// Moves a gain reference toward a new peak quickly and lets it fall slowly.
fn follow(reference: f32, value: f32, release: f32) -> f32 {
    let next = if value > reference {
        reference + (value - reference) * 0.3
    } else {
        reference * release
    };
    next.max(MIN_REFERENCE)
}

/// Maps a ratio to 0..1 on a decibel scale: 1.0 at the reference, 0.0 at
/// `range_db` below it.
fn to_unit(ratio: f32, range_db: f32) -> f32 {
    if ratio <= 0.0 {
        return 0.0;
    }
    (1.0 + 20.0 * ratio.log10() / range_db).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn sine(hz: f32, amplitude: f32, seconds: f32, sr: u32) -> Vec<f32> {
        (0..(seconds * sr as f32) as usize)
            .map(|i| amplitude * (std::f32::consts::TAU * hz * i as f32 / sr as f32).sin())
            .collect()
    }

    /// A kick drum on every beat with a quieter hi-hat tick in between.
    fn drum_loop(bpm: f32, seconds: f32, sr: u32, amplitude: f32) -> Vec<f32> {
        let n = (seconds * sr as f32) as usize;
        let beat = 60.0 / bpm * sr as f32;
        let mut out = vec![0.0f32; n];
        let mut noise = 0x1234_5678u32;
        for (i, sample) in out.iter_mut().enumerate() {
            let since_kick = (i as f32 % beat) / sr as f32;
            let kick = (std::f32::consts::TAU * 60.0 * since_kick).sin() * (-since_kick * 18.0).exp();
            let since_hat = ((i as f32 + beat / 2.0) % beat) / sr as f32;
            noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let white = (noise >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            let hat = white * (-since_hat * 90.0).exp() * 0.25;
            *sample = amplitude * (kick + hat);
        }
        out
    }

    fn feed(analyzer: &mut Analyzer, samples: &[f32]) {
        // Uneven chunks, like a real audio callback delivers.
        for chunk in samples.chunks(441) {
            analyzer.push(chunk);
        }
    }

    #[test]
    fn silence_is_inactive_and_dark() {
        let mut a = Analyzer::new(SR);
        feed(&mut a, &vec![0.0; SR as usize]);
        let f = a.features();
        assert!(!f.active);
        assert_eq!(f.level, 0.0);
        assert!(f.bands.iter().all(|b| *b == 0.0));
        assert_eq!(f.beat_count, 0);
    }

    #[test]
    fn a_tone_lights_its_own_band_and_nothing_far_away() {
        let mut a = Analyzer::new(SR);
        feed(&mut a, &sine(1000.0, 0.1, 1.0, SR));
        let f = a.features();
        assert!(f.active);
        assert!(f.level > 0.8, "level {}", f.level);
        let loudest = (0..BANDS).max_by(|x, y| f.bands[*x].total_cmp(&f.bands[*y])).unwrap();
        let (lo, hi) = a.band_bins[loudest];
        let hz_per_bin = SR as f32 / a.n as f32;
        assert!(lo as f32 * hz_per_bin <= 1000.0 && 1000.0 <= hi as f32 * hz_per_bin);
        assert!(f.bands[loudest] > 0.9);
        assert!(f.bands[0] < 0.05 && f.bands[BANDS - 1] < 0.05, "{:?}", f.bands);
        assert!(f.mid > 0.5 && f.bass < 0.05 && f.treble < 0.05);
    }

    #[test]
    fn bass_and_treble_tones_land_in_their_groups() {
        let mut a = Analyzer::new(SR);
        feed(&mut a, &sine(80.0, 0.2, 1.0, SR));
        assert!(a.features().bass > 0.6 && a.features().treble < 0.05);
        let mut a = Analyzer::new(SR);
        feed(&mut a, &sine(8000.0, 0.2, 1.0, SR));
        assert!(a.features().treble > 0.5 && a.features().bass < 0.05);
    }

    #[test]
    fn quiet_and_loud_sources_look_the_same() {
        let run = |amplitude: f32| {
            let mut a = Analyzer::new(SR);
            feed(&mut a, &sine(440.0, amplitude, 2.0, SR));
            a.features()
        };
        let (quiet, loud) = (run(0.01), run(0.8));
        assert!((quiet.level - loud.level).abs() < 0.05, "{} vs {}", quiet.level, loud.level);
        assert!((quiet.mid - loud.mid).abs() < 0.05);
    }

    #[test]
    fn hiss_far_below_the_floor_is_not_amplified() {
        let mut a = Analyzer::new(SR);
        feed(&mut a, &sine(1000.0, 0.0002, 1.0, SR));
        assert!(a.features().level < 0.2, "level {}", a.features().level);
    }

    #[test]
    fn features_fade_out_when_the_sound_stops() {
        let mut a = Analyzer::new(SR);
        feed(&mut a, &sine(1000.0, 0.2, 1.0, SR));
        assert!(a.features().level > 0.8);
        feed(&mut a, &vec![0.0; SR as usize]);
        let f = a.features();
        assert!(!f.active);
        assert!(f.level < 0.01 && f.mid < 0.01);
    }

    #[test]
    fn idle_fades_out_without_any_samples() {
        let mut a = Analyzer::new(SR);
        feed(&mut a, &sine(1000.0, 0.2, 1.0, SR));
        for _ in 0..20 {
            a.idle(0.05);
        }
        assert!(!a.features().active);
        assert!(a.features().level < 0.01);
    }

    #[test]
    fn an_onset_is_reported_within_40_ms() {
        let mut a = Analyzer::new(SR);
        feed(&mut a, &sine(200.0, 0.001, 1.0, SR));
        let burst = sine(200.0, 0.5, 0.2, SR);
        let mut heard_after = None;
        for (i, chunk) in burst.chunks(48).enumerate() {
            a.push(chunk);
            if a.features().onset > 0.9 {
                heard_after = Some((i + 1) as f32);
                break;
            }
        }
        let ms = heard_after.expect("no onset reported");
        assert!(ms <= 40.0, "onset took {ms} ms");
    }

    fn tempo_after(bpm: f32, sr: u32, amplitude: f32) -> AudioFeatures {
        let mut a = Analyzer::new(sr);
        feed(&mut a, &drum_loop(bpm, 12.0, sr, amplitude));
        a.features()
    }

    #[test]
    fn finds_the_tempo_of_a_drum_loop() {
        for bpm in [90.0, 120.0, 128.0, 140.0, 174.0] {
            let f = tempo_after(bpm, SR, 0.5);
            assert!(
                (f.bpm - bpm).abs() < bpm * 0.02,
                "expected {bpm} BPM, measured {}",
                f.bpm
            );
        }
    }

    #[test]
    fn tempo_does_not_depend_on_sample_rate_or_volume() {
        for sr in [44_100, 96_000] {
            let f = tempo_after(120.0, sr, 0.5);
            assert!((f.bpm - 120.0).abs() < 2.4, "{sr} Hz gave {}", f.bpm);
        }
        let f = tempo_after(120.0, SR, 0.01);
        assert!((f.bpm - 120.0).abs() < 2.4, "quiet loop gave {}", f.bpm);
    }

    #[test]
    fn beats_land_on_the_kicks() {
        let bpm = 120.0;
        let mut a = Analyzer::new(SR);
        let samples = drum_loop(bpm, 20.0, SR, 0.5);
        let beat_s = 60.0 / bpm;
        let mut last_count = 0;
        let mut errors = Vec::new();
        let mut counted = 0;
        for (i, chunk) in samples.chunks(480).enumerate() {
            a.push(chunk);
            let f = a.features();
            let now = (i + 1) as f32 * 480.0 / SR as f32;
            if f.beat_count != last_count {
                last_count = f.beat_count;
                // Judge only once the tracker has had time to lock.
                if now > 10.0 {
                    counted += 1;
                    let off = (now + beat_s / 2.0).rem_euclid(beat_s) - beat_s / 2.0;
                    errors.push(off);
                }
            }
        }
        // 10 seconds at 120 BPM is 20 beats.
        assert!((19..=21).contains(&counted), "counted {counted} beats in 10 s");
        let worst = errors.iter().fold(0.0f32, |m, e| m.max(e.abs()));
        assert!(worst < 0.07, "a beat was {:.0} ms away from its kick", worst * 1000.0);
    }

    #[test]
    fn hits_stand_in_for_beats_when_there_is_no_tempo() {
        // Three isolated hits at irregular times.
        let mut samples = vec![0.0f32; SR as usize * 4];
        for at in [0.5f32, 1.3, 3.1] {
            let start = (at * SR as f32) as usize;
            for (i, s) in sine(100.0, 0.5, 0.1, SR).iter().enumerate() {
                samples[start + i] = *s;
            }
        }
        let mut a = Analyzer::new(SR);
        feed(&mut a, &samples);
        assert_eq!(a.features().beat_count, 3);
        assert_eq!(a.features().bpm, 0.0);
    }

    #[test]
    fn sensitivity_scales_the_bands() {
        let mut a = Analyzer::new(SR);
        a.set_sensitivity(0.25);
        feed(&mut a, &sine(1000.0, 0.1, 1.0, SR));
        let low = a.features().mid;
        let mut b = Analyzer::new(SR);
        feed(&mut b, &sine(1000.0, 0.1, 1.0, SR));
        assert!(low < b.features().mid * 0.5);
    }
}
