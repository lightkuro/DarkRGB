//! Sends each rendered frame to every enabled device.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender as ChannelSender};
use std::thread;

use ols_core::{Renderer, Show};
use serde::Serialize;

use crate::ddp;

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
        })
    }

    pub fn stats(&self) -> &HashMap<u32, DeviceStats> {
        &self.stats
    }

    /// Call when the show changes. Starts looking up any new addresses.
    pub fn sync(&mut self, show: &Show) {
        self.targets.retain(|id, _| show.devices.iter().any(|d| d.id == *id));
        self.stats.retain(|id, _| show.devices.iter().any(|d| d.id == *id));
        for device in &show.devices {
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
            match self.sender.send_frame(addr, &self.buf) {
                Ok(_) => stats.frames_sent += 1,
                Err(_) => stats.frames_dropped += 1,
            }
        }
    }
}
