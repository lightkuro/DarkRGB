//! Performing live: what is being held down, a tempo tapped by hand, beats
//! fired by hand, and a scene waiting for the beat.
//!
//! None of this is saved. Nothing here does I/O or reads a clock; times
//! are seconds since the engine started, passed in, so all of it is unit
//! tested.

use std::collections::HashMap;

use darkrgb_core::{Action, AudioFeatures, KeyMode, Launch, Show};
use serde::Serialize;

/// A new run of taps starts after a pause this long.
const TAP_GAP_S: f64 = 2.0;
const TAP_MEMORY: usize = 8;
const MIN_BPM: f32 = 40.0;
const MAX_BPM: f32 = 240.0;
/// How quickly a beat's flash fades, matching the audio analysis.
const PULSE_FADE_S: f64 = 0.10;
/// A scene waiting for a beat stops waiting after this many seconds, in
/// case the music stopped.
const BEAT_WAIT_S: f64 = 2.5;
const BAR_WAIT_S: f64 = 8.0;

/// A tempo set by tapping. While one is set, it replaces the tempo found
/// in the music.
#[derive(Clone, Debug, Default)]
pub struct Tempo {
    taps: Vec<f64>,
    /// Zero while the music's own tempo is in charge.
    bpm: f32,
    /// A moment a beat fell on.
    anchor: f64,
    /// The beat number at `anchor`.
    count_at_anchor: u32,
    /// The beat number of the first tap of the current run.
    run_start: u32,
}

impl Tempo {
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// One tap. `count` is the beat number the lights are on right now.
    /// The first tap of a run starts a new bar; from the second tap on
    /// there is a tempo.
    pub fn tap(&mut self, now: f64, count: u32) {
        let new_run = self.taps.last().is_none_or(|last| now - last > TAP_GAP_S);
        if new_run {
            self.taps.clear();
            // The next multiple of four after the current beat.
            self.run_start = (count / 4 + 1).wrapping_mul(4);
        }
        self.taps.push(now);
        if self.taps.len() > TAP_MEMORY {
            self.taps.remove(0);
            self.run_start = self.run_start.wrapping_add(1);
        }
        self.anchor = now;
        self.count_at_anchor = self.run_start.wrapping_add(self.taps.len() as u32 - 1);
        if self.taps.len() >= 2 {
            let span = now - self.taps[0];
            let interval = span / (self.taps.len() - 1) as f64;
            self.bpm = ((60.0 / interval) as f32).clamp(MIN_BPM, MAX_BPM);
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Replaces the beat in `features` with the tapped one, if there is one.
    fn apply(&self, now: f64, features: &mut AudioFeatures) {
        if self.bpm <= 0.0 {
            return;
        }
        let beats = ((now - self.anchor) * self.bpm as f64 / 60.0).max(0.0);
        let since_beat = beats.fract() * 60.0 / self.bpm as f64;
        features.bpm = self.bpm;
        features.beat_phase = beats.fract() as f32;
        features.beat_count = self.count_at_anchor.wrapping_add(beats.floor() as u32);
        features.beat = (-since_beat / PULSE_FADE_S).exp() as f32;
    }
}

/// A scene that will start on a coming beat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pending {
    pub scene: u32,
    pub since: f64,
}

/// What clients are told about the live state.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LiveStatus {
    /// Keys being held down right now.
    pub held: Vec<u32>,
    /// The scene waiting for the beat, if any.
    pub pending_scene: Option<u32>,
    /// The tapped tempo, or 0 when the music's own tempo is used.
    pub tapped_bpm: f32,
}

#[derive(Debug, Default)]
pub struct Live {
    pub tempo: Tempo,
    /// Keys held down, with how many clients hold each.
    held: HashMap<u32, usize>,
    pub pending: Option<Pending>,
    /// When the last beat was fired by hand, and how many there have been.
    hit_at: Option<f64>,
    hits: u32,
    /// Counts scene changes, so the frame loop can tell one from an edit.
    pub scene_serial: u64,
}

impl Live {
    pub fn status(&self) -> LiveStatus {
        let mut held: Vec<u32> = self.held.keys().copied().collect();
        held.sort_unstable();
        LiveStatus {
            held,
            pending_scene: self.pending.map(|p| p.scene),
            tapped_bpm: self.tempo.bpm(),
        }
    }

