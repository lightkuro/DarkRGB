//! Rough upper bound on capture latency.
//!
//! Plays a file that is one second of silence followed by a burst, and
//! reports how long after the burst should have started it was detected.
//! The figure includes the player's own start-up and output delay, so the
//! real capture latency is lower.
//!
//! `cargo run -p ols-audio --example latency -- <player command> <file>`
//! for example `-- paplay burst.wav`

use std::process::Command;
use std::time::{Duration, Instant};

use ols_audio::{AudioCapture, SYSTEM_SOURCE};

fn main() {
    let mut args = std::env::args().skip(1);
    let player = args.next().expect("player command");
    let player_args: Vec<String> = args.collect();
    let capture = AudioCapture::start(SYSTEM_SOURCE, 1.0);
    // Let the stream settle and any earlier sound fade.
    std::thread::sleep(Duration::from_secs(4));
    for run in 1..=5 {
        let before = capture.features().level;
        let started = Instant::now();
        let mut child = Command::new(&player).args(&player_args).spawn().expect("player");
        let mut heard = None;
        while started.elapsed() < Duration::from_secs(3) {
            if capture.features().level > 0.3 {
                heard = Some(started.elapsed());
                break;
            }
            std::thread::sleep(Duration::from_micros(500));
        }
        let _ = child.wait();
        match heard {
            Some(at) => println!(
                "run {run}: burst detected {:.0} ms after it was due (level before {before:.2})",
                (at.as_secs_f64() - 1.0) * 1000.0
            ),
            None => println!("run {run}: burst not detected"),
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
