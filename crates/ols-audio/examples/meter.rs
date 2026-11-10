//! Prints a live level meter from system audio (or a chosen input).
//!
//! `cargo run -p ols-audio --example meter -- [source] [seconds]`

use std::time::{Duration, Instant};

use ols_audio::{AudioCapture, SYSTEM_SOURCE, list_inputs};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut args = std::env::args().skip(1);
    let source = args.next().unwrap_or_else(|| SYSTEM_SOURCE.to_string());
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);

    println!("Inputs:");
    for input in list_inputs() {
        println!("  {}  ({})", input.name, input.id);
    }

    let capture = AudioCapture::start(&source, 1.0);
    let started = Instant::now();
    let mut last_status = None;
    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(250));
        let status = capture.status();
        if last_status.as_ref() != Some(&status) {
            println!("status: {status:?}");
            last_status = Some(status);
        }
        let f = capture.features();
        if let Some(status) = &last_status
            && status.sample_rate > 0
        {
            let expected = started.elapsed().as_secs_f64() * status.sample_rate as f64;
            print!("got {:3.0}% ", capture.samples_received() as f64 / expected * 100.0);
        }
        let bar = "#".repeat((f.level * 30.0) as usize);
        println!(
            "level {:4.2} bass {:4.2} mid {:4.2} treble {:4.2} bpm {:5.1} beats {:3} active {:5} |{bar}",
            f.level, f.bass, f.mid, f.treble, f.bpm, f.beat_count, f.active
        );
    }
}
