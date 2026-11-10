//! Command-line checks for the things that only real hardware can prove:
//! system audio capture, and steady frames to a real device.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use ols_audio::{AudioCapture, AudioState};
use ols_core::Rgb;
use ols_output::discovery::{Discovery, Event};
use ols_output::{ddp, wled};

/// Listens for a while and says whether system audio is being captured.
pub fn audio_test(source: &str, seconds: u64) -> bool {
    println!("Listening for {seconds} s. Play some music now.\n");
    let capture = AudioCapture::start(source, 1.0);
    let started = Instant::now();
    let mut announced = None;
    let mut running_since: Option<Instant> = None;
    let mut samples_at_start = 0u64;
    let mut loudest = 0.0f32;
    let mut bpm_seen = 0.0f32;
    let mut sample_rate = 0u32;

    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(100));
        let status = capture.status();
        if announced.as_ref() != Some(&status) {
            match status.state {
                AudioState::Starting => {}
                AudioState::Running => {
                    println!(
                        "Capturing from: {} ({} Hz, {} channels)",
                        status.device.as_deref().unwrap_or("?"),
                        status.sample_rate,
                        status.channels
                    );
                    running_since = Some(Instant::now());
                    samples_at_start = capture.samples_received();
                    sample_rate = status.sample_rate;
                }
                AudioState::Error => {
                    println!("Problem: {}", status.message.as_deref().unwrap_or("unknown"));
                    running_since = None;
                }
            }
            announced = Some(status);
        }
        let f = capture.features();
        loudest = loudest.max(f.level);
        if f.bpm > 0.0 {
            bpm_seen = f.bpm;
        }
        let bar = "#".repeat((f.level * 40.0) as usize);
        print!(
            "\r  level {:4.2}  bass {:4.2}  mid {:4.2}  treble {:4.2}  {:>5.1} BPM  |{bar:<40}|",
            f.level, f.bass, f.mid, f.treble, f.bpm
        );
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
    println!("\n");

    let Some(since) = running_since else {
        println!("RESULT: FAILED. Audio capture never started. See the problem above.");
        return false;
    };
    let received = capture.samples_received() - samples_at_start;
    let expected = since.elapsed().as_secs_f64() * sample_rate as f64;
    let share = if expected > 0.0 {
        received as f64 / expected * 100.0
    } else {
        0.0
    };

    if loudest < 0.05 {
        // Windows delivers nothing at all while nothing plays, so a low
        // share of samples is normal here and says nothing by itself.
        println!("RESULT: NO SOUND HEARD. Capture started, but nothing was playing.");
        println!("Start some music on this computer and run the test again.");
        return false;
    }
    println!("Received {share:.0}% of the audio that was expected while listening.");
    if bpm_seen > 0.0 {
        println!("Tempo detected: about {bpm_seen:.0} BPM.");
    } else {
        println!("No steady tempo detected (normal for speech or ambient music).");
    }
    println!("RESULT: OK. System audio is being captured with no setup.");
    true
}

