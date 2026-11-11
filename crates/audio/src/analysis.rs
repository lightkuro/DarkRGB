//! Turns audio samples into the features effects react to.
//!
//! This is plain signal processing with no audio device involved, so it can
//! be tested with synthetic signals and reused later to analyse song files.
//!
//! Per step (about every 11 ms):
//!
//! 1. A windowed FFT gives the spectrum, which is summed into mel-spaced bands.
//! 2. Automatic gain makes quiet and loud sources produce the same picture.
//!    Each band is also levelled against its own recent peaks, because
//!    treble carries far less energy than bass and would otherwise never
//!    show next to a kick drum.
//! 3. The rise in band energy since the last step ("spectral flux") marks
//!    onsets: the starts of notes and drum hits.
//! 4. The rhythm of those onsets over the last few seconds gives the tempo,
//!    and a beat clock is kept in step with it.

use std::collections::VecDeque;
use std::sync::Arc;

use darkrgb_core::audio::{BAND_MAX_HZ, band_edge_hz, group_of_band};
use darkrgb_core::{AudioFeatures, BANDS};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// A band this far below its own recent peak shows as zero, at most.
const BAND_RANGE_DB: f32 = 24.0;
/// How close to the peak the "quiet" end of a band's range may rise, when
/// the band never goes quiet. Music with a steady bass line under its kick
/// drum then still swings: the steady part reads low and the kick high.
const MIN_RANGE_DB: f32 = 9.0;
/// Seconds for the quiet end of the range to drift up to a sustained level.
const FLOOR_RISE_S: f32 = 3.0;
/// A group quieter than this, relative to the loudest group (-22 dB), is
/// treated as spill when looking for hits in it.
const GROUP_SPILL: f32 = 0.08;
/// A band is never levelled up by more than this relative to the loudest
/// band (30 dB), so bands holding only noise stay dark.
const BAND_FLOOR: f32 = 0.03;
/// Input quieter than this (about -80 dBFS) counts as silence.
const SILENCE_RMS: f32 = 1e-4;
/// Gain never amplifies beyond what this reference allows, so background
/// hiss is not blown up into a full-scale display.
const MIN_REFERENCE: f32 = 3e-3;

/// The song's loudness is smoothed over about this long before parts of
/// it are compared, so single hits do not count as a change of section.
const SECTION_SMOOTH_S: f32 = 1.5;
/// How long the loudest and quietest parts are remembered.
const SECTION_MEMORY_S: f32 = 45.0;
/// Parts of a song differ by at least this much before the quieter one
/// reads as "low". A steadily loud song therefore reads as high throughout.
const SECTION_RANGE_DB: f32 = 8.0;
/// A change of this much of the range, lasting, is a new section.
const SECTION_JUMP: f32 = 0.3;
/// Sections do not change more often than this.
const SECTION_GAP_S: f32 = 4.0;
/// Getting this much louder than anything in recent memory, within a few
/// seconds, is a new section too. It is how the first drop after a quiet
/// intro is told: there is no louder part yet to measure the intro by.
const SECTION_SURGE_DB: f32 = 4.0;

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
    /// Recent peak of the loudest band, and of each band by itself.
    band_ref: f32,
    band_peaks: [f32; BANDS],
    /// Recent quiet level of each band: falls at once, rises slowly.
    band_floors: [f32; BANDS],
    /// The same pair for bass, mid and treble as whole groups.
    group_peaks: [f32; 3],
    group_floors: [f32; 3],
    prev_group_log: [f32; 3],
    prev_group_rise: [f32; 3],
    level_ref: f32,
    level_floor: f32,
    prev_log: [f32; BANDS],
    /// As `prev_log`, but measured against the loudest band, not each
    /// band's own peak. The tempo tracker uses this one.
    prev_log_natural: [f32; BANDS],
    quiet_steps: usize,

    /// Finds hits in the whole spectrum.
    onsets: OnsetDetector,
    /// Finds hits in the bass, the mids and the treble separately, so a
    /// layer that listens only to highs can flash on hi-hats alone.
    group_onsets: [OnsetDetector; 3],
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

    /// How big the current part of the song is: overall, and in the bass.
    sections: [SectionMeter; 2],
    /// The intensity the song last settled at, to tell a change from it.
    settled: f32,
    /// How long the intensity has been away from `settled`, in steps.
    away_steps: usize,
    steps_since_section: usize,

    out: AudioFeatures,
}

