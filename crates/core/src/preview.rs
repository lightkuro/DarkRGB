//! Small moving pictures of each effect, for the list effects are chosen
//! from. An effect is run on a little matrix against made-up music with a
//! plain four-beat bar, and the frames are kept. Nothing here touches
//! the show that is playing.

use crate::audio::{AudioFeatures, BANDS};
use crate::layout::Shape;
use crate::render::Renderer;
use crate::show::Show;

/// The side of the square of lights a preview is drawn on. Enough that
/// effects made of many small things (the bulbs of a marquee) show them.
pub const SIDE: u32 = 32;
/// Frames in a preview, and how many are shown a second. Two seconds:
/// one bar at the tempo below, so a preview that keeps time loops cleanly.
pub const FRAMES: u32 = 24;
pub const RATE: u32 = 12;
/// The tempo of the made-up music.
pub const BPM: f32 = 120.0;

/// Bytes before the frames: side, side, frames, frames a second.
pub const HEADER: usize = 4;

/// What the made-up music sounds like at `t` seconds: a kick on every
/// beat, a snare on the second and fourth, and hats between.
pub fn music(t: f64) -> AudioFeatures {
    let beats = t * (BPM as f64 / 60.0);
    let phase = beats.fract() as f32;
    let count = beats.floor() as u32;
    let kick = (-phase * 7.0).exp();
    let snare = if count % 2 == 1 { (-phase * 9.0).exp() } else { 0.0 };
    let hat = (-((phase * 2.0).fract()) * 10.0).exp();
    let mut bands = [0.0f32; BANDS];
    for (i, band) in bands.iter_mut().enumerate() {
        let place = i as f32 / (BANDS - 1) as f32;
        let low = (1.0 - place * 3.5).max(0.0) * kick;
        let middle = (1.0 - (place - 0.45).abs() * 4.0).max(0.0) * (0.25 + 0.75 * snare);
        let high = ((place - 0.6) * 2.5).clamp(0.0, 1.0) * hat * 0.8;
        // A little life of its own in each band, so bars are not a block.
        let wobble = 0.85 + 0.15 * ((t as f32) * (1.3 + place * 4.0) + place * 9.0).sin();
        *band = ((low + middle + high) * wobble).clamp(0.0, 1.0);
    }
    AudioFeatures {
        level: (0.35 + 0.65 * kick.max(snare * 0.8)).min(1.0),
        bass: kick,
        mid: 0.25 + 0.75 * snare,
        treble: hat * 0.8,
        bands,
        onset: kick.max(snare),
        onsets: [kick, snare, hat],
        beat: kick,
        beat_phase: phase,
        bpm: BPM,
        beat_count: count,
        active: true,
        intensity: 0.9,
        section: 0,
        drop: 0.0,
        bar_beat: (count % 4) as u8,
        coasting: false,
    }
}

/// The moving picture of one effect with its usual settings: a header
/// (see [`HEADER`]) and then each frame as rows of red, green, blue.
/// `None` for an effect that does not exist.
pub fn effect(id: &str) -> Option<Vec<u8>> {
    crate::effects::find(id)?;
    let mut show = Show::default();
    show.layers.clear();
    show.add_layer(id);
    show.add_virtual(
        "preview",
        &Shape::Matrix {
            cols: SIDE,
            rows: SIDE,
            serpentine: false,
        },
        false,
    );
    // Nothing is held back in a preview: it shows what the effect does.
    show.flash.limit = 0.0;
    show.audio.dynamics = 0.0;
    let mut renderer = Renderer::new();
    renderer.sync(&show);
    // The steps between kept frames, and two bars run first so that
    // effects with a history (fire, trails) have got going.
    const STEPS: u32 = 5;
    let dt = 1.0 / (RATE * STEPS) as f32;
    let lead_in = FRAMES * STEPS * 2;
    let mut out = Vec::with_capacity(HEADER + (FRAMES * SIDE * SIDE * 3) as usize);
    out.extend_from_slice(&[SIDE as u8, SIDE as u8, FRAMES as u8, RATE as u8]);
    let mut frame = Vec::new();
    for step in 0..lead_in + FRAMES * STEPS {
        let t = step as f64 * dt as f64;
        renderer.render(&show, t, dt, &music(t));
        if step >= lead_in && (step - lead_in).is_multiple_of(STEPS) {
            renderer.preview_bytes(&mut frame);
            out.extend_from_slice(&frame);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_has_a_preview_of_the_right_size_that_shows_something() {
        let size = HEADER + (FRAMES * SIDE * SIDE * 3) as usize;
        for def in crate::effects::registry() {
            let id = def.info.id;
            let preview = effect(id).unwrap_or_else(|| panic!("{id} has no preview"));
            assert_eq!(preview.len(), size, "{id}");
            assert_eq!(preview[..HEADER], [32, 32, 24, 12]);
            let lit = preview[HEADER..].iter().filter(|b| **b > 40).count();
            assert!(lit > 0, "{id} shows nothing in its preview");
        }
        assert!(effect("no such effect").is_none());
    }

    #[test]
    fn the_made_up_music_has_a_beat() {
        let on = music(0.0);
        let between = music(0.4);
        assert!(on.beat > 0.9 && between.beat < 0.1);
        assert_eq!((music(0.6).beat_count, music(2.1).bar_beat), (1, 0));
        assert!(music(0.5).mid > music(0.0).mid, "the snare is on the second beat");
    }
}
