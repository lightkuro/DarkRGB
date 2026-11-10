//! Prints how much each audio feature moves: the 10th, 50th and 90th
//! percentile over a few seconds. A feature that barely moves gives flat,
//! washed-out effects, so this is the number to watch when tuning.
//!
//! `cargo run --release -p ols-audio --example spread -- [seconds]`

use std::time::{Duration, Instant};

use ols_audio::{AudioCapture, SYSTEM_SOURCE};

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let capture = AudioCapture::start(SYSTEM_SOURCE, 1.0);
    // Let capture start and the automatic gain settle.
    std::thread::sleep(Duration::from_secs(4));
    let mut rows: [Vec<f32>; 4] = Default::default();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(10));
        let f = capture.features();
        for (row, value) in rows.iter_mut().zip([f.level, f.bass, f.mid, f.treble]) {
            row.push(value);
        }
    }
    for (name, mut row) in ["level", "bass", "mid", "treble"].into_iter().zip(rows) {
        row.sort_by(f32::total_cmp);
        let at = |p: f32| row[((row.len() - 1) as f32 * p) as usize];
        println!(
            "{name:<7} p10 {:.2}  p50 {:.2}  p90 {:.2}  swing {:.2}",
            at(0.1),
            at(0.5),
            at(0.9),
            at(0.9) - at(0.1)
        );
    }
}
