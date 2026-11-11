//! Performing live: what is being held down, a tempo tapped by hand, beats
//! fired by hand, and a scene waiting for the beat.
//!
//! None of this is saved. Nothing here does I/O or reads a clock; times
//! are seconds since the engine started, passed in, so all of it is unit
//! tested.

use std::collections::HashMap;

use darkrgb_core::{Action, AudioFeatures, Cues, KeyMode, Launch, LayerHit, Show};
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

/// Autopilot changes scene after this long when there is no tempo to count
/// bars by: two seconds a bar, as at 120 beats a minute.
const BAR_WITHOUT_TEMPO_S: f64 = 2.0;
/// Autopilot leaves a scene up at least this long, however many new
/// sections the song goes through.
const AUTO_MIN_S: f64 = 4.0;

/// Where autopilot started counting for the scene that is playing.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AutoMark {
    at: f64,
    beat: u32,
    section: u32,
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
    /// The picture is stopped where it is.
    pub frozen: bool,
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
    /// The scene last switched to, to count "next" and "previous" from
    /// when what is playing has since been edited.
    pub last_scene: Option<u32>,
    /// Hits fired by hand for single layers: when the last one was, and
    /// how many there have been. Kept, because an effect tells a new hit
    /// by the count going up.
    fired: HashMap<u32, (f64, u32)>,
    /// The picture has been frozen by a press (not by a key being held).
    pub frozen: bool,
    auto: Option<AutoMark>,
    /// Feeds the shuffle; changes each time it is used.
    shuffle: u32,
}

