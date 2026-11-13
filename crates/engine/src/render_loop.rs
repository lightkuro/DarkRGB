//! The frame loop: render, send to devices, publish the preview.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use darkrgb_core::safety::FlashGuard;
use darkrgb_core::{AudioFeatures, Renderer, Show};
use darkrgb_output::Outputs;

use crate::app::Shared;

/// The preview is sent about this many times a second, whatever the
/// frame rate.
const PREVIEW_RATE: u32 = 30;

/// The scene being left while the next one fades in. It keeps playing,
/// with its own renderer, until the fade is over.
struct Fade {
    show: Show,
    from: Renderer,
    started: Instant,
    seconds: f32,
}

/// What the audio analysis said a while ago, for speakers that play late.
#[derive(Default)]
struct Delay {
    history: VecDeque<(f64, AudioFeatures)>,
}

impl Delay {
    /// Takes the newest features and returns the ones from `delay_ms` ago.
    fn step(&mut self, now: f64, newest: AudioFeatures, delay_ms: f32) -> AudioFeatures {
        if delay_ms <= 0.0 {
            self.history.clear();
            return newest;
        }
        self.history.push_back((now, newest));
        let due = now - delay_ms as f64 / 1000.0;
        // Keep the newest entry that is old enough at the front.
        while self.history.len() > 1 && self.history[1].0 <= due {
            self.history.pop_front();
        }
        self.history.front().map_or(newest, |(_, features)| *features)
    }
}

/// First byte of a binary WebSocket message.
pub const MSG_PREVIEW: u8 = 1;
pub const MSG_LAYOUT: u8 = 2;

/// Builds a binary message: kind, three bytes of padding so the payload is
/// 4-byte aligned, the layout serial, then the payload.
fn message(kind: u8, layout_serial: u32, payload_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload_len);
    out.extend_from_slice(&[kind, 0, 0, 0]);
    out.extend_from_slice(&layout_serial.to_le_bytes());
    out
}

/// Runs the frame loop until told to stop, and starts it again if it
/// ever fails. A show left running for an evening, or for a month in a
/// window, must not go dark for good because of one bad frame.
pub fn run(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    let mut restarts = 0u32;
    keep_running(
        &stop,
        || frames(&shared, &stop),
        |what| {
            restarts += 1;
            log::error!("the frame loop failed ({what}); starting it again (restart {restarts})");
            shared.mend_locks();
            shared.stats.lock().unwrap().restarts = restarts;
            *shared.notice.lock().unwrap() = Some(
                "The lights stopped for a moment because of a fault in darkRGB, and were started again. \
                 If it keeps happening, the diagnostics file in Settings says what went wrong."
                    .into(),
            );
        },
    );
}

/// The shortest and longest waits before starting again.
const RESTART_WAIT: Duration = Duration::from_millis(200);
const RESTART_WAIT_MAX: Duration = Duration::from_secs(5);

