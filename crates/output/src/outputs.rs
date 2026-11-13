//! Sends each rendered frame to every enabled device.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender as ChannelSender};
use std::thread;
use std::time::{Duration, Instant};

use darkrgb_core::layout::estimated_milliamps;
use darkrgb_core::{Device, DeviceKind, Renderer, Show};
use serde::Serialize;

use crate::ddp;
use crate::dmx::{self, artnet, sacn};

/// Running totals for one device, shown in the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DeviceStats {
    pub frames_sent: u64,
    /// Frames the operating system refused to send.
    pub frames_dropped: u64,
    /// False while a host name is still being looked up, or failed to.
    pub resolved: bool,
    /// What the last frame would draw, by estimate, in milliamps.
    pub milliamps: u32,
    /// True when the last frame was dimmed to keep to the current limit.
    pub limited: bool,
}

/// Frames waiting out a device's delay are not kept beyond this many.
const MAX_HELD: usize = 512;

enum Target {
    /// A host name is being looked up on another thread.
    Resolving,
    Ready(SocketAddr),
    Failed,
}

pub struct Outputs {
    sender: ddp::Sender,
    /// Device id to where its frames go, keyed by the address it was
    /// resolved for so a changed address is looked up again.
    targets: HashMap<u32, (String, u16, Target)>,
    stats: HashMap<u32, DeviceStats>,
    resolved_tx: ChannelSender<(u32, String, u16, Option<SocketAddr>)>,
    resolved_rx: Receiver<(u32, String, u16, Option<SocketAddr>)>,
    buf: Vec<u8>,
    /// One packet of sACN or Art-Net being put together.
    packet: Vec<u8>,
    /// Device id to the sequence number of its last sACN or Art-Net frame.
    sequences: HashMap<u32, u8>,
    /// Tells sACN receivers our packets apart from another sender's.
    cid: [u8; 16],
    /// Device id to the frames waiting out its delay, oldest first, each
    /// with the time it was made.
    held: HashMap<u32, VecDeque<(Instant, Vec<u8>)>>,
    /// Buffers that held frames have finished with, to be used again.
    spare: Vec<Vec<u8>>,
}

/// Sixteen bytes that differ each time the program starts.
fn new_cid() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut cid = [0u8; 16];
    for half in cid.chunks_mut(8) {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        half.copy_from_slice(&hasher.finish().to_be_bytes());
    }
    cid
}

impl Outputs {
    pub fn new() -> std::io::Result<Self> {
        let (resolved_tx, resolved_rx) = mpsc::channel();
        Ok(Self {
            sender: ddp::Sender::new()?,
            targets: HashMap::new(),
            stats: HashMap::new(),
            resolved_tx,
            resolved_rx,
            buf: Vec::new(),
            packet: Vec::new(),
            sequences: HashMap::new(),
            cid: new_cid(),
            held: HashMap::new(),
            spare: Vec::new(),
        })
    }

    pub fn stats(&self) -> &HashMap<u32, DeviceStats> {
        &self.stats
    }

    /// Call when the show changes. Starts looking up any new addresses.
    pub fn sync(&mut self, show: &Show) {
        self.targets.retain(|id, _| show.devices.iter().any(|d| d.id == *id));
        self.stats.retain(|id, _| show.devices.iter().any(|d| d.id == *id));
        self.sequences.retain(|id, _| show.devices.iter().any(|d| d.id == *id));
        self.held
            .retain(|id, _| show.devices.iter().any(|d| d.id == *id && d.delay_ms > 0));
        for device in &show.devices {
            // Virtual lights have no address and are never sent to.
            if device.kind == DeviceKind::Virtual {
                self.targets.remove(&device.id);
                self.stats.remove(&device.id);
                continue;
            }
            let current = self
                .targets
                .get(&device.id)
                .is_some_and(|(addr, port, _)| *addr == device.address && *port == device.port);
            if current {
                continue;
            }
            let target = match device.address.parse::<IpAddr>() {
                Ok(ip) => Target::Ready(SocketAddr::new(ip, device.port)),
                Err(_) => {
                    // Name lookups can take seconds, so never do them on
                    // the thread that renders frames.
                    let tx = self.resolved_tx.clone();
                    let (id, host, port) = (device.id, device.address.clone(), device.port);
                    thread::spawn(move || {
                        let addr = (host.as_str(), port)
                            .to_socket_addrs()
                            .ok()
                            .and_then(|mut addrs| addrs.find(SocketAddr::is_ipv4));
                        let _ = tx.send((id, host, port, addr));
                    });
                    Target::Resolving
                }
            };
            self.targets
                .insert(device.id, (device.address.clone(), device.port, target));
        }
    }

    /// Sends the renderer's last frame to every enabled device.
    pub fn send(&mut self, show: &Show, renderer: &Renderer) {
        self.send_at(show, renderer, Instant::now());
    }