impl Live {
    pub fn status(&self) -> LiveStatus {
        let mut held: Vec<u32> = self.held.keys().copied().collect();
        held.sort_unstable();
        LiveStatus {
            held,
            pending_scene: self.pending.map(|p| p.scene),
            tapped_bpm: self.tempo.bpm(),
            frozen: self.frozen,
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

    /// Releases every held key, unfreezes, and forgets a waiting scene.
    /// The tempo stays.
    pub fn let_go(&mut self) {
        self.held.clear();
        self.pending = None;
        self.frozen = false;
        self.last_scene = None;
        self.auto = None;
    }

    /// Tells autopilot that the scene has just changed, by whatever hand,
    /// so that it counts the new scene's bars from now.
    pub fn scene_changed(&mut self) {
        self.auto = None;
    }

    /// The scene autopilot wants to switch to on this frame, if any.
    /// `beat` is true when a new beat has just begun; `features` carry
    /// the tapped tempo if there is one.
    pub fn autopilot(&mut self, now: f64, show: &Show, features: &AudioFeatures, beat: bool) -> Option<u32> {
        let settings = show.live.auto;
        let usable: Vec<u32> = show.scenes.iter().filter(|s| s.auto).map(|s| s.id).collect();
        if !settings.enabled || usable.len() < 2 {
            self.auto = None;
            return None;
        }
        let Some(mark) = self.auto else {
            self.auto = Some(AutoMark {
                at: now,
                beat: features.beat_count,
                section: features.section,
            });
            return None;
        };
        let played = now - mark.at;
        // A new section right after a change of scene is let pass: acting
        // on it late, once the minimum time is up, would look arbitrary.
        // Bars are counted afresh from it, though: sections start on the
        // song's own phrase boundaries, so the next change by bar count
        // then lands on one.
        if features.section != mark.section && played < AUTO_MIN_S {
            self.auto = Some(AutoMark {
                at: mark.at,
                beat: features.beat_count,
                section: features.section,
            });
            return None;
        }
        let bars_up = if features.bpm > 0.0 {
            // On a beat, so the change lands in time.
            beat && features.beat_count.wrapping_sub(mark.beat) >= settings.every_bars.clamp(1, 64) * 4
        } else {
            played >= settings.every_bars.clamp(1, 64) as f64 * BAR_WITHOUT_TEMPO_S
        };
        let new_section = settings.on_drops && features.section != mark.section;
        if !bars_up && !new_section {
            return None;
        }

        let current = show.current_scene().or(self.last_scene);
        let next = if settings.shuffle {
            let others: Vec<u32> = usable.iter().copied().filter(|id| Some(*id) != current).collect();
            // A small generator is plenty for picking a scene.
            self.shuffle = self.shuffle.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) ^ (now * 1000.0) as u32;
            others[(self.shuffle >> 8) as usize % others.len()]
        } else {
            // The next usable scene after the current one, going round.
            let at = current.and_then(|id| show.scenes.iter().position(|s| s.id == id));
            let count = show.scenes.len();
            (1..=count)
                .map(|step| &show.scenes[(at.map_or(step - 1, |at| at + step)) % count])
                .find(|s| s.auto && Some(s.id) != current)
                .map(|s| s.id)?
        };
        self.auto = None;
        Some(next)
    }

    /// Fires a hit into each of `layers`, as if a beat had landed on them.
    pub fn fire(&mut self, layers: &[u32], now: f64) {
        for layer in layers {
            let entry = self.fired.entry(*layer).or_insert((now, 0));
            *entry = (now, entry.1.wrapping_add(1));
        }
    }

    /// What has been fired by hand, as the renderer wants it.
    pub fn cues(&self, now: f64) -> Cues {
        let hits = self
            .fired
            .iter()
            .map(|(layer, (at, count))| LayerHit {
                layer: *layer,
                pulse: (-(now - at).max(0.0) / PULSE_FADE_S).exp() as f32,
                count: *count,
            })
            .collect();
        Cues { hits }
    }

    /// Whether the picture should stand still: frozen by a press, or the
    /// other way round for as long as a freeze key is held.
    pub fn is_frozen(&self, show: &Show) -> bool {
        let held = show
            .bindings
            .iter()
            .any(|b| b.action == Action::Freeze && b.mode == KeyMode::Hold && self.is_held(b.id));
        self.frozen != held
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
        let held = |b: &&darkrgb_core::Binding| b.mode == KeyMode::Hold && self.is_held(b.id);
        // A held scene first, so that keys held with it act on its effects.
        for binding in show.bindings.iter().filter(held) {
            if let Action::Scene { id } = binding.action
                && let Some(scene) = show.scenes.iter().find(|s| s.id == id)
            {
                // Pinned folders stay, as they do on a real scene change.
                (out.layers, out.layer_groups) = show.scene_stack(scene);
            }
        }
        for binding in show.bindings.iter().filter(held) {
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
                // Scenes were dealt with above; freezing is the frame loop's.
                Action::Scene { .. }
                | Action::TapTempo
                | Action::Hit
                | Action::NextScene
                | Action::PrevScene
                | Action::Freeze => {}
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
    fn a_held_scene_shows_only_while_held_and_other_keys_act_on_it() {
        let mut show = Show::default();
        let plasma = show.layers[0].id;
        // A scene with one effect of its own, built aside and saved.
        let (scene, other) = {
            let saved = show.layers.clone();
            show.layers.clear();
            let solid = show.add_layer("solid");
            let scene = show.save_scene("Flash");
            show.layers = saved;
            (scene, solid)
        };
        let key = show.bind("KeyQ", "Q", Action::Scene { id: scene }, KeyMode::Hold);
        let cut = show.bind("KeyW", "W", Action::Layer { id: other }, KeyMode::Hold);
        assert_eq!(
            show.binding_for(Action::Scene { id: scene }).unwrap().mode,
            KeyMode::Hold
        );

        let mut live = Live::default();
        live.hold(key, true);
        let seen = live.overlay(&show);
        assert_eq!(seen.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [other]);
        assert_eq!(show.layers[0].id, plasma, "the saved show still plays its own effects");
        // A second key held with it flips an effect of the held scene.
        live.hold(cut, true);
        assert!(!live.overlay(&show).layers[0].enabled);
        live.hold(cut, false);
        live.hold(key, false);
        assert_eq!(live.overlay(&show), show);
    }

    #[test]
    fn fired_hits_are_per_layer_fade_and_keep_counting() {
        let mut live = Live::default();
        assert!(live.cues(0.0).hits.is_empty());
        live.fire(&[7, 9], 1.0);
        live.fire(&[9], 1.2);
        let cues = live.cues(1.2);
        let of = |cues: &Cues, layer| *cues.hits.iter().find(|h| h.layer == layer).unwrap();
        assert_eq!(of(&cues, 7).count, 1);
        assert_eq!(of(&cues, 9).count, 2);
        assert!(of(&cues, 9).pulse > 0.99);
        assert!(of(&cues, 7).pulse < 0.2, "fired 0.2 s ago, mostly faded");
        // Long after, the pulse is gone but the count stays, so nothing fires twice.
        let later = live.cues(60.0);
        assert!(of(&later, 9).pulse < 1e-6);
        assert_eq!(of(&later, 9).count, 2);
    }

    #[test]
    fn freeze_by_press_and_by_holding() {
        let mut show = Show::default();
        let hold = show.bind("KeyZ", "Z", Action::Freeze, KeyMode::Hold);
        let mut live = Live::default();
        assert!(!live.is_frozen(&show));
        live.hold(hold, true);
        assert!(live.is_frozen(&show));
        live.hold(hold, false);
        assert!(!live.is_frozen(&show));
        // Frozen by a press, holding the key lets it run for a moment.
        live.frozen = true;
        assert!(live.is_frozen(&show) && live.status().frozen);
        live.hold(hold, true);
        assert!(!live.is_frozen(&show));
        live.let_go();
        assert!(!live.is_frozen(&show), "a new show starts moving");
    }

    /// A show with scenes A, B, C and autopilot on.
    fn auto_show() -> (Show, [u32; 3]) {
        let mut show = Show::default();
        let a = show.save_scene("A");
        show.add_layer("solid");
        let b = show.save_scene("B");
        show.add_layer("flow");
        let c = show.save_scene("C");
        show.recall_scene(a);
        show.live.auto.enabled = true;
        show.live.auto.every_bars = 2;
        (show, [a, b, c])
    }

    fn music(beat_count: u32, section: u32) -> AudioFeatures {
        AudioFeatures {
            bpm: 120.0,
            beat_count,
            section,
            active: true,
            ..AudioFeatures::default()
        }
    }

    #[test]
    fn autopilot_changes_scene_every_so_many_bars_in_order() {
        let (mut show, [a, b, c]) = auto_show();
        let mut live = Live::default();
        let mut launched = Vec::new();
        // 120 beats a minute: a beat every half second, frames at 50 a second.
        for frame in 0..(50 * 26) {
            let now = frame as f64 / 50.0;
            let beat_count = (now * 2.0) as u32;
            let beat = frame > 0 && beat_count != (((frame - 1) as f64 / 50.0) * 2.0) as u32;
            if let Some(scene) = live.autopilot(now, &show, &music(beat_count, 0), beat) {
                launched.push((now, scene));
                show.recall_scene(scene);
                live.last_scene = Some(scene);
                live.scene_changed();
            }
        }
        // Two bars are eight beats, four seconds.
        let scenes: Vec<u32> = launched.iter().map(|(_, s)| *s).collect();
        assert_eq!(scenes, [b, c, a, b, c, a], "{launched:?}");
        for pair in launched.windows(2) {
            let gap = pair[1].0 - pair[0].0;
            assert!((gap - 4.0).abs() < 0.05, "scenes {gap} s apart");
        }
        // Each change lands on a beat.
        assert!(launched.iter().all(|(at, _)| (at * 2.0).fract() < 0.05), "{launched:?}");
    }

    #[test]
    fn autopilot_changes_at_once_on_a_new_section_but_not_too_often() {
        let (show, [_, b, _]) = auto_show();
        let mut live = Live::default();
        assert_eq!(
            live.autopilot(0.0, &show, &music(0, 0), false),
            None,
            "it starts counting"
        );
        // A new section a second in is too soon after the last change. It
        // is let pass, not saved up for later.
        assert_eq!(live.autopilot(1.0, &show, &music(2, 1), false), None);
        assert_eq!(live.autopilot(AUTO_MIN_S + 0.1, &show, &music(3, 1), false), None);
        // The next one, with the scene well under way, changes it at once.
        assert_eq!(live.autopilot(AUTO_MIN_S + 2.0, &show, &music(4, 2), false), Some(b));

        // With that switched off, sections are ignored.
        let mut calm = show.clone();
        calm.live.auto.on_drops = false;
        let mut live = Live::default();
        live.autopilot(0.0, &calm, &music(0, 0), false);
        assert_eq!(live.autopilot(5.0, &calm, &music(3, 4), false), None);
    }

    #[test]
    fn autopilot_keeps_time_by_the_clock_when_there_is_no_tempo() {
        let (show, [_, b, _]) = auto_show();
        let mut live = Live::default();
        let silence = AudioFeatures::default();
        live.autopilot(10.0, &show, &silence, false);
        assert_eq!(live.autopilot(13.9, &show, &silence, false), None);
        assert_eq!(
            live.autopilot(14.1, &show, &silence, false),
            Some(b),
            "two bars at two seconds each"
        );
    }

    #[test]
    fn autopilot_skips_scenes_marked_to_skip_and_needs_two_to_choose_from() {
        let (mut show, [a, b, c]) = auto_show();
        show.scenes[1].auto = false;
        let mut live = Live::default();
        let silence = AudioFeatures::default();
        live.autopilot(0.0, &show, &silence, false);
        assert_eq!(live.autopilot(5.0, &show, &silence, false), Some(c), "B is skipped");
        let _ = (a, b);
        // One usable scene is nothing to change between.
        show.scenes[2].auto = false;
        live.autopilot(6.0, &show, &silence, false);
        assert_eq!(live.autopilot(60.0, &show, &silence, false), None);
        // Off means off.
        show.scenes[1].auto = true;
        show.scenes[2].auto = true;
        show.live.auto.enabled = false;
        assert_eq!(live.autopilot(120.0, &show, &silence, false), None);
    }

    #[test]
    fn shuffled_autopilot_never_repeats_the_scene_that_is_playing() {
        let (mut show, _) = auto_show();
        show.live.auto.shuffle = true;
        let mut live = Live::default();
        let silence = AudioFeatures::default();
        let mut seen = std::collections::HashSet::new();
        let mut now = 0.0;
        for _ in 0..40 {
            live.autopilot(now, &show, &silence, false);
            now += 5.0;
            let next = live.autopilot(now, &show, &silence, false).expect("time is up");
            assert_ne!(Some(next), show.current_scene());
            show.recall_scene(next);
            live.scene_changed();
            seen.insert(next);
        }
        assert_eq!(seen.len(), 3, "every scene gets its turn");
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
