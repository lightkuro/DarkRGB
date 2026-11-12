//! Keeps flashing within what is considered safe for people with
//! photosensitive epilepsy.
//!
//! The rule is the one web and broadcast guidance share: no more than
//! three flashes in any one second. What counts as a flash is the picture's
//! overall brightness rising by some share of full brightness after having
//! fallen by as much. Two shares are offered:
//!
//! - [`STRICT`], a tenth, is what the guidance for screens says. Almost
//!   any effect that follows fast music changes by that much several times
//!   a second, so this setting flattens most of them.
//! - [`STANDARD`], a quarter, catches what stage-lighting guidance is
//!   about (strobes, white flashes, blinders) and leaves effects that
//!   shimmer or pulse gently alone. It is the default.
//!
//! The limit is applied to the finished frame, so it covers every effect,
//! every key and the on-screen preview. A rise that would be one flash too
//! many is held just under the share, so it reads as a flicker instead of
//! a flash. As soon as a flash is allowed again the picture is let
//! through, so brightness that stays is only late, never lost.

use std::collections::VecDeque;

use crate::color::Rgb;

/// The share of full brightness that makes a flash under screen guidance.
pub const STRICT: f32 = 0.10;
/// The share used by default: hard flashes only.
pub const STANDARD: f32 = 0.25;
/// Pictures already this bright cannot flash in the sense that matters.
const SAFE_DARK: f32 = 0.80;
/// The time flashes are counted over, in seconds.
const WINDOW_S: f64 = 1.0;
/// How quickly the resting levels follow a slow change, in seconds. A fade
/// slower than this is not a flash.
const FOLLOW_S: f32 = 0.4;

#[derive(Debug, Default)]
pub struct FlashGuard {
    /// True after a counted rise, until the picture falls again.
    up: bool,
    /// The lowest level since the last fall, or the highest since the last
    /// rise: the level the next change is measured from.
    rest: f32,
    /// When the rises of the last second happened.
    rises: VecDeque<f64>,
    last: f64,
}

/// Overall brightness of a frame, 0..1, weighted the way the eye is: green
/// counts most, blue least. Squaring undoes the screen's curve near enough.
pub fn luminance(frame: &[Rgb]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame
        .iter()
        .map(|px| {
            let (r, g, b) = (px.r.clamp(0.0, 1.0), px.g.clamp(0.0, 1.0), px.b.clamp(0.0, 1.0));
            0.2126 * r * r + 0.7152 * g * g + 0.0722 * b * b
        })
        .sum();
    sum / frame.len() as f32
}