    /// As [`Outputs::send`], with the time given, for tests.
    pub fn send_at(&mut self, show: &Show, renderer: &Renderer, now: Instant) {
        while let Ok((id, host, port, addr)) = self.resolved_rx.try_recv() {
            if let Some((h, p, target)) = self.targets.get_mut(&id) {
                // Ignore answers for an address the user has since changed.
                if *h == host && *p == port {
                    *target = addr.map_or(Target::Failed, Target::Ready);
                    if addr.is_none() {
                        log::warn!("could not find {host} on the network");
                    }
                }
            }
        }
        let mut buf = std::mem::take(&mut self.buf);
        for device in &show.devices {
            if device.kind == DeviceKind::Virtual {
                continue;
            }
            let stats = self.stats.entry(device.id).or_default();
            let addr = match self.targets.get(&device.id) {
                Some((_, _, Target::Ready(addr))) => *addr,
                _ => {
                    stats.resolved = false;
                    continue;
                }
            };
            stats.resolved = true;
            if !device.enabled {
                self.held.remove(&device.id);
                continue;
            }
            stats.limited = renderer.device_bytes(show, device.id, &mut buf);
            stats.milliamps = estimated_milliamps(&buf) as u32;
            if device.delay_ms > 0 {
                // The frame joins the queue, and the newest one that has
                // waited long enough is sent in its place. Until one has,
                // nothing is: the device keeps what it last showed.
                let queue = self.held.entry(device.id).or_default();
                let mut frame = self.spare.pop().unwrap_or_default();
                frame.clear();
                frame.extend_from_slice(&buf);
                queue.push_back((now, frame));
                while queue.len() > MAX_HELD {
                    self.spare.extend(queue.pop_front().map(|(_, f)| f));
                }
                let wait = Duration::from_millis(device.delay_ms as u64);
                let mut due = None;
                while queue.front().is_some_and(|(made, _)| now.duration_since(*made) >= wait) {
                    self.spare.extend(due.take());
                    due = queue.pop_front().map(|(_, f)| f);
                }
                let Some(frame) = due else { continue };
                self.spare.push(std::mem::replace(&mut buf, frame));
            } else {
                self.held.remove(&device.id);
            }
            let sent = self.transmit(device, addr, &buf);
            let stats = self.stats.entry(device.id).or_default();
            match sent {
                Ok(()) => stats.frames_sent += 1,
                Err(_) => stats.frames_dropped += 1,
            }
        }
        self.buf = buf;
    }

    /// One frame of bytes to one device, in its own protocol.
    fn transmit(&mut self, device: &Device, addr: SocketAddr, bytes: &[u8]) -> std::io::Result<()> {
        match device.kind {
            DeviceKind::Wled | DeviceKind::Virtual => self.sender.send_frame(addr, bytes).map(|_| ()),
            kind => {
                // One packet for each universe of 170 pixels. Every
                // universe of a frame carries the same sequence number.
                let seq = self.sequences.entry(device.id).or_default();
                *seq = seq.wrapping_add(1);
                let seq = *seq;
                for (i, channels) in bytes.chunks(dmx::CHANNELS).enumerate() {
                    let universe = device.universe.saturating_add(i as u16);
                    if kind == DeviceKind::Sacn {
                        sacn::packet(&mut self.packet, &self.cid, "darkRGB", universe, seq, channels);
                    } else {
                        artnet::packet(&mut self.packet, universe, seq, channels);
                    }
                    self.sender.send_raw(addr, &self.packet)?;
                }
                Ok(())
            }
        }
    }

    /// Leaves every device dark, and tells sACN receivers the stream is
    /// over. For when the program stops: without it a pixel controller
    /// holds its last frame, lit, until someone switches it off.
    pub fn go_dark(&mut self, show: &Show) {
        self.held.clear();
        let mut black = std::mem::take(&mut self.buf);
        for device in &show.devices {
            if !device.sends() {
                continue;
            }
            let Some((_, _, Target::Ready(addr))) = self.targets.get(&device.id) else {
                continue;
            };
            let addr = *addr;
            black.clear();
            black.resize(device.pixel_count as usize * 3, 0);
            // Twice, in case the network loses one.
            for _ in 0..2 {
                let _ = self.transmit(device, addr, &black);
            }
            if device.kind == DeviceKind::Sacn {
                let universes = darkrgb_core::layout::universes(device.pixel_count) as u16;
                let seq = self.sequences.get(&device.id).copied().unwrap_or(0);
                for round in 1..=3u8 {
                    for i in 0..universes {
                        let universe = device.universe.saturating_add(i);
                        sacn::farewell(
                            &mut self.packet,
                            &self.cid,
                            "darkRGB",
                            universe,
                            seq.wrapping_add(round),
                        );
                        let _ = self.sender.send_raw(addr, &self.packet);
                    }
                }
            }
        }
        self.buf = black;
    }
}