/// Follows one loudness over seconds and places it within the range it
/// has moved over lately.
#[derive(Clone, Copy, Debug, Default)]
struct SectionMeter {
    /// Average power over the last second or two. Power, not decibels:
    /// the gaps between drum hits must count as little, not as silence.
    power: f32,
    steps: u32,
    low: f32,
    high: f32,
    /// Where `high` was a few seconds ago.
    high_before: f32,
}

impl SectionMeter {
    /// Takes the loudness of one step and returns its place, 0..1.
    fn step(&mut self, value: f32, step_rate: f32) -> f32 {
        let power = value * value;
        let span = step_rate * SECTION_SMOOTH_S;
        self.steps = self.steps.saturating_add(1);
        if (self.steps as f32) <= span {
            // Settling in: a plain average, and nothing to compare with yet.
            self.power += (power - self.power) / self.steps as f32;
            let db = 10.0 * self.power.max(1e-12).log10();
            (self.low, self.high, self.high_before) = (db, db, db);
            return 1.0;
        }
        self.power += (power - self.power) / span;
        let now = 10.0 * self.power.max(1e-12).log10();
        // The remembered extremes give way slowly, so a long quiet part
        // does not come to count as normal within a few bars.
        let forget = 1.0 / (step_rate * SECTION_MEMORY_S);
        self.high = if now > self.high {
            now
        } else {
            self.high + (now - self.high) * forget
        };
        self.low = if now < self.low {
            now
        } else {
            self.low + (now - self.low) * forget
        };
        self.high_before += (self.high - self.high_before) / (step_rate * 3.0);
        let range = (self.high - self.low).max(SECTION_RANGE_DB);
        ((now - (self.high - range)) / range).clamp(0.0, 1.0)
    }