impl FlashGuard {
    /// Limits `frame` to `per_second` flashes, a flash being a change of
    /// `share` of full brightness there and back. Zero or less flashes a
    /// second switches the limit off. `now` is in seconds and must not go
    /// backwards. Returns true when the frame was held back.
    pub fn apply(&mut self, frame: &mut [Rgb], now: f64, per_second: f32, share: f32) -> bool {
        if per_second <= 0.0 || !per_second.is_finite() || frame.is_empty() {
            *self = Self::default();
            return false;
        }
        let threshold = if share.is_finite() {
            share.clamp(0.05, 0.9)
        } else {
            STANDARD
        };
        let limit = (per_second.floor() as usize).max(1);
        let dt = ((now - self.last) as f32).clamp(0.0, 0.25);
        self.last = now;
        while self.rises.front().is_some_and(|at| now - at >= WINDOW_S) {
            self.rises.pop_front();
        }

        let level = luminance(frame);
        let follow = (dt / FOLLOW_S).min(1.0);
        if self.up {
            // Waiting for the fall that completes the flash.
            if level <= self.rest - threshold {
                self.up = false;
                self.rest = level;
            } else if level > self.rest {
                self.rest = level;
            } else {
                self.rest += (level - self.rest) * follow;
            }
            return false;
        }

        if level < self.rest {
            self.rest = level;
            return false;
        }
        if level < self.rest + threshold || self.rest >= SAFE_DARK {
            // Not a flash: a small change, a slow one, or a bright picture.
            self.rest += (level - self.rest) * follow;
            return false;
        }

        // A rise big enough to be a flash. Allowed if the last second has
        // room, and not right on the heels of the one before, so that a
        // fast strobe comes out evenly spaced.
        let spaced = self
            .rises
            .back()
            .is_none_or(|last| now - last >= WINDOW_S / (limit + 1) as f64);
        if self.rises.len() < limit && spaced {
            self.rises.push_back(now);
            self.up = true;
            self.rest = level;
            return false;
        }

        // One flash too many: hold the picture just under a flash.
        let allowed = self.rest + threshold * 0.9;
        // Brightness goes with the square of the values, so scale by the root.
        let scale = (allowed / level).sqrt();
        for px in frame.iter_mut() {
            *px = px.scale(scale);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME_S: f64 = 1.0 / 60.0;
    /// These tests use the strict share unless they say otherwise.
    const THRESHOLD: f32 = STRICT;

    /// Runs frames of one grey level each through a guard and returns the
    /// brightness that came out of each.
    fn run(levels: impl Iterator<Item = f32>, per_second: f32) -> Vec<f32> {
        let mut guard = FlashGuard::default();
        levels
            .enumerate()
            .map(|(i, level)| {
                let mut frame = vec![Rgb::new(level, level, level); 8];
                guard.apply(&mut frame, i as f64 * FRAME_S, per_second, THRESHOLD);
                luminance(&frame)
            })
            .collect()
    }

    /// A square wave between black and white, `hz` times a second.
    fn strobe(hz: f64, seconds: f64) -> impl Iterator<Item = f32> {
        (0..(seconds / FRAME_S) as usize).map(move |i| {
            let phase = (i as f64 * FRAME_S * hz).fract();
            if phase < 0.5 { 1.0 } else { 0.0 }
        })
    }

    /// The most flashes in any one second: rises of a tenth or more after
    /// a fall of as much.
    fn worst_second(out: &[f32]) -> usize {
        let mut rises = Vec::new();
        let (mut up, mut rest) = (false, out[0]);
        for (i, y) in out.iter().enumerate() {
            if up {
                if *y <= rest - THRESHOLD {
                    up = false;
                    rest = *y;
                } else {
                    rest = rest.max(*y);
                }
            } else if *y >= rest + THRESHOLD {
                rises.push(i as f64 * FRAME_S);
                up = true;
                rest = *y;
            } else {
                rest = rest.min(*y);
            }
        }
        rises
            .iter()
            .map(|start| rises.iter().filter(|t| **t >= *start && **t < *start + 1.0).count())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn a_fast_strobe_is_cut_to_three_flashes_a_second() {
        let raw: Vec<f32> = strobe(10.0, 6.0).collect();
        assert!(worst_second(&raw) >= 10, "the test's own counting works");
        let out = run(raw.into_iter(), 3.0);
        assert!(worst_second(&out) <= 3, "{} flashes in a second", worst_second(&out));
        // It still flashes: the limit thins the strobe, it does not kill it.
        let full = out.iter().filter(|y| **y > 0.9).count();
        assert!(full > 30, "only {full} bright frames in six seconds");
        // And the ones let through are evenly spread, not three in a burst.
        let starts: Vec<usize> = (1..out.len()).filter(|i| out[*i] > 0.9 && out[i - 1] < 0.5).collect();
        let gaps: Vec<usize> = starts.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(
            gaps.iter().all(|g| *g >= 15),
            "flashes closer than a quarter second: {gaps:?}"
        );
    }

    #[test]
    fn a_higher_limit_lets_more_through() {
        let out = run(strobe(10.0, 6.0), 4.0);
        assert_eq!(worst_second(&out), 4);
    }

    #[test]
    fn slow_flashing_and_music_tempos_pass_untouched() {
        for hz in [1.0, 2.0, 2.9] {
            let raw: Vec<f32> = strobe(hz, 5.0).collect();
            let out = run(raw.iter().copied(), 3.0);
            assert_eq!(out, raw, "{hz} flashes a second was changed");
        }
    }

    #[test]
    fn a_steady_bright_picture_and_a_slow_fade_are_left_alone() {
        // `run` reports brightness, which goes with the square of the level.
        let changed = |levels: &[f32]| {
            let out = run(levels.iter().copied(), 3.0);
            out.iter()
                .zip(levels)
                .filter(|(o, l)| (*o - *l * *l).abs() > 1e-3)
                .count()
        };
        let steady: Vec<f32> = std::iter::repeat_n(0.9, 300).collect();
        assert_eq!(changed(&steady), 0);
        // Black to white over two seconds, again and again.
        let fade: Vec<f32> = (0..600).map(|i| ((i % 240) as f32 / 120.0).min(1.0)).collect();
        assert_eq!(changed(&fade), 0);
    }

    #[test]
    fn brightness_that_stays_is_late_but_not_lost() {
        // Three flashes inside a second, then the picture goes bright and stays.
        let levels = (0..165).map(|i| {
            let flash = [0, 18, 36].iter().any(|start| (*start..start + 3).contains(&i));
            if flash || i >= 45 { 1.0 } else { 0.0 }
        });
        let out = run(levels, 3.0);
        assert!(out[45] < 0.2, "the fourth rise in a second is held at first");
        let arrives = out[45..]
            .iter()
            .position(|y| *y > 0.99)
            .expect("the picture never came through");
        assert!(arrives <= 20, "it took {arrives} frames");
        assert!(out[45 + arrives..].iter().all(|y| *y > 0.99), "and it stays through");
    }

    #[test]
    fn flicker_on_top_of_a_lit_picture_counts_too() {
        // Between half and full brightness, ten times a second.
        let levels = (0..360).map(|i| if (i / 3) % 2 == 0 { 1.0 } else { 0.7 });
        let out = run(levels, 3.0);
        assert!(worst_second(&out) <= 3);
    }

    #[test]
    fn small_changes_and_a_few_pixels_flashing_are_not_flashes() {
        // A strobe on one pixel in sixteen moves the whole picture by 6%.
        let mut guard = FlashGuard::default();
        for i in 0..120 {
            let lit = (i / 3) % 2 == 0;
            let mut frame = vec![Rgb::BLACK; 16];
            frame[0] = if lit { Rgb::WHITE } else { Rgb::BLACK };
            let before = frame.clone();
            assert!(!guard.apply(&mut frame, i as f64 * FRAME_S, 3.0, THRESHOLD));
            assert_eq!(frame, before);
        }
    }

    #[test]
    fn the_standard_share_leaves_shimmer_alone_and_still_stops_a_strobe() {
        // The picture flickers by 15% of full brightness eight times a
        // second, as sparkles on hi-hats do.
        let levels = |i: usize| if (i / 4).is_multiple_of(2) { 0.5f32 } else { 0.32 };
        let run_at = |share: f32, level: &dyn Fn(usize) -> f32| -> (Vec<f32>, usize) {
            let mut guard = FlashGuard::default();
            let mut held = 0;
            let out = (0..360)
                .map(|i| {
                    let mut frame = vec![Rgb::new(level(i), level(i), level(i)); 8];
                    held += usize::from(guard.apply(&mut frame, i as f64 * FRAME_S, 3.0, share));
                    luminance(&frame)
                })
                .collect();
            (out, held)
        };
        let swing = 0.5f32 * 0.5 - 0.32 * 0.32;
        assert!(
            swing > STRICT && swing < STANDARD,
            "the test's flicker sits between the two: {swing}"
        );
        assert_eq!(run_at(STANDARD, &levels).1, 0, "shimmer passes at the standard share");
        assert!(run_at(STRICT, &levels).1 > 50, "and is held at the strict one");
        // A strobe is stopped either way.
        let strobe = |i: usize| if (i / 3).is_multiple_of(2) { 1.0f32 } else { 0.0 };
        let (out, held) = run_at(STANDARD, &strobe);
        assert!(held > 50);
        let rises = (1..out.len()).filter(|i| out[*i] > 0.9 && out[i - 1] < 0.5).count();
        assert!(rises <= 18 + 1, "{rises} full flashes in six seconds");
        // A nonsense share falls back to something sane instead of switching the limit off.
        let (_, held) = run_at(f32::NAN, &strobe);
        assert!(held > 50);
    }

    #[test]
    fn off_means_off() {
        let raw: Vec<f32> = strobe(10.0, 2.0).collect();
        assert_eq!(run(raw.iter().copied(), 0.0), raw);
        let mut guard = FlashGuard::default();
        assert!(!guard.apply(&mut [], 0.0, 3.0, THRESHOLD), "an empty frame is fine");
    }

    #[test]
    fn colour_is_kept_when_a_flash_is_held() {
        let mut guard = FlashGuard::default();
        let mut last = Rgb::BLACK;
        for i in 0..60 {
            let lit = (i / 3) % 2 == 0;
            let mut frame = vec![if lit { Rgb::new(1.0, 0.5, 0.0) } else { Rgb::BLACK }; 4];
            if guard.apply(&mut frame, i as f64 * FRAME_S, 3.0, THRESHOLD) {
                last = frame[0];
            }
        }
        assert!(last.r > 0.0 && last.b == 0.0);
        assert!(
            (last.g / last.r - 0.5).abs() < 1e-4,
            "held flashes keep their hue: {last:?}"
        );
    }
}