/// Streams a test pattern to one device and reports how steady it was.
pub fn ddp_test(address: &str, fps: u32, seconds: u64) -> bool {
    let (host, http_port) = match wled::parse_address(address) {
        Ok(parsed) => parsed,
        Err(e) => {
            println!("{e}");
            return false;
        }
    };
    let info = match wled::fetch_info(&host, http_port, Duration::from_secs(4)) {
        Ok(info) => info,
        Err(e) => {
            println!("Could not reach the device: {e}");
            return false;
        }
    };
    println!(
        "Device: {} (version {}, {} LEDs{})",
        info.name,
        info.version,
        info.led_count,
        info.matrix
            .map(|(w, h)| format!(", {w}x{h} matrix"))
            .unwrap_or_default()
    );
    if let (true, Some(source)) = (info.live, &info.live_source) {
        println!("Warning: it is already receiving a stream ({source}). Stop that first for a clean test.");
    }
    let target = match (host.as_str(), info.ddp_port).to_socket_addrs_v4() {
        Some(addr) => addr,
        None => {
            println!("Could not look up {host}.");
            return false;
        }
    };

    let fps = fps.clamp(1, 240);
    let pixels = info.led_count as usize;
    let packets_per_frame = (pixels * 3).div_ceil(ddp::MAX_DATA).max(1);
    println!(
        "Sending a moving rainbow at {fps} frames per second for {seconds} s \
         ({packets_per_frame} packet(s) per frame).\nWatch the lights: the white dot should glide without stutter.\n"
    );

    // Ask the device how it is doing from another thread, so slow HTTP
    // replies do not disturb the timing being measured.
    let stop = Arc::new(AtomicBool::new(false));
    let (report_tx, report_rx) = mpsc::channel::<String>();
    let poller = {
        let (stop, host, is_sim) = (stop.clone(), host.clone(), info.arch.as_deref() == Some("sim"));
        std::thread::spawn(move || {
            let mut device_fps = Vec::new();
            let mut last_sim = None;
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(2));
                if let Ok(now) = wled::fetch_info(&host, http_port, Duration::from_secs(2)) {
                    if let Some(f) = now.fps {
                        device_fps.push(f);
                    }
                }
                if is_sim {
                    if let Ok(body) = wled::get(&host, http_port, "/sim/stats", Duration::from_secs(2)) {
                        last_sim = Some(body);
                    }
                }
            }
            if is_sim {
                if let Some(body) = last_sim {
                    let _ = report_tx.send(format!("Simulator's own measurement of arrivals: {body}"));
                }
            } else if !device_fps.is_empty() {
                let min = device_fps.iter().min().unwrap();
                let avg = device_fps.iter().sum::<u32>() as f32 / device_fps.len() as f32;
                let _ = report_tx.send(format!(
                    "The device reported its own refresh rate as {avg:.0} per second on average, {min} at worst."
                ));
            }
        })
    };

    let mut sender = match ddp::Sender::new() {
        Ok(sender) => sender,
        Err(e) => {
            println!("Could not open a network socket: {e}");
            return false;
        }
    };
    let period = Duration::from_secs_f64(1.0 / fps as f64);
    let sleeper = spin_sleep::SpinSleeper::default();
    let started = Instant::now();
    let mut deadline = started;
    let mut last_send: Option<Instant> = None;
    let mut gaps_ms: Vec<f32> = Vec::new();
    let mut frame = vec![0u8; pixels * 3];
    let (mut sent, mut dropped) = (0u64, 0u64);

    while started.elapsed() < Duration::from_secs(seconds) {
        sleeper.sleep_until(deadline);
        deadline += period;
        let t = started.elapsed().as_secs_f32();
        let dot = (t * 0.5).fract() * pixels as f32;
        for (i, px) in frame.chunks_exact_mut(3).enumerate() {
            let colour = if (i as f32 - dot).abs() < 1.5 {
                Rgb::WHITE
            } else {
                Rgb::from_hsv(i as f32 / pixels.max(1) as f32 + t * 0.1, 1.0, 0.25)
            };
            px.copy_from_slice(&colour.to_bytes());
        }
        let now = Instant::now();
        match sender.send_frame(target, &frame) {
            Ok(_) => sent += 1,
            Err(_) => dropped += 1,
        }
        if let Some(last) = last_send {
            gaps_ms.push(now.duration_since(last).as_secs_f32() * 1000.0);
        }
        last_send = Some(now);
    }
    // Leave the lights dark, not frozen on the last frame.
    frame.fill(0);
    let _ = sender.send_frame(target, &frame);
    stop.store(true, Ordering::Relaxed);
    let _ = poller.join();

    let n = gaps_ms.len().max(1) as f32;
    let mean = gaps_ms.iter().sum::<f32>() / n;
    let jitter = (gaps_ms.iter().map(|g| (g - mean) * (g - mean)).sum::<f32>() / n).sqrt();
    let worst = gaps_ms.iter().copied().fold(0.0f32, f32::max);
    println!("Sent {sent} frames ({dropped} refused by the operating system).");
    println!(
        "Gap between frames as sent: {mean:.2} ms on average (target {:.2}), {worst:.2} ms at worst, jitter {jitter:.3} ms.",
        period.as_secs_f32() * 1000.0
    );
    while let Ok(line) = report_rx.try_recv() {
        println!("{line}");
    }
    let steady = dropped == 0 && worst < period.as_secs_f32() * 1000.0 * 2.0;
    if steady {
        println!("RESULT: OK on the sending side. Judge the lights by eye: smooth means the network keeps up.");
    } else {
        println!("RESULT: UNSTEADY. This computer did not send frames on time.");
    }
    steady
}

trait ResolveV4 {
    fn to_socket_addrs_v4(&self) -> Option<std::net::SocketAddr>;
}

impl ResolveV4 for (&str, u16) {
    fn to_socket_addrs_v4(&self) -> Option<std::net::SocketAddr> {
        use std::net::ToSocketAddrs;
        self.to_socket_addrs().ok()?.find(std::net::SocketAddr::is_ipv4)
    }
}

/// Lists WLED devices that answer on the local network.
pub fn discover(seconds: u64) -> bool {
    println!("Looking for WLED devices for {seconds} s...\n");
    let (tx, rx) = mpsc::channel();
    let _discovery = match Discovery::start(move |event| {
        let _ = tx.send(event);
    }) {
        Ok(d) => d,
        Err(e) => {
            println!("{e}");
            return false;
        }
    };
    let started = Instant::now();
    let mut seen = Vec::new();
    while let Some(left) = Duration::from_secs(seconds).checked_sub(started.elapsed()) {
        let Ok(Event::Found(found)) = rx.recv_timeout(left) else {
            continue;
        };
        if seen.contains(&found.key) {
            continue;
        }
        seen.push(found.key.clone());
        let answer = found.hosts.iter().find_map(|host| {
            Some((
                host,
                wled::fetch_info(host, found.http_port, Duration::from_secs(3)).ok()?,
            ))
        });
        match answer {
            Some((host, info)) => {
                let address = if found.http_port == 80 {
                    host.clone()
                } else {
                    format!("{host}:{}", found.http_port)
                };
                println!(
                    "  {:<24} {:<22} {:>5} LEDs{}  version {}",
                    info.name,
                    address,
                    info.led_count,
                    info.matrix.map(|(w, h)| format!(" ({w}x{h})")).unwrap_or_default(),
                    info.version
                );
            }
            None => println!("  advertised at {} but did not answer", found.hosts.join(", ")),
        }
    }
    if seen.is_empty() {
        println!("None found. Devices on another network segment or Wi-Fi with client isolation");
        println!("do not show up here; they can still be added by IP address.");
    }
    !seen.is_empty()
}
