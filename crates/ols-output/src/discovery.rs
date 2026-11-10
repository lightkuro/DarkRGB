//! Finds WLED devices on the local network by mDNS.

use std::net::IpAddr;
use std::thread;

use mdns_sd::{ServiceDaemon, ServiceEvent};

/// The service name WLED advertises.
pub const WLED_SERVICE: &str = "_wled._tcp.local.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// mDNS instance name, unique on the network. Used to notice removal.
    pub key: String,
    /// The device's IPv4 address.
    pub host: String,
    pub http_port: u16,
    pub mac: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Found(Found),
    Lost { key: String },
}

/// Keeps browsing until dropped.
pub struct Discovery {
    daemon: ServiceDaemon,
}

impl Discovery {
    /// Starts browsing and calls `on_event` from a background thread.
    pub fn start(on_event: impl Fn(Event) + Send + 'static) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| format!("could not start mDNS: {e}"))?;
        let events = daemon
            .browse(WLED_SERVICE)
            .map_err(|e| format!("could not browse for WLED devices: {e}"))?;
        thread::Builder::new()
            .name("discovery".into())
            .spawn(move || {
                while let Ok(event) = events.recv() {
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            // WLED streams over IPv4, so ignore v6-only answers.
                            let v4 = info.addresses.iter().map(|a| a.to_ip_addr()).find(IpAddr::is_ipv4);
                            if let Some(ip) = v4 {
                                on_event(Event::Found(Found {
                                    key: info.fullname.clone(),
                                    host: ip.to_string(),
                                    http_port: info.port,
                                    mac: info.txt_properties.get_property_val_str("mac").map(str::to_string),
                                }));
                            }
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => on_event(Event::Lost { key: fullname }),
                        _ => {}
                    }
                }
            })
            .map_err(|e| format!("could not start the discovery thread: {e}"))?;
        Ok(Self { daemon })
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}
