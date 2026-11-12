//! A pretend WLED device, for developing and testing without hardware.
//!
//! It answers the parts of WLED the app uses: `/json/info` over HTTP, pixel
//! data over DDP, and an mDNS advertisement so discovery finds it. It also
//! measures how regularly frames arrive, which a real device cannot report.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use darkrgb_output::ddp;
use serde_json::json;

/// WLED leaves realtime mode this long after the last packet.
const LIVE_TIMEOUT: Duration = Duration::from_millis(2500);

#[derive(Clone, Debug)]
pub struct SimConfig {
    pub name: String,
    pub leds: u32,
    /// Columns and rows, to pose as a 2D matrix.
    pub matrix: Option<(u32, u32)>,
    /// 0 picks a free port.
    pub http_port: u16,
    /// 0 picks a free port. Real devices always use 4048.
    pub ddp_port: u16,
    /// Advertise on the local network so discovery finds it.
    pub mdns: bool,
    /// Listen on all interfaces instead of this machine only.
    pub public: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            name: "Sim strip".into(),
            leds: 60,
            matrix: None,
            http_port: 0,
            ddp_port: 0,
            mdns: false,
            public: false,
        }
    }
}

/// How regularly frames have been arriving.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub frames: u64,
    pub packets: u64,
    /// Packets that were not valid DDP or pointed outside the LEDs.
    pub rejected: u64,
    /// Frames per second over the recent intervals.
    pub fps: f32,
    /// Milliseconds between frames: average, shortest and longest.
    pub interval_ms: f32,
    pub min_ms: f32,
    pub max_ms: f32,
    /// Standard deviation of the interval, in milliseconds.
    pub jitter_ms: f32,
}

struct State {
    /// What the LEDs show: the last pushed frame.
    shown: Vec<u8>,
    /// Data received since the last push.
    pending: Vec<u8>,
    frames: u64,
    packets: u64,
    rejected: u64,
    last_push: Option<Instant>,
    last_packet: Option<(Instant, IpAddr)>,
    /// Recent gaps between frames, newest last.
    intervals: Vec<f32>,
}

pub struct Sim {
    pub name: String,
    pub http_port: u16,
    pub ddp_port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
    http: Arc<tiny_http::Server>,
    mdns: Option<mdns_sd::ServiceDaemon>,
}

