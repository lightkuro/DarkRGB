//! Runs the analysis over songs and says how well it kept time.
//!
//! Usage: `cargo run --release -p darkrgb-audio --example score -- LIST`
//! where LIST is a file of lines `path<TAB>bpm<TAB>name`, each path a WAV
//! file of 16-bit PCM (mono or stereo, any rate), and bpm the song's
//! known tempo. Songs are fed in blocks of 10 ms, as capture delivers
//! them, and the analysis is looked at a hundred times a second.
//!
//! For each song it prints:
//! - lock: seconds until the tempo was first right, or "never";
//! - acc1: share of the time after the first 10 s that the tempo was
//!   within 4% of the known one;
//! - acc2: the same, also counting half, double, a third and three times
//!   (the beat counted at another level);
//! - the tempo it settled on most, the beats it counted a second against
//!   the known rate, hits a second, sections, and how much of the time
//!   the beat was being kept from memory;
//! - how even the bass, mids, highs and level were (their average and
//!   how much of the time each sat near full), to see if levelling
//!   flattens the music.

use std::collections::BTreeMap;
use std::fs;

use darkrgb_audio::Analyzer;

fn read_wav(path: &str) -> Result<(u32, Vec<f32>), String> {
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{path}: not a WAV file"));
    }
    let (mut at, mut channels, mut rate, mut bits) = (12usize, 1u16, 44_100u32, 16u16);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        if id == b"fmt " {
            channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            if bits != 16 {
                return Err(format!("{path}: only 16-bit PCM is read"));
            }
            let frames = body.len() / (2 * channels as usize);
            let mut out = Vec::with_capacity(frames);
            for f in 0..frames {
                let mut sum = 0.0f32;
                for c in 0..channels as usize {
                    let i = (f * channels as usize + c) * 2;
                    sum += i16::from_le_bytes([body[i], body[i + 1]]) as f32 / 32768.0;
                }
                out.push(sum / channels as f32);
            }
            return Ok((rate, out));
        }
        at += 8 + size + (size & 1);
    }
    Err(format!("{path}: no sound in it"))
}

/// Whether `heard` is the known tempo, or one at another level of the beat.
fn right(heard: f32, known: f32, levels: &[f32]) -> bool {
    heard > 0.0 && levels.iter().any(|k| (heard / (known * k) - 1.0).abs() <= 0.04)
}

