//! Performing live: what is being held down, a tempo tapped by hand, beats
//! fired by hand, and a scene waiting for the beat.
//!
//! None of this is saved. Nothing here does I/O or reads a clock; times
//! are seconds since the engine started, passed in, so all of it is unit
//! tested.

use std::collections::HashMap;

use darkrgb_core::timeline::Moment;
use darkrgb_core::{Action, AudioFeatures, Cues, KeyMode, Launch, LayerHit, Mood, Show};
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

    /// The tempo of the taps so far, whether or not they are the clock.
    fn tapped_bpm(&self) -> f32 {
        if self.taps.len() < 2 {
            return 0.0;
        }
        let interval = (self.taps[self.taps.len() - 1] - self.taps[0]) / (self.taps.len() - 1) as f64;
        ((60.0 / interval) as f32).clamp(MIN_BPM, MAX_BPM)
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

/// What a performer does to the beat, on top of wherever it comes from.
///
/// The music's own beat can be right in tempo and wrong in everything
/// else: counted at twice or half the speed a person would clap, a
/// little ahead or behind, with its bars starting on the wrong beat.
/// These put it right without taking the clock away from the music, so
/// it keeps following the song and never drifts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    /// Half time, as heard, or double time: 0.5, 1 or 2.
    pub scale: f32,
    /// Beats added to where the beat is.
    pub nudge: f64,
    /// A beat number that begins a bar.
    bar_origin: i64,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            scale: 1.0,
            nudge: 0.0,
            bar_origin: 0,
        }
    }
}

/// The most a beat can be nudged either way, in beats.
const MAX_NUDGE: f64 = 2.0;

/// A beat sent by another machine as MIDI clock: 24 ticks to the beat.
#[derive(Clone, Copy, Debug, Default)]
pub struct MidiClock {
    /// Ticks since the sender last started.
    ticks: u64,
    /// When the last tick came, and the time between ticks, smoothed.
    last: f64,
    interval: f64,
    heard: bool,
    /// The sender has said "start": the next tick is the first of a bar.
    starting: bool,
}

impl MidiClock {
    const TICKS_A_BEAT: f64 = 24.0;
    /// A clock that has sent nothing for this long has stopped.
    const GONE_S: f64 = 0.5;

    pub fn tick(&mut self, now: f64) {
        let gap = now - self.last;
        if self.heard && gap > 0.0 && gap < Self::GONE_S {
            // Ticks arrive with a millisecond or two of jitter each.
            self.interval = if self.interval > 0.0 {
                self.interval * 0.9 + gap * 0.1
            } else {
                gap
            };
        } else {
            self.interval = 0.0;
        }
        self.ticks = if std::mem::take(&mut self.starting) {
            0
        } else {
            self.ticks + 1
        };
        self.last = now;
        self.heard = true;
    }

    /// The sender started from the top.
    pub fn start(&mut self) {
        self.starting = true;
    }

    /// The tempo being sent, if a clock is arriving now.
    pub fn bpm(&self, now: f64) -> Option<f32> {
        let arriving = self.heard && now - self.last < Self::GONE_S && self.interval > 0.0;
        arriving.then(|| ((60.0 / (Self::TICKS_A_BEAT * self.interval)) as f32).clamp(20.0, 400.0))
    }

    /// Beats since the sender started, and the tempo.
    fn position(&self, now: f64) -> Option<(f64, f32)> {
        let bpm = self.bpm(now)?;
        let into_tick = ((now - self.last) / self.interval).clamp(0.0, 1.0);
        Some(((self.ticks as f64 + into_tick) / Self::TICKS_A_BEAT, bpm))
    }
}

/// Where the beat comes from, to tell the performer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BeatSource {
    #[default]
    Music,
    Tapped,
    Midi,
}

/// What clients are told about the beat clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct ClockStatus {
    pub source: BeatSource,
    /// 0.5, 1 or 2.
    pub scale: f32,
    /// Beats the beat has been nudged by.
    pub nudge: f32,
    /// The tempo of a MIDI clock that is arriving, followed or not.
    pub midi_bpm: Option<f32>,
}

/// Autopilot changes scene after this long when there is no tempo to count
/// bars by: two seconds a bar, as at 120 beats a minute.
const BAR_WITHOUT_TEMPO_S: f64 = 2.0;
/// Autopilot leaves a scene up at least this long, however many new
/// sections the song goes through.
const AUTO_MIN_S: f64 = 4.0;
/// The shortest fade into and out of the scene shown in silence.
const IDLE_FADE_S: f32 = 2.0;

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
    /// The music has stopped and the scene chosen for that is showing.
    pub idle: bool,
    /// The show's schedule has the lights off at this time of day.
    pub asleep: bool,
    /// Where the timeline is.
    pub timeline: TransportStatus,
    /// Where the beat comes from and how it has been corrected.
    pub clock: ClockStatus,
    /// The scene being changed in the effects panel while the lights
    /// play something else, if any.
    pub editing: Option<u32>,
    /// True while the lights play something other than what the effects
    /// panel shows.
    pub blind: bool,
    /// The scene the lights went to last, for marking it while blind.
    pub on_air_scene: Option<u32>,
}

/// What clients are told about the timeline's playback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct TransportStatus {
    pub playing: bool,
    /// What is played by hand is being written onto the timeline.
    pub recording: bool,
    /// Seconds from the start, as of the last frame.
    pub position: f64,
}