    /// Notes a key going down or up. Returns whether what is held changed.
    pub fn hold(&mut self, binding: u32, down: bool) -> bool {
        if down {
            let holders = self.held.entry(binding).or_insert(0);
            *holders += 1;
            *holders == 1
        } else {
            match self.held.get_mut(&binding) {
                Some(holders) if *holders > 1 => {
                    *holders -= 1;
                    false
                }
                Some(_) => {
                    self.held.remove(&binding);
                    true
                }
                None => false,
            }
        }
    }

    /// Releases every held key and forgets a waiting scene. The tempo stays.
    pub fn let_go(&mut self) {
        self.held.clear();
        self.pending = None;
    }

    pub fn is_held(&self, binding: u32) -> bool {
        self.held.contains_key(&binding)
    }

    /// Fires a beat by hand.
    pub fn hit(&mut self, now: f64) {
        self.hit_at = Some(now);
        self.hits = self.hits.wrapping_add(1);
    }

    /// One tap of the tempo. `raw` is what the audio analysis says now.
    pub fn tap(&mut self, now: f64, raw: AudioFeatures) {
        let count = self.clocked(now, raw).beat_count;
        self.tempo.tap(now, count);
    }

    /// The audio features with the tapped tempo in place of the music's.
    /// Bars are counted on this, before beats fired by hand are added.
    pub fn clocked(&self, now: f64, mut features: AudioFeatures) -> AudioFeatures {
        self.tempo.apply(now, &mut features);
        features
    }

    /// The audio features as the lights should see them: with the tapped
    /// tempo in place of the music's, and beats fired by hand added.
    pub fn features(&self, now: f64, features: AudioFeatures) -> AudioFeatures {
        self.with_hits(now, self.clocked(now, features))
    }

    /// Adds beats fired by hand to features that already carry the tempo.
    pub fn with_hits(&self, now: f64, mut features: AudioFeatures) -> AudioFeatures {
        if let Some(at) = self.hit_at {
            let pulse = (-(now - at).max(0.0) / PULSE_FADE_S).exp() as f32;
            features.beat = features.beat.max(pulse);
            features.onset = features.onset.max(pulse);
            for onset in &mut features.onsets {
                *onset = onset.max(pulse);
            }
        }
        features.beat_count = features.beat_count.wrapping_add(self.hits);
        features
    }

    /// The show as the lights should see it: every held key flips what it
    /// is tied to, for as long as it is down.
    pub fn overlay(&self, show: &Show) -> Show {
        let mut out = show.clone();
        for binding in show.bindings.iter().filter(|b| b.mode == KeyMode::Hold) {
            if !self.is_held(binding.id) {
                continue;
            }
            match binding.action {
                Action::Layer { id } => {
                    if let Some(layer) = out.layer_mut(id) {
                        layer.enabled = !layer.enabled;
                    }
                }
                Action::LayerGroup { id } => {
                    if let Some(folder) = out.layer_groups.iter_mut().find(|g| g.id == id) {
                        folder.enabled = !folder.enabled;
                    }
                }
                Action::LightGroup { id } => {
                    if let Some(group) = out.light_groups.iter_mut().find(|g| g.id == id) {
                        group.enabled = !group.enabled;
                    }
                }
                Action::Blackout => out.master.blackout = !out.master.blackout,
                Action::Scene { .. } | Action::TapTempo | Action::Hit => {}
            }
        }
        out
    }