    /// How far, in decibels, the loudest level has just been pushed up.
    fn surge(&self) -> f32 {
        self.high - self.high_before
    }
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
        let edge_hz = |i: usize| band_edge_hz(i, top);
        let hz_per_bin = sample_rate / n as f32;
        let mut band_bins = [(0usize, 0usize); BANDS];
        let mut band_class = [1u8; BANDS];
        for b in 0..BANDS {
            let lo = ((edge_hz(b) / hz_per_bin).round() as usize).clamp(1, n / 2 - 1);
            let hi = ((edge_hz(b + 1) / hz_per_bin).round() as usize).clamp(lo + 1, n / 2);
            band_bins[b] = (lo, hi);
            band_class[b] = group_of_band(b) as u8;
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
            band_peaks: [0.0; BANDS],
            band_floors: [0.0; BANDS],
            group_peaks: [0.0; 3],
            group_floors: [0.0; 3],
            prev_group_log: [0.0; 3],
            prev_group_rise: [0.0; 3],
            level_ref: MIN_REFERENCE,
            level_floor: 0.0,
            prev_log: [0.0; BANDS],
            prev_log_natural: [0.0; BANDS],
            quiet_steps: usize::MAX / 2,
            onsets: OnsetDetector::new(0.5),
            group_onsets: [0, 1, 2].map(|_| OnsetDetector::new(0.4)),
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
            sections: [SectionMeter::default(); 2],
            settled: 1.0,
            away_steps: 0,
            steps_since_section: usize::MAX / 2,
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
        let floor = self.band_ref * BAND_FLOOR;
        for b in 0..BANDS {
            let peak = &mut self.band_peaks[b];
            *peak = if bands[b] > *peak {
                *peak + (bands[b] - *peak) * 0.3
            } else {
                *peak * release
            };
            let relative = bands[b] / peak.max(floor);
            let log = (1.0 + 50.0 * relative).ln();
            let rise = (log - self.prev_log[b]).max(0.0);
            self.prev_log[b] = log;
            flux += rise;

            // For rhythm, keep the natural balance: a kick should outweigh
            // a hi-hat, which levelling each band would undo.
            let natural = (1.0 + 50.0 * bands[b] / self.band_ref).ln();
            let natural_rise = (natural - self.prev_log_natural[b]).max(0.0);
            self.prev_log_natural[b] = natural;
            group_flux[self.band_class[b] as usize] += natural_rise;
            group_size[self.band_class[b] as usize] += 1;

            let reference = peak.max(floor);
            let quiet = follow_floor(&mut self.band_floors[b], bands[b], self.step_rate, reference);
            let target = if active {
                (within(bands[b], quiet, reference).powf(1.2) * self.sensitivity).min(1.0)
            } else {
                0.0
            };
            let current = &mut self.out.bands[b];
            let rate = if target > *current { 0.7 } else { 0.18 };
            *current += (target - *current) * rate;
        }

        let quiet = follow_floor(&mut self.level_floor, rms, self.step_rate, self.level_ref);
        let target = if active {
            (within(rms, quiet, self.level_ref).powf(1.2) * self.sensitivity).min(1.0)
        } else {
            0.0
        };
        let rate = if target > self.out.level { 0.6 } else { 0.12 };
        self.out.level += (target - self.out.level) * rate;

        // Bass, mid and treble come from each group's total energy, levelled
        // the same way as a single band. Taking the loudest band of a group
        // would sit near the top all the time: some band always is.
        let mut energy = [0.0f32; 3];
        for b in 0..BANDS {
            energy[self.band_class[b] as usize] += bands[b] * bands[b];
        }
        let mut groups = [0.0f32; 3];
        // How far each group's energy rose this step, for hits in that group.
        let mut group_rise = [0.0f32; 3];
        // A group whose energy is far below the loudest group's is only
        // hearing spill from it (a hi-hat's trace of bass, a kick's click).
        let spill = self.group_peaks.iter().copied().fold(0.0f32, f32::max) * GROUP_SPILL;
        for g in 0..3 {
            let value = energy[g].sqrt();
            let peak = &mut self.group_peaks[g];
            *peak = if value > *peak {
                *peak + (value - *peak) * 0.3
            } else {
                *peak * release
            };
            let reference = peak.max(floor);
            // Hits are measured against the group's own recent peak, so
            // quiet hi-hats under a loud kick still count, but never
            // against less than the spill level. Squaring makes anything
            // well under that count for almost nothing.
            let relative = value / peak.max(spill).max(1e-9);
            let log = (1.0 + 10.0 * relative * relative).ln();
            group_rise[g] = (log - self.prev_group_log[g]).max(0.0);
            self.prev_group_log[g] = log;
            let quiet = follow_floor(&mut self.group_floors[g], value, self.step_rate, reference);
            let target = if active {
                (within(value, quiet, reference).powf(1.2) * self.sensitivity).min(1.0)
            } else {
                0.0
            };
            let current = [&mut self.out.bass, &mut self.out.mid, &mut self.out.treble];
            let rate = if target > *current[g] { 0.7 } else { 0.18 };
            groups[g] = *current[g] + (target - *current[g]) * rate;
        }
        [self.out.bass, self.out.mid, self.out.treble] = groups;
        self.out.active = active;
        self.track_sections(rms, energy[0].sqrt(), active);

        let onset = self.onsets.step(flux, self.step_rate) && active;
        self.step_index += 1;
        if onset {
            self.recent_onsets.push_back(self.step_index);
        }
        let horizon = (self.step_rate * TEMPO_WINDOW_S) as u64;
        while self
            .recent_onsets
            .front()
            .is_some_and(|at| self.step_index - at > horizon)
        {
            self.recent_onsets.pop_front();
        }
        let fade = (-1.0 / (self.step_rate * 0.10)).exp();
        self.out.onset = if onset { 1.0 } else { self.out.onset * fade };
        // The index reaches into four arrays at once; a plain range reads best.
        #[allow(clippy::needless_range_loop)]
        for g in 0..3 {
            // A hit takes a few steps to climb through the analysis window,
            // so judge the rise over two steps, not one.
            let rise = group_rise[g] + self.prev_group_rise[g];
            self.prev_group_rise[g] = group_rise[g];
            let hit = self.group_onsets[g].step(rise, self.step_rate) && active;
            self.out.onsets[g] = if hit { 1.0 } else { self.out.onsets[g] * fade };
        }

        // Beats are carried mostly by kick and bass, while hi-hats tick on
        // the off-beats too. Weighting the groups keeps a busy hi-hat from
        // being mistaken for the beat.
        let per_band = |g: usize| group_flux[g] / group_size[g].max(1) as f32;
        let rhythm = 2.0 * per_band(0) + per_band(1) + 0.4 * per_band(2);
        self.track_beat(if active { rhythm } else { 0.0 }, onset, fade);
    }