/// The timeline's clock: playing or stopped, and where.
///
/// It runs on the engine's own time. A window playing the song keeps it
/// honest with [`Transport::sync`], because sound is what people notice
/// being late, so the lights follow the sound and not the other way.
#[derive(Clone, Copy, Debug, Default)]
pub struct Transport {
    pub playing: bool,
    pub recording: bool,
    /// Where it was at `anchor`, in seconds from the start.
    position: f64,
    /// The engine time `position` was true at.
    anchor: f64,
}

impl Transport {
    /// Seconds from the start of the timeline, at engine time `now`.
    pub fn position(&self, now: f64) -> f64 {
        if self.playing {
            (self.position + (now - self.anchor)).max(0.0)
        } else {
            self.position
        }
    }

    /// Starts playing, from `from` or from where it stands.
    pub fn play(&mut self, now: f64, from: Option<f64>) {
        self.position = from.unwrap_or(self.position(now)).max(0.0);
        self.anchor = now;
        self.playing = true;
    }

    /// Stops where it is. Recording stops with it.
    pub fn stop(&mut self, now: f64) {
        self.position = self.position(now);
        self.playing = false;
        self.recording = false;
    }

    /// Jumps to `position`, playing or not.
    pub fn seek(&mut self, now: f64, position: f64) {
        self.position = position.max(0.0);
        self.anchor = now;
    }

    /// Told where the song really is by the window playing it. Small
    /// differences are left alone, so the clock does not jitter; a real
    /// one is taken at once. Returns whether the clock was moved.
    pub fn sync(&mut self, now: f64, position: f64) -> bool {
        if !self.playing || (self.position(now) - position).abs() < SYNC_SLACK_S {
            return false;
        }
        self.seek(now, position);
        true
    }
}

/// How far the engine's clock may be from the song before it is moved.
const SYNC_SLACK_S: f64 = 0.03;

/// What a step of the timeline changed, for the frame loop to act on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimelineStep {
    /// The scene or the pads the timeline calls for are not what they were.
    pub changed: bool,
    /// Pads whose clips have just begun.
    pub entered: Vec<u32>,
    /// Playback reached the end and stopped.
    pub ended: bool,
    /// The timeline is running.
    pub playing: bool,
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
    /// What the lights play while the effects panel holds a scene that
    /// is being changed out of sight: the effects and their folders.
    pub on_air: Option<(Vec<darkrgb_core::Layer>, Vec<darkrgb_core::LayerGroup>)>,
    /// The scene being changed out of sight.
    pub editing: Option<u32>,
    /// Hits fired by hand for single layers: when the last one was, and
    /// how many there have been. Kept, because an effect tells a new hit
    /// by the count going up.
    fired: HashMap<u32, (f64, u32)>,
    /// The picture has been frozen by a press (not by a key being held).
    pub frozen: bool,
    auto: Option<AutoMark>,
    /// Feeds the shuffle; changes each time it is used.
    shuffle: u32,
    /// When the sound was last heard to stop, while it stays stopped.
    quiet_since: Option<f64>,
    /// The music has stopped for long enough: the scene chosen for
    /// silence is showing in place of what is playing.
    pub idle: bool,
    /// The last change of scene was into or out of silence, which always
    /// fades.
    idle_fade: bool,
    /// The show's schedule has the lights off at this time of day.
    pub asleep: bool,
    /// The timeline's clock.
    pub transport: Transport,
    /// What the timeline calls for on this frame: nothing while stopped.
    moment: Moment,
    /// The engine time of the last frame. Clients are told where the
    /// timeline's clock stood then, which after a jump is the new place
    /// and never the old one.
    frame_at: f64,
    /// Pads being recorded: the clip each press began, by pad.
    pub recording: HashMap<u32, u32>,
    /// The performer's corrections to the beat.
    pub clock: Clock,
    /// A beat arriving as MIDI clock, and whether the show follows it.
    pub midi_clock: MidiClock,
    pub follow_midi: bool,
    /// Milliseconds the lights run ahead of the beat heard in the music.
    pub lead_ms: f32,
}

impl Live {
    /// Moves the timeline on to engine time `now` and works out what it
    /// calls for. Stopped, it calls for nothing and the show is itself.
    pub fn timeline_step(&mut self, now: f64, show: &Show) -> TimelineStep {
        let mut step = TimelineStep::default();
        let mut position = self.transport.position(now);
        let length = show.timeline.length as f64;
        if self.transport.playing && position >= length {
            if show.timeline.repeat {
                self.transport.seek(now, 0.0);
                position = 0.0;
            } else {
                self.transport.stop(now);
                self.transport.seek(now, length);
                position = length;
                step.ended = true;
            }
        }
        self.frame_at = now;
        let moment = if self.transport.playing {
            show.timeline.at(position as f32)
        } else {
            Moment::default()
        };
        if moment != self.moment {
            step.entered = moment
                .pads
                .iter()
                .copied()
                .filter(|pad| !self.moment.pads.contains(pad))
                .collect();
            // A change of scene by the timeline fades like any other.
            if moment.scene != self.moment.scene {
                self.scene_serial += 1;
            }
            self.moment = moment;
            step.changed = true;
        }
        step.changed |= step.ended;
        step.playing = self.transport.playing;
        step
    }

    /// Whether what is played by hand should be written down: recording
    /// is on and the timeline is running.
    pub fn is_recording(&self) -> bool {
        self.transport.playing && self.transport.recording
    }

    /// Whether the timeline is deciding the scene right now.
    pub fn timeline_has_the_scene(&self) -> bool {
        self.moment.scene.is_some()
    }