#[cfg(test)]
mod tests {
    use darkrgb_core::AudioFeatures;
    use std::net::UdpSocket;

    use super::*;

    /// Everything that arrives at `socket` within a short wait.
    fn receive(socket: &UdpSocket, packets: usize) -> Vec<Vec<u8>> {
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut buf = [0u8; 2048];
        (0..packets)
            .map(|_| {
                let (n, _) = socket.recv_from(&mut buf).expect("a packet should arrive");
                buf[..n].to_vec()
            })
            .collect()
    }

    #[test]
    fn pixel_controllers_get_one_packet_for_each_universe() {
        let sacn_rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let artnet_rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut show = Show::default();
        show.layers.clear();
        let layer = show.add_layer("solid");
        show.layer_mut(layer).unwrap().params.insert(
            "colour".into(),
            darkrgb_core::params::ParamValue::Text("#ff0000".into()),
        );
        // 400 pixels are three universes: 170, 170 and 60.
        let port = sacn_rx.local_addr().unwrap().port();
        show.add_device(Device::dmx(DeviceKind::Sacn, "roof", "127.0.0.1", port, 400, 7), None);
        let port = artnet_rx.local_addr().unwrap().port();
        show.add_device(Device::dmx(DeviceKind::Artnet, "wall", "127.0.0.1", port, 171, 0), None);
        for device in &mut show.devices {
            device.gamma = 1.0;
        }

        let mut renderer = Renderer::new();
        renderer.sync(&show);
        renderer.render(&show, 0.0, 1.0 / 60.0, &AudioFeatures::default());
        let mut outputs = Outputs::new().unwrap();
        outputs.sync(&show);
        outputs.send(&show, &renderer);
        outputs.send(&show, &renderer);

        let got = receive(&sacn_rx, 6);
        let parsed: Vec<_> = got.iter().map(|p| sacn::parse(p).expect("sACN")).collect();
        let universes: Vec<u16> = parsed.iter().map(|p| p.0).collect();
        assert_eq!(universes, [7, 8, 9, 7, 8, 9]);
        let lengths: Vec<usize> = parsed.iter().map(|p| p.2.len()).collect();
        assert_eq!(lengths, [510, 510, 180, 510, 510, 180]);
        let red = parsed[0].2[0];
        assert!(red > 0, "the lights are lit");
        assert!(
            parsed.iter().all(|p| p.2.chunks(3).all(|c| c == [red, 0, 0])),
            "{:?}",
            &parsed[0].2[..6]
        );
        // One sequence number for a frame, the next for the next.
        let seqs: Vec<u8> = parsed.iter().map(|p| p.1).collect();
        assert_eq!(seqs, [1, 1, 1, 2, 2, 2]);

        let got = receive(&artnet_rx, 4);
        let parsed: Vec<_> = got.iter().map(|p| artnet::parse(p).expect("Art-Net")).collect();
        let universes: Vec<u16> = parsed.iter().map(|p| p.0).collect();
        assert_eq!(universes, [0, 1, 0, 1]);
        // The second universe holds one pixel, padded to an even length.
        assert_eq!(parsed[1].2.len(), 4);
        assert_eq!(parsed[1].2[1..], [0, 0, 0]);
        assert_eq!(parsed[1].2[0], red);

        for device in &show.devices {
            assert_eq!(outputs.stats()[&device.id].frames_sent, 2);
        }
        // A paused controller is sent nothing.
        show.devices[0].enabled = false;
        outputs.sync(&show);
        outputs.send(&show, &renderer);
        assert_eq!(outputs.stats()[&show.devices[0].id].frames_sent, 2);
        assert_eq!(outputs.stats()[&show.devices[1].id].frames_sent, 3);
    }

    /// A show of one solid colour on one device of `pixels`.
    fn lit(device: Device, colour: &str) -> (Show, Renderer) {
        let mut show = Show::default();
        show.layers.clear();
        let layer = show.add_layer("solid");
        show.layer_mut(layer)
            .unwrap()
            .params
            .insert("colour".into(), darkrgb_core::params::ParamValue::Text(colour.into()));
        show.add_device(device, None);
        show.devices[0].gamma = 1.0;
        let mut renderer = Renderer::new();
        renderer.sync(&show);
        renderer.render(&show, 0.0, 1.0 / 60.0, &AudioFeatures::default());
        (show, renderer)
    }

    fn recolour(show: &mut Show, renderer: &mut Renderer, colour: &str) {
        show.layers[0]
            .params
            .insert("colour".into(), darkrgb_core::params::ParamValue::Text(colour.into()));
        renderer.render(show, 0.0, 1.0 / 60.0, &AudioFeatures::default());
    }