fn main() {
    let list = std::env::args().nth(1).expect("a list of songs");
    let text = fs::read_to_string(&list).expect("the list");
    let (mut sum1, mut sum2, mut count) = (0.0, 0.0, 0);
    let mut sum_tight = 0.0f32;
    let (mut sum_f, mut with_ref) = (0.0f32, 0u32);
    println!(
        "{:<30} {:>5} {:>6} {:>5} {:>5} {:>6} {:>6} {:>5} {:>4} {:>5}  kicks at/tight  bass/mid/high/level (mean, % near full)",
        "song", "bpm", "lock", "acc1", "acc2", "mostly", "beats", "hits", "secs", "coast",
    );
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        let (path, known, name) = (
            parts[0],
            parts[1].parse::<f32>().unwrap(),
            parts.get(2).copied().unwrap_or(parts[0]),
        );
        let (rate, samples) = match read_wav(path) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };
        // Reference beat times, if there are any beside the song.
        let reference: Vec<f32> = fs::read_to_string(path.trim_end_matches(".wav").to_string() + ".beats")
            .map(|t| t.lines().filter_map(|l| l.trim().parse().ok()).collect())
            .unwrap_or_default();
        let mut ours: Vec<f32> = Vec::new();
        let mut analyzer = Analyzer::new(rate);
        let block = (rate / 100) as usize;
        let (mut lock, mut ok1, mut ok2, mut after) = (None, 0u32, 0u32, 0u32);
        let mut tempos: BTreeMap<i32, u32> = BTreeMap::new();
        let (mut beats, mut last_beat, mut hits, mut was_hit) = (0u32, 0u32, 0u32, false);
        let (mut coasting, mut first_section, mut last_section) = (0u32, None, 0u32);
        let mut sums = [0.0f32; 4];
        let mut full = [0u32; 4];
        let mut looks = 0u32;
        // Where in the beat each bass hit falls, as a point on a circle:
        // all near nought means the beat sits on the kicks.
        let (mut kick_x, mut kick_y, mut kicks, mut was_kick) = (0.0f32, 0.0f32, 0u32, false);
        for (i, chunk) in samples.chunks(block).enumerate() {
            analyzer.push(chunk);
            let f = analyzer.features();
            let t = i as f32 / 100.0;
            if lock.is_none() && right(f.bpm, known, &[1.0]) {
                lock = Some(t);
            }
            if t >= 10.0 {
                after += 1;
                ok1 += u32::from(right(f.bpm, known, &[1.0]));
                ok2 += u32::from(right(f.bpm, known, &[1.0, 0.5, 2.0, 1.0 / 3.0, 3.0]));
                if f.bpm > 0.0 {
                    *tempos.entry(f.bpm.round() as i32).or_default() += 1;
                }
                if f.beat_count != last_beat {
                    beats += 1;
                    // The beat was due at the end of this block.
                    ours.push(((i + 1) * block) as f32 / rate as f32);
                }
                coasting += u32::from(f.coasting);
                for (k, v) in [f.bass, f.mid, f.treble, f.level].iter().enumerate() {
                    sums[k] += v;
                    full[k] += u32::from(*v > 0.9);
                }
                looks += 1;
            }
            last_beat = f.beat_count;
            let kick = f.onsets[0] > 0.99;
            if kick && !was_kick && t >= 10.0 && f.bpm > 0.0 {
                let angle = f.beat_phase * std::f32::consts::TAU;
                kick_x += angle.cos();
                kick_y += angle.sin();
                kicks += 1;
            }
            was_kick = kick;
            let hit = f.onset > 0.99;
            hits += u32::from(hit && !was_hit);
            was_hit = hit;
            first_section.get_or_insert(f.section);
            last_section = f.section;
        }
        let seconds = samples.len() as f32 / rate as f32;
        let after_s = (after as f32 / 100.0).max(1.0);
        let mostly = tempos.iter().max_by_key(|(_, n)| **n).map_or(0, |(b, _)| *b);
        let acc1 = ok1 as f32 / after.max(1) as f32;
        let acc2 = ok2 as f32 / after.max(1) as f32;
        sum1 += acc1;
        sum2 += acc2;
        count += 1;
        let looks = looks.max(1) as f32;
        // The kicks' mean place in the beat (-0.5..0.5) and how tightly
        // they gather there (0 scattered, 1 all at one place).
        let n = kicks.max(1) as f32;
        let (kx, ky) = (kick_x / n, kick_y / n);
        let kick_at = ky.atan2(kx) / std::f32::consts::TAU;
        let kick_tight = (kx * kx + ky * ky).sqrt();
        sum_tight += kick_tight;
        // F-measure of our beats against the reference within 70 ms, the
        // mean offset of those that match (positive: ours late), and the
        // same against the off-beats, half way between reference beats.
        let refs: Vec<f32> = reference.iter().copied().filter(|t| *t >= 10.0).collect();
        let offbeats: Vec<f32> = reference
            .windows(2)
            .map(|w| (w[0] + w[1]) / 2.0)
            .filter(|t| *t >= 10.0)
            .collect();
        let fmeasure = |refs: &[f32]| -> (f32, f32) {
            if refs.is_empty() || ours.is_empty() {
                return (f32::NAN, f32::NAN);
            }
            let mut used = vec![false; refs.len()];
            let (mut hits, mut offset) = (0u32, 0.0f32);
            for t in &ours {
                if let Some((j, d)) = refs
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| !used[*j])
                    .map(|(j, r)| (j, t - r))
                    .filter(|(_, d)| d.abs() <= 0.07)
                    .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                {
                    used[j] = true;
                    hits += 1;
                    offset += d;
                }
            }
            let precision = hits as f32 / ours.len() as f32;
            let recall = hits as f32 / refs.len() as f32;
            let f = if hits == 0 {
                0.0
            } else {
                2.0 * precision * recall / (precision + recall)
            };
            (f, if hits == 0 { 0.0 } else { offset / hits as f32 })
        };
        let (f_on, late) = fmeasure(&refs);
        let (f_off, _) = fmeasure(&offbeats);
        if !f_on.is_nan() {
            sum_f += f_on;
            with_ref += 1;
        }
        let levels: Vec<String> = (0..4)
            .map(|k| format!("{:.2}/{:>2.0}%", sums[k] / looks, 100.0 * full[k] as f32 / looks))
            .collect();
        println!(
            "{:<30} {:>5.0} {:>6} {:>4.0}% {:>4.0}% {:>6} {:>5.2}x {:>5.1} {:>4} {:>4.0}%  F {:>3.0}% off {:>3.0}% late {:>+4.0}ms  {:+.2}/{:.2}  {}",
            name.chars().take(30).collect::<String>(),
            known,
            lock.map_or("never".to_string(), |t| format!("{t:.1}s")),
            acc1 * 100.0,
            acc2 * 100.0,
            mostly,
            beats as f32 / after_s / (known / 60.0),
            hits as f32 / seconds,
            last_section - first_section.unwrap_or(0),
            100.0 * coasting as f32 / looks,
            f_on * 100.0,
            f_off * 100.0,
            late * 1000.0,
            kick_at,
            kick_tight,
            levels.join(" "),
        );
    }
    println!(
        "\nmean over {count} songs: acc1 {:.0}%, acc2 {:.0}%, kicks gathered {:.2}; beat F {:.0}% over {with_ref} songs with reference beats",
        100.0 * sum1 / count.max(1) as f32,
        100.0 * sum2 / count.max(1) as f32,
        sum_tight / count.max(1) as f32,
        100.0 * sum_f / with_ref.max(1) as f32
    );
}