    /// Whether a pad counts as held: by a key, or by a clip on the timeline.
    fn pad_on(&self, binding: &darkrgb_core::Binding) -> bool {
        (binding.mode == KeyMode::Hold && self.is_held(binding.id))
            || self.moment.pads.contains(&binding.id)
            // A pad switched on while recording is on until its second press.
            || self.recording.contains_key(&binding.id)
    }

    pub fn status(&self) -> LiveStatus {
        let mut held: Vec<u32> = self.held.keys().copied().collect();
        held.sort_unstable();
        LiveStatus {
            held,
            pending_scene: self.pending.map(|p| p.scene),
            tapped_bpm: self.tempo.bpm(),
            frozen: self.frozen,
            idle: self.idle,
            asleep: self.asleep,
            timeline: TransportStatus {
                playing: self.transport.playing,
                recording: self.transport.recording,
                position: self.transport.position(self.frame_at),
            },
            clock: ClockStatus {
                source: self.source(self.frame_at),
                scale: self.clock.scale,
                nudge: self.clock.nudge as f32,
                midi_bpm: self.midi_clock.bpm(self.frame_at),
            },
            editing: self.editing.filter(|_| self.on_air.is_some()),
            blind: self.on_air.is_some(),
            on_air_scene: self.last_scene.filter(|_| self.on_air.is_some()),
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
        // A scene chosen on purpose is shown, silence or not. The wait
        // for silence starts again from here.
        self.idle = false;
        self.quiet_since = None;
    }

    /// Watches for the music stopping and starting. After the show's
    /// chosen stretch of silence the scene set aside for it takes over,
    /// and what was playing comes back with the sound. Returns true when
    /// that has just changed.
    pub fn idle_step(&mut self, now: f64, show: &Show, features: &AudioFeatures) -> bool {
        let idle = show.live.idle;
        let has_scene = idle.scene.is_some_and(|id| show.scenes.iter().any(|s| s.id == id));
        let waiting = if !has_scene || features.active {
            self.quiet_since = None;
            false
        } else {
            let since = *self.quiet_since.get_or_insert(now);
            now - since >= idle.after.clamp(3.0, 600.0) as f64
        };
        if waiting == self.idle {
            return false;
        }
        self.idle = waiting;
        self.idle_fade = true;
        self.scene_serial += 1;
        // Autopilot counts afresh when the music is back.
        self.auto = None;
        true
    }

    /// Checks the show's hours against the time of day, given in minutes
    /// after midnight. Returns true when the lights have just gone off or
    /// come on because of it.
    pub fn schedule_step(&mut self, minute: u16, show: &Show) -> bool {
        let asleep = !show.schedule.lit_at(minute);
        std::mem::replace(&mut self.asleep, asleep) != asleep
    }

    /// How long the change of scene just made should fade for, given the
    /// show's own setting. Going into silence and coming out of it never
    /// cut.
    pub fn take_fade(&mut self, show_fade: f32) -> f32 {
        if std::mem::take(&mut self.idle_fade) {
            show_fade.max(IDLE_FADE_S)
        } else {
            show_fade
        }
    }

    /// The scene autopilot wants to switch to on this frame, if any.
    /// `beat` is true when a new beat has just begun; `features` carry
    /// the tapped tempo if there is one.
    pub fn autopilot(&mut self, now: f64, show: &Show, features: &AudioFeatures, beat: bool) -> Option<u32> {
        let settings = show.live.auto;
        let usable: Vec<u32> = show
            .scenes
            .iter()
            .filter(|s| s.auto && show.in_use(s))
            .map(|s| s.id)
            .collect();
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
            // On the first beat of a bar, so the change lands where the
            // music itself changes.
            beat && features.bar_beat == 0
                && features.beat_count.wrapping_sub(mark.beat) + 3 >= settings.every_bars.clamp(1, 64) * 4
        } else {
            played >= settings.every_bars.clamp(1, 64) as f64 * BAR_WITHOUT_TEMPO_S
        };
        let new_section = settings.on_drops && features.section != mark.section;
        if !bars_up && !new_section {
            return None;
        }

        // While a scene is being changed out of sight, the show holds
        // that scene, not the one the lights are on.
        let current = if self.on_air.is_some() {
            self.last_scene
        } else {
            show.current_scene().or(self.last_scene)
        };
        // Scenes say which parts of a song they suit. In a big part the
        // calm ones are passed over, and in a quiet part the big ones,
        // as long as that leaves something to change to.
        let energy = if features.active { features.intensity } else { 0.5 };
        let part = if features.drop > 0.5 || energy > 0.75 {
            Mood::Big
        } else if energy < 0.35 {
            Mood::Calm
        } else {
            Mood::Any
        };
        let suits = |mood: Mood| match part {
            Mood::Big => mood != Mood::Calm,
            Mood::Calm => mood != Mood::Big,
            Mood::Any => true,
        };
        let fitting: Vec<u32> = show
            .scenes
            .iter()
            .filter(|s| s.auto && suits(s.mood) && Some(s.id) != current)
            .map(|s| s.id)
            .collect();
        let usable = if fitting.is_empty() { usable } else { fitting };
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
                .find(|s| usable.contains(&s.id) && Some(s.id) != current)
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
            .any(|b| b.action == Action::Freeze && self.pad_on(b));
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
    /// One tap of the tempo key.
    ///
    /// Tapped along to a beat the music already gives (at its speed, or
    /// half or double it), the taps correct that beat: where it falls,
    /// which speed it is counted at, and which beat is the first of the
    /// bar. The clock stays with the music. Tapped at a tempo of their
    /// own, the taps become the clock, as they always did.
    pub fn tap(&mut self, now: f64, raw: AudioFeatures) {
        let count = self.clocked(now, raw).beat_count;
        self.tempo.tap(now, count);
        let taps = self.tempo.taps.len();
        let following_midi = self.source(now) == BeatSource::Midi;
        let fits = (taps >= 3 && raw.bpm > 0.0 && !following_midi)
            .then(|| {
                let ratio = self.tempo.tapped_bpm() / raw.bpm;
                [0.5f32, 1.0, 2.0].into_iter().find(|s| (ratio / s - 1.0).abs() < 0.08)
            })
            .flatten();
        match fits {
            Some(scale) => {
                // Where the music's beat is at this speed, before any nudge.
                let plain = self.music_position(&raw, scale, 0.0);
                // This tap is a beat: the nudge moves the nearest one onto it.
                let on = plain.round();
                self.clock.scale = scale;
                self.clock.nudge = on - plain;
                // And the first tap of the run was beat one.
                self.clock.bar_origin = on as i64 - (taps as i64 - 1);
                self.tempo.bpm = 0.0;
            }
            None if self.tempo.bpm > 0.0 => {
                // A clock of its own, counted from its first tap.
                self.clock = Clock::default();
            }
            None => {}
        }
    }

    /// Where the music's beat is, in beats, at a speed and with a nudge:
    /// its own position, ahead by the lead the lights are given.
    fn music_position(&self, raw: &AudioFeatures, scale: f32, nudge: f64) -> f64 {
        let lead = self.lead_ms.clamp(0.0, 250.0) as f64 / 1000.0 * (raw.bpm * scale) as f64 / 60.0;
        ((raw.beat_count as f64 + raw.beat_phase as f64) * scale as f64 + nudge + lead).max(0.0)
    }

    /// Where the beat is coming from at `now`.
    pub fn source(&self, now: f64) -> BeatSource {
        if self.follow_midi && self.midi_clock.bpm(now).is_some() {
            BeatSource::Midi
        } else if self.tempo.bpm > 0.0 {
            BeatSource::Tapped
        } else {
            BeatSource::Music
        }
    }

    /// The audio features with the beat as the show should count it: from
    /// a MIDI clock, from taps, or from the music with the performer's
    /// corrections and the lights' lead. Bars are counted on this, before
    /// beats fired by hand are added.
    pub fn clocked(&self, now: f64, mut features: AudioFeatures) -> AudioFeatures {
        let position = match self.source(now) {
            BeatSource::Midi => self.midi_clock.position(now).map(|(at, bpm)| {
                features.bpm = bpm;
                features.coasting = false;
                at
            }),
            BeatSource::Tapped => {
                self.tempo.apply(now, &mut features);
                features.coasting = false;
                None
            }
            BeatSource::Music if features.bpm > 0.0 => {
                let at = self.music_position(&features, self.clock.scale, self.clock.nudge);
                features.bpm *= self.clock.scale;
                Some(at)
            }
            BeatSource::Music => None,
        };
        if let Some(at) = position {
            features.beat_count = at.floor() as u32;
            features.beat_phase = at.fract() as f32;
            let since_beat = at.fract() * 60.0 / features.bpm.max(1.0) as f64;
            features.beat = (-since_beat / PULSE_FADE_S).exp() as f32;
        }
        features.bar_beat = (features.beat_count as i64 - self.clock.bar_origin).rem_euclid(4) as u8;
        features
    }

    /// Says the beat nearest to now is the first of a bar.
    pub fn downbeat(&mut self, now: f64, raw: AudioFeatures) {
        let f = self.clocked(now, raw);
        let nearest = f.beat_count as i64 + i64::from(f.beat_phase >= 0.5);
        self.clock.bar_origin = nearest;
    }

    /// Counts the music's beat at half its speed, as it is, or double.
    /// The beat that is playing keeps its place in the bar.
    pub fn set_scale(&mut self, now: f64, raw: AudioFeatures, scale: f32) {
        let before = self.clocked(now, raw);
        self.clock.scale = scale;
        let after = self.clocked(now, raw);
        self.clock.bar_origin = after.beat_count as i64 - before.bar_beat as i64;
    }

    /// Moves the beat earlier (negative) or later by a part of a beat.
    pub fn nudge(&mut self, beats: f64) {
        self.clock.nudge = (self.clock.nudge + beats).clamp(-MAX_NUDGE, MAX_NUDGE);
    }

    /// Hands the clock back to the music, as it hears it.
    pub fn follow_music(&mut self) {
        self.tempo.clear();
        self.clock.scale = 1.0;
        self.clock.nudge = 0.0;
    }

    /// A message of MIDI clock: a tick, or the sender starting.
    pub fn midi_tick(&mut self, now: f64) {
        let starting = self.midi_clock.starting;
        self.midi_clock.tick(now);
        // A sender that starts from the top starts a bar.
        if starting && self.follow_midi {
            self.clock.bar_origin = 0;
        }
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
        // While a scene is being changed out of sight, the lights go on
        // with what they had; the show holds what is being changed.
        if let Some((layers, groups)) = &self.on_air {
            (out.layers, out.layer_groups) = (layers.clone(), groups.clone());
        }
        // With the music stopped, the scene chosen for silence shows. The
        // saved show is not changed: when the music is back, so is what
        // was playing.
        if self.idle
            && let Some(scene) = show
                .live
                .idle
                .scene
                .and_then(|id| show.scenes.iter().find(|s| s.id == id))
        {
            (out.layers, out.layer_groups) = show.scene_stack(scene);
        }
        // A timeline that is playing decides the scene. Again the saved
        // show is left as it is, for when playback stops.
        if let Some(scene) = self.moment.scene.and_then(|id| show.scenes.iter().find(|s| s.id == id)) {
            (out.layers, out.layer_groups) = show.scene_stack(scene);
        }
        // Outside the show's hours the lights are dark. The show's own
        // blackout switch is not touched.
        if self.asleep {
            out.master.blackout = true;
        }
        // Held by a key, or by a clip on the timeline: either way once.
        let held = |b: &&darkrgb_core::Binding| self.pad_on(b);
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
                Action::Colours { id } => {
                    out.live.colours = if out.live.colours == Some(id) { None } else { Some(id) };
                }
                // Scenes were dealt with above; freezing is the frame loop's.
                Action::Scene { .. }
                | Action::TapTempo
                | Action::Hit
                | Action::NextScene
                | Action::PrevScene
                | Action::Freeze
                | Action::Downbeat => {}
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
            Launch::Bar => (beat && features.bar_beat == 0) || waited > BAR_WAIT_S,
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
            bar_beat: (beat_count % 4) as u8,
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
    fn autopilot_picks_scenes_that_suit_the_part_of_the_song() {
        // A is calm, B suits anything, C is big. A is playing.
        let (mut show, [a, b, c]) = auto_show();
        show.scenes[0].mood = Mood::Calm;
        show.scenes[2].mood = Mood::Big;
        let part = |intensity: f32, drop: f32| AudioFeatures {
            intensity,
            drop,
            active: true,
            ..AudioFeatures::default()
        };
        // Asks what would come next, without a tempo: by the clock.
        let next = |show: &Show, features: &AudioFeatures| {
            let mut live = Live::default();
            live.autopilot(0.0, show, features, false);
            live.autopilot(5.0, show, features, false)
        };
        // A quiet part: the big scene is passed over, wherever it is in the order.
        show.recall_scene(b);
        assert_eq!(
            next(&show, &part(0.1, 0.0)),
            Some(a),
            "from B, quiet: C is skipped, round to A"
        );
        // A big part: the calm one is passed over.
        show.recall_scene(c);
        assert_eq!(next(&show, &part(0.9, 0.0)), Some(b), "from C, big: A is skipped");
        show.recall_scene(b);
        assert_eq!(next(&show, &part(0.5, 1.0)), Some(c), "a drop counts as big");
        // In between, the order is the order.
        show.recall_scene(a);
        assert_eq!(next(&show, &part(0.5, 0.0)), Some(b));
        show.recall_scene(b);
        assert_eq!(next(&show, &part(0.5, 0.0)), Some(c));
        // If nothing else suits, it still changes rather than sticking.
        show.scenes[1].mood = Mood::Big;
        show.recall_scene(a);
        assert_eq!(
            next(&show, &part(0.1, 0.0)),
            Some(b),
            "only big scenes are left to go to"
        );
        // Shuffled, the same rule holds.
        show.scenes[1].mood = Mood::Any;
        show.live.auto.shuffle = true;
        show.recall_scene(b);
        for _ in 0..10 {
            assert_eq!(next(&show, &part(0.1, 0.0)), Some(a));
        }
    }

    #[test]
    fn silence_brings_the_scene_chosen_for_it_and_music_brings_the_show_back() {
        let (mut show, [a, b, _]) = auto_show();
        show.live.idle.scene = Some(b);
        show.live.idle.after = 10.0;
        show.live.fade = 0.0;
        let mut live = Live::default();
        let quiet = AudioFeatures::default();
        let playing = music(1, 0);
        let stack = |live: &Live, show: &Show| live.overlay(show).layers.len();
        let own = show.layers.len();
        let waiting = show.scenes[1].layers.len();
        assert_ne!(own, waiting);

        assert!(!live.idle_step(0.0, &show, &playing));
        assert!(!live.idle_step(1.0, &show, &quiet), "silence has only just begun");
        assert!(
            !live.idle_step(10.5, &show, &quiet),
            "counted from when it began, not from zero"
        );
        let serial = live.scene_serial;
        assert!(live.idle_step(11.0, &show, &quiet), "ten seconds of silence");
        assert!(live.idle && live.status().idle);
        assert_eq!(live.scene_serial, serial + 1, "the frame loop sees a change of scene");
        assert_eq!(stack(&live, &show), waiting, "the waiting scene shows");
        assert_eq!(show.current_scene(), Some(a), "and the show itself is as it was");
        assert_eq!(live.take_fade(0.0), IDLE_FADE_S, "it fades even when scene changes cut");
        assert_eq!(live.take_fade(0.0), 0.0, "once");
        assert!(
            !live.idle_step(60.0, &show, &quiet),
            "nothing more happens while it stays quiet"
        );

        // The music is back: so is what was playing, at once.
        assert!(live.idle_step(61.0, &show, &playing));
        assert!(!live.idle);
        assert_eq!(stack(&live, &show), own);
        assert_eq!(live.take_fade(0.5), IDLE_FADE_S);

        // A gap in the music shorter than the wait changes nothing.
        assert!(!live.idle_step(62.0, &show, &quiet));
        assert!(!live.idle_step(70.0, &show, &playing));
        assert!(!live.idle_step(71.0, &show, &quiet));
        assert!(
            !live.idle_step(80.0, &show, &quiet),
            "nine seconds since it stopped again"
        );

        // Choosing a scene by hand ends the waiting, and the wait starts over.
        assert!(live.idle_step(82.0, &show, &quiet));
        live.scene_changed();
        assert!(!live.idle);
        assert!(!live.idle_step(83.0, &show, &quiet));
        assert!(live.idle_step(93.5, &show, &quiet));

        // With no scene chosen for silence, or one that is gone, nothing happens.
        let mut live = Live::default();
        show.live.idle.scene = None;
        assert!(!live.idle_step(0.0, &show, &quiet) && !live.idle_step(1000.0, &show, &quiet));
        show.live.idle.scene = Some(9999);
        assert!(!live.idle_step(2000.0, &show, &quiet));
        assert_eq!(stack(&live, &show), own);
        // Switching it off while waiting ends the waiting.
        show.live.idle.scene = Some(b);
        live.idle_step(3000.0, &show, &quiet);
        assert!(live.idle_step(3011.0, &show, &quiet));
        show.live.idle.scene = None;
        assert!(live.idle_step(3012.0, &show, &quiet));
        assert!(!live.idle);
    }

    /// The music's beat as the analysis reports it: at `bpm`, `beats` in.
    fn heard(bpm: f32, beats: f64) -> AudioFeatures {
        AudioFeatures {
            bpm,
            beat_count: beats.floor() as u32,
            beat_phase: beats.fract() as f32,
            active: true,
            ..AudioFeatures::default()
        }
    }

    #[test]
    fn the_lights_lead_the_beat_they_hear() {
        let mut live = Live::default();
        // Nothing asked for: the beat is passed on as it is.
        let f = live.clocked(0.0, heard(120.0, 10.25));
        assert_eq!((f.beat_count, f.bpm), (10, 120.0));
        assert!((f.beat_phase - 0.25).abs() < 1e-6);
        assert_eq!(f.bar_beat, 2);
        // A lead of 50 ms at 120 is a tenth of a beat: 0.95 of the way
        // through beat 10 as heard, the lights are already on beat 11.
        live.lead_ms = 50.0;
        let f = live.clocked(0.0, heard(120.0, 10.95));
        assert_eq!(f.beat_count, 11);
        assert!((f.beat_phase - 0.05).abs() < 1e-4);
        assert!(f.beat > 0.7, "and the beat's flash has just begun: {}", f.beat);
        // Without a tempo there is nothing to run ahead of.
        let none = live.clocked(
            0.0,
            AudioFeatures {
                beat_count: 7,
                ..AudioFeatures::default()
            },
        );
        assert_eq!((none.beat_count, none.bpm), (7, 0.0));
    }

    #[test]
    fn the_beat_can_be_halved_doubled_nudged_and_given_its_first_beat() {
        let mut live = Live::default();
        let raw = heard(140.0, 20.5);
        // Half time: every second beat, at half the tempo.
        live.set_scale(0.0, raw, 0.5);
        let f = live.clocked(0.0, raw);
        assert_eq!((f.bpm, f.beat_count), (70.0, 10));
        assert!((f.beat_phase - 0.25).abs() < 1e-6);
        // The beat that is playing kept its place in the bar.
        assert_eq!(f.bar_beat, live_bar(&Live::default(), raw));
        live.set_scale(0.0, raw, 2.0);
        let f = live.clocked(0.0, raw);
        assert_eq!((f.bpm, f.beat_count), (280.0, 41));
        live.set_scale(0.0, raw, 1.0);

        // A nudge of a quarter of a beat later; they add up, to a limit.
        live.nudge(0.25);
        assert!((live.clocked(0.0, raw).beat_phase - 0.75).abs() < 1e-6);
        live.nudge(-0.5);
        assert!((live.clocked(0.0, raw).beat_phase - 0.25).abs() < 1e-6);
        for _ in 0..100 {
            live.nudge(1.0);
        }
        assert_eq!(live.clock.nudge, MAX_NUDGE);
        assert_eq!(live.status().clock.nudge, MAX_NUDGE as f32);

        // "This is beat one": the nearest beat begins the bar.
        let mut live = Live::default();
        live.downbeat(0.0, heard(120.0, 22.1));
        assert_eq!(live.clocked(0.0, heard(120.0, 22.1)).bar_beat, 0);
        assert_eq!(live.clocked(0.0, heard(120.0, 25.0)).bar_beat, 3);
        assert_eq!(live.clocked(0.0, heard(120.0, 26.0)).bar_beat, 0);
        // Pressed just before a beat, it means that beat.
        live.downbeat(0.0, heard(120.0, 30.9));
        assert_eq!(live.clocked(0.0, heard(120.0, 31.0)).bar_beat, 0);
        assert_eq!(live.clocked(0.0, heard(120.0, 30.9)).bar_beat, 3);

        // Back to the music as it is heard. The bar stays where it was put.
        live.nudge(0.3);
        live.set_scale(0.0, raw, 2.0);
        live.follow_music();
        assert_eq!((live.clock.scale, live.clock.nudge), (1.0, 0.0));
    }

    fn live_bar(live: &Live, raw: AudioFeatures) -> u8 {
        live.clocked(0.0, raw).bar_beat
    }

    #[test]
    fn taps_along_with_the_music_correct_its_beat_and_leave_it_in_charge() {
        let mut live = Live::default();
        // The music is at 120; the analysis is 0.3 of a beat out and its
        // bars start on the wrong beat. Three taps, one a beat, the first
        // of them on beat one.
        let at = |seconds: f64| heard(120.0, 40.3 + seconds * 2.0);
        live.tap(0.0, at(0.0));
        live.tap(0.5, at(0.5));
        assert_eq!(
            live.source(0.5),
            BeatSource::Tapped,
            "two taps: their own clock for now"
        );
        live.tap(1.0, at(1.0));
        assert_eq!(
            live.source(1.0),
            BeatSource::Music,
            "three that fit: the music's, corrected"
        );
        assert_eq!(live.status().tapped_bpm, 0.0);
        let f = live.clocked(1.0, at(1.0));
        assert!(
            f.beat_phase < 0.01 || f.beat_phase > 0.99,
            "the tap is on the beat: {}",
            f.beat_phase
        );
        assert_eq!(f.bar_beat, 2, "the third tap is the third beat of the bar");
        // It follows the music from here: a bar later, still in step.
        let f = live.clocked(3.0, at(3.0));
        assert!(f.beat_phase < 0.01 || f.beat_phase > 0.99);
        assert_eq!((f.bar_beat, f.bpm), (2, 120.0));

        // Tapped at half the speed the analysis counts, it is half time.
        let mut live = Live::default();
        let at = |seconds: f64| heard(160.0, 8.0 + seconds * 160.0 / 60.0);
        for tap in 0..4 {
            let t = tap as f64 * 0.75;
            live.tap(t, at(t));
        }
        assert_eq!(live.source(3.0), BeatSource::Music);
        assert_eq!(live.clock.scale, 0.5);
        assert_eq!(live.clocked(3.0, at(3.0)).bpm, 80.0);

        // Tapped at a tempo that has nothing to do with the music's, or
        // with no music, the taps are the clock, as before.
        let mut live = Live::default();
        for tap in 0..4 {
            let t = tap as f64 * 0.6;
            live.tap(t, heard(133.0, t * 133.0 / 60.0));
        }
        assert_eq!(live.source(2.0), BeatSource::Tapped);
        assert!((live.status().tapped_bpm - 100.0).abs() < 0.5);
    }

    #[test]
    fn a_midi_clock_is_followed_only_when_asked_and_only_while_it_ticks() {
        let mut live = Live::default();
        // 24 ticks a beat at 125 beats a minute, after a start.
        let tick = 60.0 / 125.0 / 24.0;
        live.midi_clock.start();
        for n in 0..=24 * 6 {
            live.midi_tick(n as f64 * tick);
        }
        let now = 24.0 * 6.0 * tick;
        assert!((live.midi_clock.bpm(now).unwrap() - 125.0).abs() < 0.5);
        // Heard, but not followed: the music's beat stands.
        assert_eq!(live.source(now), BeatSource::Music);
        assert_eq!(live.clocked(now, heard(98.0, 3.5)).bpm, 98.0);
        // Followed: six beats since the start, so the third beat of the second bar.
        live.follow_midi = true;
        let f = live.clocked(now, heard(98.0, 3.5));
        assert!((f.bpm - 125.0).abs() < 0.5);
        assert_eq!((f.beat_count, f.bar_beat), (6, 2));
        // Between ticks the position runs on.
        let f = live.clocked(now + tick * 0.5, heard(98.0, 3.5));
        assert!((f.beat_phase - 0.5 / 24.0).abs() < 0.01);
        // The sender stops: half a second later the music is back in charge.
        assert_eq!(live.source(now + 0.6), BeatSource::Music);
        assert_eq!(live.clocked(now + 0.6, heard(98.0, 3.5)).bpm, 98.0);
        // Starting again begins a bar.
        live.midi_clock.start();
        live.midi_tick(now + 2.0);
        live.midi_tick(now + 2.0 + tick);
        let f = live.clocked(now + 2.0 + tick, heard(98.0, 3.5));
        assert_eq!((f.beat_count, f.bar_beat), (0, 0));
    }

    #[test]
    fn the_timelines_clock_plays_stops_jumps_and_follows_the_song() {
        let mut t = Transport::default();
        assert_eq!(t.position(50.0), 0.0, "stopped, it stays where it is");
        t.play(100.0, None);
        assert!((t.position(102.5) - 2.5).abs() < 1e-9);
        t.stop(103.0);
        assert!((t.position(900.0) - 3.0).abs() < 1e-9);
        t.play(200.0, Some(60.0));
        assert!((t.position(201.0) - 61.0).abs() < 1e-9);
        t.seek(201.0, 10.0);
        assert!(
            (t.position(202.0) - 11.0).abs() < 1e-9,
            "a jump while playing plays on from there"
        );
        // The window playing the song says where the song is. A hair's
        // difference is left alone; a real one is taken.
        assert!(!t.sync(202.0, 11.02));
        assert!((t.position(202.0) - 11.0).abs() < 1e-9);
        assert!(t.sync(202.0, 11.2));
        assert!((t.position(203.0) - 12.2).abs() < 1e-9);
        // Stopping ends a recording too, and a stopped clock ignores the song.
        t.recording = true;
        t.stop(203.0);
        assert!(!t.recording && !t.sync(204.0, 99.0));
    }

    fn timeline_show() -> (Show, [u32; 2], [u32; 2]) {
        use darkrgb_core::timeline::{Clip, ClipKind};
        let mut show = Show::default();
        show.layers[0].effect = "plasma".into();
        let verse = show.save_scene("Verse");
        show.layers[0].effect = "fire".into();
        let chorus = show.save_scene("Chorus");
        show.layers[0].effect = "solid".into();
        let layer = show.layers[0].id;
        // A pad that switches the layer when pressed, and one for blackout.
        let pad = show.bind("KeyQ", "", Action::Layer { id: layer }, KeyMode::Toggle);
        let dark = show.bind("KeyB", "", Action::Blackout, KeyMode::Hold);
        let clip = |id: u32, start: f32, length: f32, what: ClipKind| Clip {
            id,
            start,
            length,
            what,
        };
        show.timeline.length = 20.0;
        show.timeline.clips = vec![
            clip(901, 2.0, 0.0, ClipKind::Scene { id: verse }),
            clip(902, 10.0, 0.0, ClipKind::Scene { id: chorus }),
            clip(903, 4.0, 2.0, ClipKind::Pad { id: dark }),
            clip(904, 12.0, 1.0, ClipKind::Pad { id: pad }),
        ];
        (show, [verse, chorus], [pad, dark])
    }

    #[test]
    fn a_playing_timeline_decides_the_scene_and_holds_its_pads() {
        let (show, _, _) = timeline_show();
        let mut live = Live::default();
        let effect = |live: &Live| live.overlay(&show).layers[0].effect.clone();
        // Stopped, the show is itself.
        assert!(!live.timeline_step(0.0, &show).changed);
        assert_eq!(effect(&live), "solid");

        live.transport.play(100.0, None);
        assert!(
            !live.timeline_step(101.0, &show).changed,
            "nothing before the first clip"
        );
        assert_eq!(effect(&live), "solid");
        let serial = live.scene_serial;
        let step = live.timeline_step(102.0, &show);
        assert!(step.changed && step.entered.is_empty());
        assert_eq!(effect(&live), "plasma", "the verse");
        assert_eq!(live.scene_serial, serial + 1, "a change of scene, so it fades like one");
        assert!(live.timeline_has_the_scene());
        // The same moment again changes nothing.
        assert!(!live.timeline_step(102.5, &show).changed);

        // The blackout pad's clip: held from 4 to 6, without touching the show.
        let step = live.timeline_step(104.0, &show);
        assert_eq!(step.entered.len(), 1);
        assert!(live.overlay(&show).master.blackout);
        assert!(!show.master.blackout);
        assert_eq!(live.scene_serial, serial + 1, "a pad is not a change of scene");
        live.timeline_step(106.0, &show);
        assert!(!live.overlay(&show).master.blackout);

        live.timeline_step(110.0, &show);
        assert_eq!(effect(&live), "fire", "the chorus");
        // A pad that switches its effect when pressed has it switched for the clip.
        assert!(live.overlay(&show).layers[0].enabled);
        live.timeline_step(112.5, &show);
        assert!(!live.overlay(&show).layers[0].enabled);
        live.timeline_step(113.5, &show);
        assert!(live.overlay(&show).layers[0].enabled);

        // A jump back shows what was there then.
        live.transport.seek(114.0, 3.0);
        live.timeline_step(114.0, &show);
        assert_eq!(effect(&live), "plasma");

        // At the end it stops, and the show is itself again.
        let step = live.timeline_step(140.0, &show);
        assert!(step.ended && step.changed && !step.playing);
        assert_eq!(effect(&live), "solid");
        assert_eq!(live.status().timeline.position, 20.0);
        assert!(!live.status().timeline.playing);
    }

    #[test]
    fn a_timeline_set_to_repeat_starts_again_and_one_stopped_calls_for_nothing() {
        let (mut show, _, _) = timeline_show();
        show.timeline.repeat = true;
        let mut live = Live::default();
        live.transport.play(0.0, Some(19.0));
        live.timeline_step(0.5, &show);
        assert_eq!(live.overlay(&show).layers[0].effect, "fire");
        let step = live.timeline_step(1.5, &show);
        assert!(step.playing && !step.ended);
        assert!(live.status().timeline.position < 0.01, "back at the start");
        assert_eq!(
            live.overlay(&show).layers[0].effect,
            "solid",
            "before the first clip again"
        );
        live.timeline_step(4.0, &show);
        assert_eq!(live.overlay(&show).layers[0].effect, "plasma");
        // Stopped by hand in the middle of a held pad: everything is let go.
        live.transport.seek(4.0, 5.0);
        live.timeline_step(4.0, &show);
        assert!(live.overlay(&show).master.blackout);
        live.transport.stop(4.5);
        assert!(live.timeline_step(4.6, &show).changed);
        assert!(!live.overlay(&show).master.blackout);
        assert_eq!(live.overlay(&show).layers[0].effect, "solid");
    }

    #[test]
    fn outside_its_hours_a_show_is_dark_without_being_changed() {
        let mut show = Show::default();
        let mut live = Live::default();
        assert!(!live.schedule_step(3 * 60, &show), "no schedule, nothing happens");
        show.schedule.enabled = true;
        assert!(
            live.schedule_step(3 * 60, &show),
            "three at night is outside five to eleven"
        );
        assert!(live.asleep && live.status().asleep);
        assert!(live.overlay(&show).master.blackout);
        assert!(!show.master.blackout, "the show's own switch is as it was");
        assert!(!live.schedule_step(4 * 60, &show), "still outside: no change to report");
        assert!(live.schedule_step(18 * 60, &show));
        assert!(!live.overlay(&show).master.blackout);
        // Switching the schedule off wakes the lights whatever the time.
        live.schedule_step(3 * 60, &show);
        show.schedule.enabled = false;
        assert!(live.schedule_step(3 * 60, &show));
        assert!(!live.asleep);
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
        let with_tempo = |beat_count: u32| AudioFeatures {
            bpm: 120.0,
            beat_count,
            bar_beat: (beat_count % 4) as u8,
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