    #[test]
    fn a_delayed_device_gets_each_frame_that_much_later() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = rx.local_addr().unwrap().port();
        let (mut show, mut renderer) = lit(
            Device::dmx(DeviceKind::Artnet, "wall", "127.0.0.1", port, 1, 0),
            "#0a0000",
        );
        show.devices[0].delay_ms = 100;
        let mut outputs = Outputs::new().unwrap();
        outputs.sync(&show);
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // A frame every 20 ms, each a step brighter than the last.
        for step in 0..10u64 {
            recolour(&mut show, &mut renderer, &format!("#{:02x}0000", 10 + step));
            outputs.send_at(&show, &renderer, at(step * 20));
        }
        // Frames made at 0..=180 ms; those from 0 to 80 have waited 100.
        let id = show.devices[0].id;
        assert_eq!(outputs.stats()[&id].frames_sent, 5);
        let got = receive(&rx, 5);
        let reds: Vec<u8> = got.iter().map(|p| artnet::parse(p).unwrap().2[0]).collect();
        assert_eq!(reds, [10, 11, 12, 13, 14]);
        // After a gap the newest frame that is due is sent, not a backlog.
        recolour(&mut show, &mut renderer, "#640000");
        outputs.send_at(&show, &renderer, at(1000));
        assert_eq!(artnet::parse(&receive(&rx, 1)[0]).unwrap().2[0], 19);
        assert_eq!(outputs.stats()[&id].frames_sent, 6);
        // With the delay taken off, frames go at once and nothing is kept.
        show.devices[0].delay_ms = 0;
        outputs.sync(&show);
        outputs.send_at(&show, &renderer, at(1020));
        assert_eq!(artnet::parse(&receive(&rx, 1)[0]).unwrap().2[0], 100);
        assert!(outputs.held.is_empty());
    }

    #[test]
    fn stopping_leaves_the_lights_dark_and_ends_the_stream() {
        let sacn_rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sacn_rx.local_addr().unwrap().port();
        // 200 pixels: two universes.
        let (mut show, renderer) = lit(
            Device::dmx(DeviceKind::Sacn, "roof", "127.0.0.1", port, 200, 3),
            "#ffffff",
        );
        let ddp_rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = ddp_rx.local_addr().unwrap().port();
        show.add_device(Device::wled("strip", "127.0.0.1", port, 4), None);
        // A paused device is left as it is.
        let paused_rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = paused_rx.local_addr().unwrap().port();
        show.add_device(Device::wled("paused", "127.0.0.1", port, 4), None);
        show.devices[2].enabled = false;
        let mut outputs = Outputs::new().unwrap();
        outputs.sync(&show);
        outputs.send(&show, &renderer);
        let lit_frame = receive(&sacn_rx, 2);
        assert!(sacn::parse(&lit_frame[0]).unwrap().2.iter().all(|b| *b == 255));
        receive(&ddp_rx, 1);

        outputs.go_dark(&show);
        // Two black frames of two universes, then three farewells for each.
        let got = receive(&sacn_rx, 10);
        for packet in &got[..4] {
            assert!(!sacn::is_farewell(packet));
            assert!(sacn::parse(packet).unwrap().2.iter().all(|b| *b == 0), "black");
        }
        let farewells: Vec<u16> = got[4..]
            .iter()
            .map(|p| {
                assert!(sacn::is_farewell(p));
                u16::from_be_bytes([p[113], p[114]])
            })
            .collect();
        assert_eq!(farewells, [3, 4, 3, 4, 3, 4]);
        for packet in receive(&ddp_rx, 2) {
            let parsed = ddp::parse(&packet).expect("DDP");
            assert!(parsed.data.iter().all(|b| *b == 0), "black");
        }
        paused_rx.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        assert!(
            paused_rx.recv_from(&mut [0u8; 64]).is_err(),
            "nothing for a paused device"
        );
    }

    #[test]
    fn the_estimated_current_and_the_limit_are_reported() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = rx.local_addr().unwrap().port();
        let (mut show, renderer) = lit(Device::wled("strip", "127.0.0.1", port, 50), "#ffffff");
        let id = show.devices[0].id;
        let mut outputs = Outputs::new().unwrap();
        outputs.sync(&show);
        outputs.send(&show, &renderer);
        assert_eq!(outputs.stats()[&id].milliamps, 3050);
        assert!(!outputs.stats()[&id].limited);
        show.devices[0].max_milliamps = 1000;
        outputs.send(&show, &renderer);
        let stats = outputs.stats()[&id];
        assert!(stats.limited && (950..=1000).contains(&stats.milliamps), "{stats:?}");
        let frames = receive(&rx, 2);
        let level = ddp::parse(&frames[1]).unwrap().data[0];
        assert!((75..=80).contains(&level), "{level}");
    }
}