    /// Whether a waiting scene should start on this frame. `beat` is true
    /// when a new beat began since the frame before, which was at
    /// `last_frame`.
    pub fn should_launch(
        &self,
        now: f64,
        last_frame: f64,
        launch: Launch,
        features: &AudioFeatures,
        beat: bool,
    ) -> bool {
        let Some(pending) = self.pending else {
            return false;
        };
        let waited = now - pending.since;
        // A beat that began before the scene was asked for is not the one
        // to start on, even if this is the first frame to notice it.
        let beat = beat && pending.since <= last_frame;
        match launch {
            Launch::Now => true,
            // Without a tempo there is no beat to wait for.
            _ if features.bpm <= 0.0 => true,
            Launch::Beat => beat || waited > BEAT_WAIT_S,
            Launch::Bar => (beat && features.beat_count.is_multiple_of(4)) || waited > BAR_WAIT_S,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tapped(times: &[f64]) -> Tempo {
        let mut tempo = Tempo::default();
        for t in times {
            tempo.tap(*t, 0);
        }
        tempo
    }

    #[test]
    fn two_taps_make_a_tempo_and_more_taps_refine_it() {
        let mut tempo = Tempo::default();
        tempo.tap(10.0, 0);
        assert_eq!(tempo.bpm(), 0.0, "one tap is not a tempo");
        tempo.tap(10.5, 0);
        assert!((tempo.bpm() - 120.0).abs() < 0.01);
        tempo.tap(11.02, 0);
        tempo.tap(11.5, 0);
        assert!((tempo.bpm() - 120.0).abs() < 0.5, "{}", tempo.bpm());
    }

    #[test]
    fn a_pause_starts_a_new_run_and_keeps_the_old_tempo_until_the_second_tap() {
        let mut tempo = tapped(&[0.0, 0.5, 1.0]);
        tempo.tap(10.0, 0);
        assert!((tempo.bpm() - 120.0).abs() < 0.01);
        tempo.tap(10.4, 0);
        assert!((tempo.bpm() - 150.0).abs() < 0.01);
    }

    #[test]
    fn absurd_tapping_is_kept_within_sane_tempos() {
        assert_eq!(tapped(&[0.0, 0.01]).bpm(), MAX_BPM);
        assert_eq!(tapped(&[0.0, 1.9]).bpm(), MIN_BPM);
    }

    #[test]
    fn a_tapped_tempo_drives_the_beat() {
        let live = Live {
            tempo: tapped(&[1.0, 1.5]),
            ..Live::default()
        };
        let at = |now: f64| live.features(now, AudioFeatures::default());
        let on_tap = at(1.5);
        assert_eq!(on_tap.bpm, 120.0);
        assert!(on_tap.beat > 0.99 && on_tap.beat_phase < 0.01);
        let half = at(1.75);
        assert!((half.beat_phase - 0.5).abs() < 1e-3);
        assert!(half.beat < 0.1, "the flash has faded by mid-beat");
        assert_eq!(at(1.99).beat_count, on_tap.beat_count);
        assert_eq!(at(2.01).beat_count, on_tap.beat_count + 1);
        assert_eq!(at(3.51).beat_count, on_tap.beat_count + 4);
    }

    #[test]
    fn the_first_tap_starts_a_bar_and_steady_taps_count_up_without_doubling() {
        let mut live = Live::default();
        // A beat fired by hand earlier must not shift where bars fall.
        live.hit(99.0);
        let mut now = 100.0;
        let mut counts = Vec::new();
        for _ in 0..6 {
            live.tap(now, AudioFeatures::default());
            counts.push(live.clocked(now, AudioFeatures::default()).beat_count);
            // Just before the next tap the clock has not run ahead.
            let before_next = live.clocked(now + 0.49, AudioFeatures::default()).beat_count;
            assert_eq!(before_next, *counts.last().unwrap());
            now += 0.5;
        }
        // One tap is not a tempo yet; from the second on the beats count up.
        assert_eq!(counts[0], 0);
        assert_eq!(
            counts[1] % 4,
            1,
            "the first tap was the first beat of a bar: {counts:?}"
        );
        assert!(counts[1..].windows(2).all(|w| w[1] == w[0] + 1), "{counts:?}");
    }

    #[test]
    fn without_a_tapped_tempo_the_music_keeps_its_own() {
        let live = Live::default();
        let music = AudioFeatures {
            bpm: 97.0,
            beat_count: 12,
            beat_phase: 0.3,
            ..AudioFeatures::default()
        };
        let out = live.features(5.0, music);
        assert_eq!((out.bpm, out.beat_count, out.beat_phase), (97.0, 12, 0.3));
    }

    #[test]
    fn a_beat_fired_by_hand_is_seen_by_every_kind_of_effect() {
        let mut live = Live::default();
        let before = live.features(1.0, AudioFeatures::default());
        live.hit(1.0);
        let now = live.features(1.0, AudioFeatures::default());
        assert_eq!(now.beat_count, before.beat_count + 1);
        assert!(now.beat > 0.99 && now.onset > 0.99);
        assert!(now.onsets.iter().all(|o| *o > 0.99));
        let later = live.features(1.5, AudioFeatures::default());
        assert!(later.onset < 0.01, "it fades, so the next one can fire");
        assert_eq!(later.beat_count, now.beat_count);
    }

    #[test]
    fn held_keys_flip_what_they_are_tied_to_and_only_while_held() {
        let mut show = Show::default();
        let layer = show.layers[0].id;
        let flash = show.add_layer("solid");
        show.layer_mut(flash).unwrap().enabled = false;
        let cut = show.bind("KeyA", "A", Action::Layer { id: layer }, KeyMode::Hold);
        let on = show.bind("KeyB", "B", Action::Layer { id: flash }, KeyMode::Hold);
        let dark = show.bind("KeyC", "C", Action::Blackout, KeyMode::Hold);
        let toggle = show.bind("KeyD", "D", Action::Hit, KeyMode::Toggle);

        let mut live = Live::default();
        assert_eq!(live.overlay(&show), show);
        assert!(live.hold(cut, true));
        assert!(live.hold(on, true));
        assert!(live.hold(dark, true));
        live.hold(toggle, true);
        let seen = live.overlay(&show);
        assert!(!seen.layers[0].enabled, "an effect that was on is cut");
        assert!(seen.layers[1].enabled, "an effect that was off flashes on");
        assert!(seen.master.blackout);
        assert!(show.layers[0].enabled, "the saved show is untouched");
        assert_eq!(live.status().held.len(), 4);

        assert!(live.hold(cut, false));
        assert!(live.hold(on, false));
        assert!(live.hold(dark, false));
        live.hold(toggle, false);
        assert_eq!(live.overlay(&show), show);
        assert!(!live.hold(cut, false), "releasing twice is harmless");
    }

    #[test]
    fn a_key_held_from_two_places_stays_held_until_both_let_go() {
        let mut live = Live::default();
        assert!(live.hold(7, true));
        assert!(!live.hold(7, true));
        assert!(!live.hold(7, false));
        assert!(live.is_held(7));
        assert!(live.hold(7, false));
        assert!(!live.is_held(7));
    }

    #[test]
    fn a_waiting_scene_starts_on_the_beat_or_the_bar_or_gives_up_waiting() {
        let mut live = Live::default();
        let with_tempo = |beat_count| AudioFeatures {
            bpm: 120.0,
            beat_count,
            ..AudioFeatures::default()
        };
        let go = |live: &Live, now: f64, launch, features: &AudioFeatures, beat| {
            live.should_launch(now, now - 0.016, launch, features, beat)
        };
        assert!(!go(&live, 0.0, Launch::Beat, &with_tempo(1), true), "nothing waits");
        live.pending = Some(Pending { scene: 3, since: 10.0 });

        assert!(go(&live, 10.1, Launch::Now, &with_tempo(1), false));
        assert!(!go(&live, 10.1, Launch::Beat, &with_tempo(1), false));
        assert!(go(&live, 10.1, Launch::Beat, &with_tempo(2), true));
        // A beat that began just before the scene was asked for does not count.
        assert!(!go(&live, 10.01, Launch::Beat, &with_tempo(2), true));

        assert!(!go(&live, 10.1, Launch::Bar, &with_tempo(2), true));
        assert!(!go(&live, 10.1, Launch::Bar, &with_tempo(4), false));
        assert!(go(&live, 10.1, Launch::Bar, &with_tempo(4), true));

        // No tempo, or a beat that never comes: do not leave the user waiting.
        assert!(go(&live, 10.1, Launch::Bar, &AudioFeatures::default(), false));
        assert!(go(&live, 10.0 + BEAT_WAIT_S + 0.1, Launch::Beat, &with_tempo(1), false));
        assert!(!go(&live, 10.0 + BEAT_WAIT_S + 0.1, Launch::Bar, &with_tempo(1), false));
        assert!(go(&live, 10.0 + BAR_WAIT_S + 0.1, Launch::Bar, &with_tempo(1), false));
    }
}
