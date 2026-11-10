//! The frame loop: render, send to devices, publish the preview.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ols_core::{Renderer, Show};
use ols_output::Outputs;

use crate::app::Shared;

pub const FRAME_RATE: u32 = 60;
/// The preview is sent every this many frames (30 per second).
const PREVIEW_EVERY: u64 = 2;

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

pub fn run(shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    let mut outputs = match Outputs::new() {
        Ok(outputs) => outputs,
        Err(e) => {
            log::error!("could not open a network socket for device output: {e}");
            return;
        }
    };
    let mut renderer = Renderer::new();
    let mut revisions = shared.revision.subscribe();
    let mut show: Show = shared.show.lock().unwrap().clone();
    let mut needs_sync = true;
    // Counts layout changes, so a client can tell which positions a preview
    // frame belongs to.
    let mut layout_serial = 0u32;
    let mut last_positions: Vec<[f32; 2]> = Vec::new();
    let mut preview_bytes = Vec::new();

    let period = Duration::from_secs_f64(1.0 / FRAME_RATE as f64);
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

        if revisions.has_changed().unwrap_or(false) {
            revisions.mark_unchanged();
            show = shared.show.lock().unwrap().clone();
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
        let time = frame_start.duration_since(started).as_secs_f64();
        let audio = shared.audio.features();
        renderer.render(&show, time, dt, &audio);
        outputs.send(&show, &renderer);

        if frame_index % PREVIEW_EVERY == 0 && shared.preview.receiver_count() > 0 {
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
}