impl Sim {
    pub fn start(config: SimConfig) -> io::Result<Self> {
        let bind_ip = if config.public {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        let udp = UdpSocket::bind(SocketAddr::from((bind_ip, config.ddp_port)))?;
        udp.set_read_timeout(Some(Duration::from_millis(100)))?;
        let ddp_port = udp.local_addr()?.port();

        let http = tiny_http::Server::http(SocketAddr::from((bind_ip, config.http_port)))
            .map_err(|e| io::Error::new(io::ErrorKind::AddrInUse, e.to_string()))?;
        let http_port = http
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .ok_or_else(|| io::Error::other("no HTTP port"))?;
        let http = Arc::new(http);

        let bytes = config.leds as usize * 3;
        let state = Arc::new(Mutex::new(State {
            shown: vec![0; bytes],
            pending: vec![0; bytes],
            frames: 0,
            packets: 0,
            rejected: 0,
            last_push: None,
            last_packet: None,
            intervals: Vec::new(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let mac = fake_mac(&config.name, http_port);

        let mut workers = Vec::new();
        {
            let (state, stop) = (state.clone(), stop.clone());
            workers.push(thread::spawn(move || receive(udp, state, stop)));
        }
        {
            let (state, stop, http, config, mac) =
                (state.clone(), stop.clone(), http.clone(), config.clone(), mac.clone());
            workers.push(thread::spawn(move || serve(http, state, stop, config, mac, ddp_port)));
        }

        let mdns = if config.mdns {
            match advertise(&config.name, http_port, &mac, config.public) {
                Ok(daemon) => Some(daemon),
                Err(e) => {
                    log::warn!("could not advertise {}: {e}", config.name);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            name: config.name,
            http_port,
            ddp_port,
            state,
            stop,
            workers,
            http,
            mdns,
        })
    }

    /// The RGB bytes currently shown.
    pub fn frame(&self) -> Vec<u8> {
        self.state.lock().unwrap().shown.clone()
    }

    pub fn stats(&self) -> Stats {
        stats(&self.state.lock().unwrap())
    }

    /// Forgets the timing history, to measure a fresh stretch.
    pub fn reset_stats(&self) {
        let mut s = self.state.lock().unwrap();
        s.frames = 0;
        s.packets = 0;
        s.rejected = 0;
        s.intervals.clear();
        s.last_push = None;
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.http.unblock();
        if let Some(daemon) = self.mdns.take() {
            let _ = daemon.shutdown();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn stats(s: &State) -> Stats {
    let n = s.intervals.len();
    if n == 0 {
        return Stats {
            frames: s.frames,
            packets: s.packets,
            rejected: s.rejected,
            ..Stats::default()
        };
    }
    let mean = s.intervals.iter().sum::<f32>() / n as f32;
    let var = s.intervals.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n as f32;
    Stats {
        frames: s.frames,
        packets: s.packets,
        rejected: s.rejected,
        fps: if mean > 0.0 { 1000.0 / mean } else { 0.0 },
        interval_ms: mean,
        min_ms: s.intervals.iter().copied().fold(f32::MAX, f32::min),
        max_ms: s.intervals.iter().copied().fold(0.0, f32::max),
        jitter_ms: var.sqrt(),
    }
}

fn receive(udp: UdpSocket, state: Arc<Mutex<State>>, stop: Arc<AtomicBool>) {
    let mut buf = [0u8; 2048];
    while !stop.load(Ordering::Relaxed) {
        let Ok((n, from)) = udp.recv_from(&mut buf) else {
            continue;
        };
        let now = Instant::now();
        let mut s = state.lock().unwrap();
        s.packets += 1;
        let Some(packet) = ddp::parse(&buf[..n]) else {
            s.rejected += 1;
            continue;
        };
        let end = packet.offset + packet.data.len();
        if end > s.pending.len() {
            // A real device ignores pixels past the end of its strip.
            s.rejected += 1;
            let fits = s.pending.len().saturating_sub(packet.offset);
            if fits > 0 {
                let offset = packet.offset;
                s.pending[offset..].copy_from_slice(&packet.data[..fits]);
            }
        } else {
            s.pending[packet.offset..end].copy_from_slice(packet.data);
        }
        s.last_packet = Some((now, from.ip()));
        if packet.push {
            let State { shown, pending, .. } = &mut *s;
            shown.copy_from_slice(pending);
            s.frames += 1;
            if let Some(last) = s.last_push {
                s.intervals.push(now.duration_since(last).as_secs_f32() * 1000.0);
                // Keep about ten seconds at 60 frames per second.
                if s.intervals.len() > 600 {
                    s.intervals.remove(0);
                }
            }
            s.last_push = Some(now);
        }
    }
}

fn serve(
    http: Arc<tiny_http::Server>,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    config: SimConfig,
    mac: String,
    ddp_port: u16,
) {
    let json_header = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
    while !stop.load(Ordering::Relaxed) {
        let Ok(Some(request)) = http.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        let path = request
            .url()
            .split('?')
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        let body = {
            let s = state.lock().unwrap();
            let live = s.last_packet.filter(|(at, _)| at.elapsed() < LIVE_TIMEOUT);
            match path.as_str() {
                "/json/info" => Some(info(&config, &mac, ddp_port, live.map(|(_, ip)| ip)).to_string()),
                "/json/state" => Some(json!({ "on": true, "bri": 128, "live": live.is_some() }).to_string()),
                "/sim/stats" => {
                    let st = stats(&s);
                    Some(
                        json!({
                            "frames": st.frames, "packets": st.packets, "rejected": st.rejected,
                            "fps": st.fps, "interval_ms": st.interval_ms, "min_ms": st.min_ms,
                            "max_ms": st.max_ms, "jitter_ms": st.jitter_ms,
                        })
                        .to_string(),
                    )
                }
                _ => None,
            }
        };
        let _ = match body {
            Some(body) => request.respond(tiny_http::Response::from_string(body).with_header(json_header.clone())),
            None => request.respond(tiny_http::Response::from_string("not found").with_status_code(404)),
        };
    }
}

/// The subset of WLED's `/json/info` the app reads, shaped like the real one.
fn info(config: &SimConfig, mac: &str, ddp_port: u16, live_from: Option<IpAddr>) -> serde_json::Value {
    let mut leds = json!({ "count": config.leds, "fps": 60, "rgbw": false, "maxseg": 32 });
    if let Some((w, h)) = config.matrix {
        leds["matrix"] = json!({ "w": w, "h": h });
    }
    json!({
        "ver": "0.15.0-sim",
        "name": config.name,
        "leds": leds,
        "live": live_from.is_some(),
        "lm": if live_from.is_some() { "DDP" } else { "" },
        "lip": live_from.map(|ip| ip.to_string()).unwrap_or_default(),
        "arch": "sim",
        "mac": mac,
        "udpport": 21324,
        "darkrgb_sim": { "ddp_port": ddp_port },
    })
}

fn fake_mac(name: &str, port: u16) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes().chain(port.to_be_bytes()) {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

fn advertise(name: &str, http_port: u16, mac: &str, public: bool) -> Result<mdns_sd::ServiceDaemon, String> {
    let daemon = mdns_sd::ServiceDaemon::new().map_err(|e| e.to_string())?;
    let host = format!("wled-{mac}.local.");
    let props = std::collections::HashMap::from([("mac".to_string(), mac.to_string())]);
    // Advertise only addresses the simulator actually listens on.
    let service = if public {
        mdns_sd::ServiceInfo::new("_wled._tcp.local.", name, &host, (), http_port, props)
            .map_err(|e| e.to_string())?
            .enable_addr_auto()
    } else {
        mdns_sd::ServiceInfo::new("_wled._tcp.local.", name, &host, "127.0.0.1", http_port, props)
            .map_err(|e| e.to_string())?
    };
    daemon.register(service).map_err(|e| e.to_string())?;
    Ok(daemon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkrgb_output::wled;

    fn wait_for(mut done: impl FnMut() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(started.elapsed() < Duration::from_secs(3), "timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn describes_itself_like_wled() {
        let sim = Sim::start(SimConfig {
            name: "Panel".into(),
            leds: 256,
            matrix: Some((16, 16)),
            ..SimConfig::default()
        })
        .unwrap();
        let info = wled::fetch_info("127.0.0.1", sim.http_port, Duration::from_secs(2)).unwrap();
        assert_eq!(info.name, "Panel");
        assert_eq!(info.led_count, 256);
        assert_eq!(info.matrix, Some((16, 16)));
        assert_eq!(info.ddp_port, sim.ddp_port);
        assert!(!info.live);
        assert_eq!(info.mac.as_ref().map(String::len), Some(12));
    }

    #[test]
    fn shows_a_frame_only_once_it_is_pushed() {
        let sim = Sim::start(SimConfig {
            leds: 600,
            ..SimConfig::default()
        })
        .unwrap();
        let addr: SocketAddr = ([127, 0, 0, 1], sim.ddp_port).into();
        let frame: Vec<u8> = (0..1800).map(|i| (i % 250) as u8 + 1).collect();
        let packets = ddp::packets(&frame, 1);
        assert_eq!(packets.len(), 2);
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();

        socket.send_to(&packets[0], addr).unwrap();
        wait_for(|| sim.stats().packets == 1);
        assert!(sim.frame().iter().all(|b| *b == 0), "nothing shows before the push");

        socket.send_to(&packets[1], addr).unwrap();
        wait_for(|| sim.stats().frames == 1);
        assert_eq!(sim.frame(), frame);

        let info = wled::fetch_info("127.0.0.1", sim.http_port, Duration::from_secs(2)).unwrap();
        assert!(info.live);
        assert_eq!(info.live_source.as_deref(), Some("DDP from 127.0.0.1"));
    }

    #[test]
    fn counts_junk_and_overlong_frames_as_rejected() {
        let sim = Sim::start(SimConfig {
            leds: 2,
            ..SimConfig::default()
        })
        .unwrap();
        let addr: SocketAddr = ([127, 0, 0, 1], sim.ddp_port).into();
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.send_to(b"hello", addr).unwrap();
        // Three pixels to a two-pixel device.
        socket.send_to(&ddp::packets(&[9u8; 9], 1)[0], addr).unwrap();
        wait_for(|| sim.stats().packets == 2);
        assert_eq!(sim.stats().rejected, 2);
        assert_eq!(sim.frame(), [9u8; 6], "the pixels that fit are still shown");
    }

    #[test]
    fn measures_frame_timing() {
        let sim = Sim::start(SimConfig::default()).unwrap();
        let addr: SocketAddr = ([127, 0, 0, 1], sim.ddp_port).into();
        let mut sender = ddp::Sender::new().unwrap();
        for _ in 0..20 {
            sender.send_frame(addr, &[1u8; 180]).unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        wait_for(|| sim.stats().frames == 20);
        let stats = sim.stats();
        assert!(stats.interval_ms > 9.0 && stats.interval_ms < 20.0, "{stats:?}");
        assert!(stats.fps > 50.0 && stats.fps < 111.0, "{stats:?}");
    }
}