    /// Works out how big the current part of the song is, and notices
    /// when the song moves to a clearly bigger or smaller part.
    fn track_sections(&mut self, rms: f32, bass: f32, active: bool) {
        let fade = (-1.0 / (self.step_rate * 1.5)).exp();
        self.out.drop *= fade;
        self.steps_since_section = self.steps_since_section.saturating_add(1);
        if !active {
            // Silence is not a quiet section of a song; start afresh after it.
            self.sections = [SectionMeter::default(); 2];
            self.out.intensity = 1.0;
            self.settled = 1.0;
            self.away_steps = 0;
            return;
        }
        // Half overall loudness, half bass: a breakdown often keeps its
        // volume and loses its kick.
        let overall = self.sections[0].step(rms, self.step_rate);
        let low_end = self.sections[1].step(bass, self.step_rate);
        let intensity = 0.5 * overall + 0.5 * low_end;
        self.out.intensity = intensity;

        // For a few seconds after a change the song is still arriving at
        // its new level; the rest of the way there is the same change.
        if (self.steps_since_section as f32) < self.step_rate * SECTION_GAP_S {
            self.settled = intensity;
            self.away_steps = 0;
            return;
        }
        // Louder than anything remembered: a new section even though the
        // intensity was, and stays, at the top.
        if self.sections.iter().any(|m| m.surge() > SECTION_SURGE_DB) {
            self.out.section = self.out.section.wrapping_add(1);
            self.out.drop = 1.0;
            self.settled = intensity;
            self.away_steps = 0;
            self.steps_since_section = 0;
            return;
        }
        let change = intensity - self.settled;
        if change.abs() < SECTION_JUMP {
            self.away_steps = 0;
            // Drift is followed; only a jump is a new section.
            self.settled += change / (self.step_rate * 8.0);
            return;
        }
        self.away_steps += 1;
        if self.away_steps as f32 > self.step_rate * 0.6 {
            self.out.section = self.out.section.wrapping_add(1);
            if change > 0.0 {
                self.out.drop = 1.0;
            }
            self.settled = intensity;
            self.away_steps = 0;
            self.steps_since_section = 0;
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
        let locked = self.out.active && enough_hits && self.period > 0.0 && self.confidence >= TEMPO_MIN_CONFIDENCE;
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

/// Spots the moments when the rise in energy stands out from the last
/// second and a half, with a short rest after each one.
struct OnsetDetector {
    history: VecDeque<f32>,
    steps_since: usize,
    /// Smallest rise that can count, so near-silence does not trigger.
    floor: f32,
}

impl OnsetDetector {
    fn new(floor: f32) -> Self {
        Self {
            history: VecDeque::new(),
            steps_since: usize::MAX / 2,
            floor,
        }
    }

    fn step(&mut self, flux: f32, step_rate: f32) -> bool {
        let keep = (step_rate * 1.5) as usize;
        let n = self.history.len().max(1) as f32;
        let mean = self.history.iter().sum::<f32>() / n;
        let var = self.history.iter().map(|f| (f - mean) * (f - mean)).sum::<f32>() / n;
        self.history.push_back(flux);
        while self.history.len() > keep {
            self.history.pop_front();
        }
        self.steps_since = self.steps_since.saturating_add(1);
        let threshold = mean + 2.0 * var.sqrt() + self.floor;
        let rested = self.steps_since as f32 > step_rate * 0.06;
        if flux > threshold && rested {
            self.steps_since = 0;
            true
        } else {
            false
        }
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

fn db_to_ratio(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Updates a band's quiet level and returns the bottom of its range: the
/// quiet level, kept between `BAND_RANGE_DB` and `MIN_RANGE_DB` below the
/// peak. A steady tone therefore reads as full, not as nothing.
fn follow_floor(floor: &mut f32, value: f32, step_rate: f32, peak: f32) -> f32 {
    let rise = 1.0 - (-1.0 / (step_rate * FLOOR_RISE_S)).exp();
    *floor += (value - *floor) * if value < *floor { 0.2 } else { rise };
    floor.clamp(peak * db_to_ratio(-BAND_RANGE_DB), peak * db_to_ratio(-MIN_RANGE_DB))
}

/// Where `value` sits between `low` and `high` on a decibel scale, 0..1.
fn within(value: f32, low: f32, high: f32) -> f32 {
    if value <= low || high <= low {
        return 0.0;
    }
    ((value / low).log10() / (high / low).log10()).clamp(0.0, 1.0)
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
        assert!(
            (quiet.level - loud.level).abs() < 0.05,
            "{} vs {}",
            quiet.level,
            loud.level
        );
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

    /// Counts how often each group's onset fires across a signal.
    fn count_hits(samples: &[f32]) -> [u32; 3] {
        let mut a = Analyzer::new(SR);
        let mut hits = [0u32; 3];
        let mut armed = [true; 3];
        for chunk in samples.chunks(256) {
            a.push(chunk);
            let f = a.features();
            for g in 0..3 {
                if armed[g] && f.onsets[g] > 0.9 {
                    hits[g] += 1;
                    armed[g] = false;
                } else if f.onsets[g] < 0.5 {
                    armed[g] = true;
                }
            }
        }
        hits
    }

    #[test]
    fn kicks_are_bass_hits_and_hi_hats_are_treble_hits() {
        let seconds = 6.0;
        let n = (seconds * SR as f32) as usize;
        let beat = SR as usize / 2;
        // Kicks only: a low thump twice a second.
        let kicks: Vec<f32> = (0..n)
            .map(|i| {
                let t = (i % beat) as f32 / SR as f32;
                0.6 * (std::f32::consts::TAU * 60.0 * t).sin() * (-t * 18.0).exp()
            })
            .collect();
        let [bass, _, treble] = count_hits(&kicks);
        assert!(
            (10..=13).contains(&bass),
            "12 kicks should give about 12 bass hits, got {bass}"
        );
        assert!(treble <= 1, "kicks must not count as treble hits, got {treble}");
        // A much quieter source is heard the same way.
        let quiet: Vec<f32> = kicks.iter().map(|s| s * 0.02).collect();
        let [bass, _, treble] = count_hits(&quiet);
        assert!(
            (10..=13).contains(&bass),
            "quiet kicks should still be found, got {bass}"
        );
        assert!(treble <= 1, "quiet kicks must not count as treble hits, got {treble}");

        // Hi-hats only: short bursts of high-passed noise, twice a second.
        let mut noise = 0x1234_5678u32;
        let mut previous = 0.0f32;
        let hats: Vec<f32> = (0..n)
            .map(|i| {
                let t = (i % beat) as f32 / SR as f32;
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let white = (noise >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
                // A first difference removes the low end.
                let high = white - previous;
                previous = white;
                0.3 * high * (-t * 90.0).exp()
            })
            .collect();
        let [bass, _, treble] = count_hits(&hats);
        assert!(
            (10..=13).contains(&treble),
            "12 hats should give about 12 treble hits, got {treble}"
        );
        assert!(bass <= 1, "hi-hats must not count as bass hits, got {bass}");
    }

    /// A drum loop whose volume follows `parts`: seconds and loudness of each.
    fn song(parts: &[(f32, f32)], scale: f32) -> Vec<f32> {
        parts
            .iter()
            .flat_map(|(seconds, amplitude)| drum_loop(124.0, *seconds, SR, amplitude * scale))
            .collect()
    }

    /// Feeds a song and returns intensity, section count and drop once a second.
    fn follow_song(samples: &[f32]) -> Vec<(f32, u32, f32)> {
        let mut a = Analyzer::new(SR);
        let mut out = Vec::new();
        for second in samples.chunks(SR as usize) {
            // The strongest "drop" seen during the second, since it fades.
            let mut drop = 0.0f32;
            for block in second.chunks(480) {
                a.push(block);
                drop = drop.max(a.features().drop);
            }
            let f = a.features();
            out.push((f.intensity, f.section, drop));
        }
        out
    }

    #[test]
    fn a_steady_song_is_all_one_section_at_full_intensity() {
        let trace = follow_song(&song(&[(30.0, 0.5)], 1.0));
        assert!(trace.iter().all(|(_, section, _)| *section == 0), "{trace:?}");
        assert!(trace[5..].iter().all(|(i, _, _)| *i > 0.85), "{trace:?}");
        assert!(trace.iter().all(|(_, _, drop)| *drop == 0.0));
    }

    #[test]
    fn a_breakdown_and_a_drop_are_two_changes_of_section() {
        // Loud, then a quiet part 20 dB down, then loud again.
        let parts = [(14.0, 0.5), (14.0, 0.05), (14.0, 0.5)];
        let trace = follow_song(&song(&parts, 1.0));
        let (loud, quiet, back) = (trace[12], trace[26], trace[40]);
        assert!(loud.0 > 0.85, "first part {loud:?}");
        assert!(quiet.0 < 0.35, "the breakdown {quiet:?}");
        assert!(back.0 > 0.85, "after the drop {back:?}");
        assert_eq!((loud.1, quiet.1, back.1), (0, 1, 2), "{trace:?}");
        // The fall is not a drop; the return is, within three seconds of it.
        assert!(trace[14..27].iter().all(|(_, _, drop)| *drop == 0.0), "{trace:?}");
        assert!(trace[28..31].iter().any(|(_, _, drop)| *drop > 0.8), "{trace:?}");
        assert!(trace[40].2 < 0.01, "and it fades");

        // The same song fifty times quieter reads the same.
        let soft = follow_song(&song(&parts, 0.02));
        for (i, (a, b)) in trace.iter().zip(&soft).enumerate().skip(3) {
            assert!((a.0 - b.0).abs() < 0.08, "second {i}: {a:?} against {b:?}");
            assert_eq!(a.1, b.1, "second {i}");
        }
    }

    #[test]
    fn the_first_drop_after_a_quieter_intro_is_a_new_section() {
        // Nothing louder has been heard when the intro plays, so the intro
        // reads as full. The drop must still be noticed.
        let trace = follow_song(&song(&[(12.0, 0.12), (14.0, 0.5)], 1.0));
        assert_eq!(trace[10].1, 0, "{trace:?}");
        let found = trace
            .iter()
            .position(|(_, section, _)| *section == 1)
            .expect("the drop was missed");
        assert!((12..=14).contains(&found), "noticed in second {found}: {trace:?}");
        assert!(trace[found].2 > 0.5, "and it is a drop: {trace:?}");
        assert_eq!(trace.last().unwrap().1, 1, "counted once: {trace:?}");
    }

    #[test]
    fn small_changes_in_volume_are_not_new_sections() {
        // 3 dB up and down every few seconds: phrasing, not structure.
        let parts: Vec<(f32, f32)> = (0..10).map(|i| (4.0, if i % 2 == 0 { 0.5 } else { 0.35 })).collect();
        let trace = follow_song(&song(&parts, 1.0));
        assert!(trace.iter().all(|(_, section, _)| *section == 0), "{trace:?}");
        assert!(trace[5..].iter().all(|(i, _, _)| *i > 0.6), "{trace:?}");
    }

    #[test]
    fn silence_starts_the_song_afresh() {
        let mut samples = song(&[(10.0, 0.5)], 1.0);
        samples.extend(vec![0.0; 3 * SR as usize]);
        samples.extend(song(&[(8.0, 0.05)], 1.0));
        let trace = follow_song(&samples);
        assert_eq!(trace[11].0, 1.0, "nothing is held back in silence");
        // A quiet song after a pause is its own measure, not a breakdown of the last.
        assert!(trace[19].0 > 0.85, "{trace:?}");
        assert_eq!(trace.last().unwrap().1, 0);
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
