//! Sends each rendered frame to every enabled device.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender as ChannelSender};
use std::thread;

use darkrgb_core::{DeviceKind, Renderer, Show};
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
}

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
                continue;
            }
            renderer.device_bytes(show, device.id, &mut self.buf);
            let sent = match device.kind {
                DeviceKind::Wled => self.sender.send_frame(addr, &self.buf).map(|_| ()),
                kind => {
                    // One packet for each universe of 170 pixels. Every
                    // universe of a frame carries the same sequence number.
                    let seq = self.sequences.entry(device.id).or_default();
                    *seq = seq.wrapping_add(1);
                    let seq = *seq;
                    let mut result = Ok(());
                    for (i, channels) in self.buf.chunks(dmx::CHANNELS).enumerate() {
                        let universe = device.universe.saturating_add(i as u16);
                        if kind == DeviceKind::Sacn {
                            sacn::packet(&mut self.packet, &self.cid, "darkRGB", universe, seq, channels);
                        } else {
                            artnet::packet(&mut self.packet, universe, seq, channels);
                        }
                        if let Err(e) = self.sender.send_raw(addr, &self.packet) {
                            result = Err(e);
                            break;
                        }
                    }
                    result
                }
            };
            match sent {
                Ok(()) => stats.frames_sent += 1,
                Err(_) => stats.frames_dropped += 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::UdpSocket;
    use std::time::Duration;

    use darkrgb_core::{AudioFeatures, Device};

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
}