/// Calls `body` until it returns, calling it again each time it panics.
/// `failed` is told what went wrong. Failures in quick succession are
/// spaced further and further apart, so a fault that happens on every
/// frame does not spin the processor.
fn keep_running(stop: &AtomicBool, mut body: impl FnMut(), mut failed: impl FnMut(&str)) {
    let mut wait = RESTART_WAIT;
    loop {
        let began = Instant::now();
        let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut body)) else {
            return;
        };
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let what = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("no reason given");
        failed(what);
        // A run that lasted a while was a rare fault; one that did not
        // is likely to fail again at once.
        wait = if began.elapsed() > Duration::from_secs(30) {
            RESTART_WAIT
        } else {
            (wait * 2).min(RESTART_WAIT_MAX)
        };
        let until = Instant::now() + wait;
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn frames(shared: &Arc<Shared>, stop: &AtomicBool) {
    let mut outputs = match Outputs::new() {
        Ok(outputs) => outputs,
        Err(e) => {
            log::error!("could not open a network socket for device output: {e}");
            return;
        }
    };
    let mut renderer = Renderer::new();
    let mut revisions = shared.revision.subscribe();
    let mut live_revisions = shared.live_rev.subscribe();
    // The show as saved, and as the lights see it: with held keys applied.
    let mut base: Show = shared.show.lock().unwrap().clone();
    let (mut show, mut scene_serial) = {
        let live = shared.live.lock().unwrap();
        (live.overlay(&base), live.scene_serial)
    };
    let mut fade: Option<Fade> = None;
    let mut delay = Delay::default();
    let mut guard = FlashGuard::default();
    let mut window_limited = false;
    let mut last_beat = 0u32;
    let mut last_now = 0.0f64;
    // The clock the effects run on. It can run fast or slow, and stops
    // while the picture is frozen.
    let mut show_time = 0.0f64;
    let mut frozen_audio: Option<AudioFeatures> = None;
    let mut needs_sync = true;
    // Counts layout changes, so a client can tell which positions a preview
    // frame belongs to.
    let mut layout_serial = 0u32;
    let mut last_positions: Vec<[f32; 2]> = Vec::new();
    let mut preview_bytes = Vec::new();

    let period_of = |show: &Show| Duration::from_secs_f64(1.0 / show.output.frame_rate.clamp(1, 240) as f64);
    let mut period = period_of(&show);
    let sleeper = spin_sleep::SpinSleeper::default();
    let started = Instant::now();
    let mut deadline = started + period;
    let mut last_frame = started;
    let mut frame_index = 0u64;

    let mut window_start = started;
    let mut window_frames = 0u32;
    let mut window_busy = Duration::ZERO;
    let mut late_frames = 0u64;

    while !stop.load(Ordering::Relaxed) {
        let frame_start = Instant::now();
        // Knobs turned on a MIDI controller since the last frame.
        shared.apply_knobs();

        let show_changed = revisions.has_changed().unwrap_or(false);
        let live_changed = live_revisions.has_changed().unwrap_or(false);
        if show_changed {
            revisions.mark_unchanged();
            base = shared.show.lock().unwrap().clone();
        }
        if show_changed || live_changed {
            live_revisions.mark_unchanged();
            let (next, serial, fade_s) = {
                let mut live = shared.live.lock().unwrap();
                (live.overlay(&base), live.scene_serial, live.take_fade(base.live.fade))
            };
            let previous = std::mem::replace(&mut show, next);
            // A change of scene fades in, if the show asks for that. The
            // scene being left keeps its renderer and plays on underneath;
            // the new one starts fresh, which the fade hides.
            if serial != scene_serial {
                scene_serial = serial;
                if fade_s > 0.0 && show.fixtures == previous.fixtures {
                    fade = Some(Fade {
                        show: previous,
                        from: std::mem::replace(&mut renderer, Renderer::new()),
                        started: frame_start,
                        seconds: fade_s,
                    });
                }
            }
            period = period_of(&show);
            needs_sync = true;
        }
        if needs_sync {
            needs_sync = false;
            renderer.sync(&show);
            outputs.sync(&show);
            if renderer.positions() != last_positions.as_slice() || layout_serial == 0 {
                last_positions = renderer.positions().to_vec();
                layout_serial = layout_serial.wrapping_add(1).max(1);
                let mut msg = message(MSG_LAYOUT, layout_serial, last_positions.len() * 8);
                for [x, y] in &last_positions {
                    msg.extend_from_slice(&x.to_le_bytes());
                    msg.extend_from_slice(&y.to_le_bytes());
                }
                shared.layout.send_replace(msg.into());
            }
        }

        let dt = frame_start.duration_since(last_frame).as_secs_f32().min(0.25);
        last_frame = frame_start;
        let now = shared.now();
        let heard = delay.step(now, shared.audio.features(), show.audio.delay_ms);
        let (audio, launch, cues, frozen, idle_changed, timeline) = {
            let mut live = shared.live.lock().unwrap();
            // Two things about the beat that the show decides.
            live.lead_ms = base.audio.lead_ms;
            live.follow_midi = base.live.midi_clock;
            let clocked = live.clocked(now, heard);
            // Where the timeline has got to, and what it calls for now.
            let timeline = live.timeline_step(now, &base);
            // The music stopping, or starting again.
            let mut idle_changed = live.idle_step(now, &base, &clocked);
            // The show's hours, looked at once a second.
            if frame_index.is_multiple_of(60) {
                use chrono::Timelike;
                let clock = chrono::Local::now();
                idle_changed |= live.schedule_step((clock.hour() * 60 + clock.minute()) as u16, &base);
            }
            let beat = clocked.beat_count != last_beat;
            last_beat = clocked.beat_count;
            // A scene the user asked for comes first; autopilot only has
            // its say when nothing is waiting.
            let asked = live
                .should_launch(now, last_now, show.live.launch, &clocked, beat)
                .then(|| live.pending.map(|p| p.scene))
                .flatten();
            let launch = match (asked, live.pending) {
                (Some(scene), _) => Some(scene),
                (None, Some(_)) => None,
                // Nothing changes by itself while waiting for the music,
                // or while a timeline is saying which scene to show.
                (None, None) if live.idle || live.timeline_has_the_scene() => None,
                (None, None) => live.autopilot(now, &base, &clocked, beat),
            };
            (
                live.with_hits(now, clocked),
                launch,
                live.cues(now),
                live.is_frozen(&base),
                idle_changed,
                timeline,
            )
        };
        last_now = now;
        // Pads on the timeline that set something off do so as their clips begin.
        if !timeline.entered.is_empty() {
            shared.timeline_fire(&timeline.entered);
        }
        // Clients are told where a playing timeline is once a second;
        // between times they count on from what they were told.
        let playing_tick = frame_index.is_multiple_of(show.output.frame_rate.max(1) as u64) && timeline.playing;
        if idle_changed || timeline.changed || playing_tick {
            // Seen by this loop on its next frame, and by the clients.
            shared.bump_live();
        }
        if let Some(scene) = launch {
            // Seen by this loop on its next frame, like any other change.
            shared.launch_scene(scene);
        }

        // Frozen: time stands still and the sound is what it was, so the
        // effects draw the same picture. Brightness and blackout still work.
        let (audio, dt) = if frozen {
            (*frozen_audio.get_or_insert(audio), 0.0)
        } else {
            frozen_audio = None;
            (audio, dt * show.live.speed.clamp(0.1, 8.0))
        };
        show_time += dt as f64;
        let time = show_time;

        renderer.render_with(&show, time, dt, &audio, &cues);
        if let Some(leaving) = &mut fade {
            let t = frame_start.duration_since(leaving.started).as_secs_f32() / leaving.seconds;
            if t >= 1.0 {
                fade = None;
            } else {
                leaving.from.render_with(&leaving.show, time, dt, &audio, &cues);
                // Ease in and out, so the change has no hard edge.
                let eased = t * t * (3.0 - 2.0 * t);
                renderer.mix_in(leaving.from.frame(), 1.0 - eased);
            }
        }
        // Last of all, so nothing can get around it: the flash limit.
        window_limited |= guard.apply(renderer.frame_mut(), now, show.flash.limit, show.flash.share());
        outputs.send(&show, &renderer);

        let preview_every = (show.output.frame_rate / PREVIEW_RATE).max(1) as u64;
        if frame_index.is_multiple_of(preview_every) && shared.preview.receiver_count() > 0 {
            renderer.preview_bytes(&mut preview_bytes);
            let mut msg = message(MSG_PREVIEW, layout_serial, preview_bytes.len());
            msg.extend_from_slice(&preview_bytes);
            shared.preview.send_replace(msg.into());
        }
        frame_index += 1;

        window_frames += 1;
        window_busy += frame_start.elapsed();
        let window = frame_start.duration_since(window_start);
        if window >= Duration::from_secs(1) {
            let mut stats = shared.stats.lock().unwrap();
            stats.fps = window_frames as f32 / window.as_secs_f32();
            stats.frame_ms = window_busy.as_secs_f32() * 1000.0 / window_frames as f32;
            stats.late_frames = late_frames;
            stats.flash_limited = std::mem::take(&mut window_limited);
            stats.devices = outputs.stats().clone();
            window_start = frame_start;
            window_frames = 0;
            window_busy = Duration::ZERO;
        }

        let now = Instant::now();
        if now > deadline {
            late_frames += 1;
            // After a long stall, start a fresh schedule instead of
            // rushing frames out to catch up.
            if now > deadline + period * 4 {
                deadline = now;
            }
        } else {
            sleeper.sleep_until(deadline);
        }
        deadline += period;
    }
    // Stopping: leave the lights dark, not holding the last frame.
    outputs.go_dark(&show);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loop_that_fails_is_started_again_until_it_is_told_to_stop() {
        let stop = AtomicBool::new(false);
        let (mut runs, mut reasons) = (0, Vec::new());
        // Fails twice, each in its own way, then runs to its end.
        keep_running(
            &stop,
            || {
                runs += 1;
                match runs {
                    1 => panic!("first fault"),
                    2 => panic!("{} fault", "second"),
                    _ => {}
                }
            },
            |what| reasons.push(what.to_string()),
        );
        assert_eq!(runs, 3);
        assert_eq!(reasons, ["first fault", "second fault"]);
        // A failure while stopping is the end, not a reason to start again.
        let mut runs = 0;
        keep_running(
            &stop,
            || {
                runs += 1;
                stop.store(true, Ordering::Relaxed);
                panic!("on the way out");
            },
            |_| unreachable!(),
        );
        assert_eq!(runs, 1);
    }

    #[test]
    fn delayed_sound_is_what_was_heard_that_long_ago() {
        let mut delay = Delay::default();
        let at = |level: f32| AudioFeatures {
            level,
            ..AudioFeatures::default()
        };
        // One frame every 10 ms, each with its own level.
        let mut out = Vec::new();
        for frame in 0..30 {
            let now = frame as f64 * 0.01;
            out.push(delay.step(now, at(frame as f32), 95.0).level);
        }
        assert_eq!(out[0], 0.0, "until enough time has passed, the oldest is used");
        assert_eq!(out[10], 0.0);
        assert_eq!(out[15], 5.0);
        assert_eq!(out[29], 19.0);
        assert!(delay.history.len() <= 12, "old entries are dropped");

        // Switching the delay off returns to the present at once.
        assert_eq!(delay.step(0.31, at(99.0), 0.0).level, 99.0);
        assert!(delay.history.is_empty());
    }
}
