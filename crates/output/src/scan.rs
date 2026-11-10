//! Finding WLED devices by asking every address on a local network.
//!
//! mDNS discovery is the first choice, but it fails on networks that block
//! multicast, and on some Windows setups. A scan is the fallback: it asks
//! each address of a /24 network "are you WLED?" over HTTP. It only ever
//! runs when the user asks for it, and only on private address ranges.

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::wled::{self, WledInfo};

/// A local network this computer is on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LocalNetwork {
    /// This computer's address on it.
    pub address: String,
    /// The first three numbers of addresses on it, with a trailing dot,
    /// such as `192.168.1.`. Devices are almost always found under it.
    pub prefix: String,
}

/// True for the ranges set aside for local networks. A scan is refused
/// anywhere else, so it cannot be pointed at the internet.
pub fn is_private(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local()
}

/// The local networks this computer is on, home-style networks first.
pub fn local_networks() -> Vec<LocalNetwork> {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut found: Vec<Ipv4Addr> = interfaces
        .into_iter()
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) if is_private(v4.ip) => Some(v4.ip),
            _ => None,
        })
        .collect();
    // 192.168.x.x is where home routers put things; link-local is a last
    // resort. Sorting also keeps the order stable between calls.
    found.sort_by_key(|ip| {
        let rank = match ip.octets() {
            [192, 168, ..] => 0,
            [10, ..] => 1,
            [172, ..] => 2,
            _ => 3,
        };
        (rank, *ip)
    });
    let mut networks: Vec<LocalNetwork> = Vec::new();
    for ip in found {
        let [a, b, c, _] = ip.octets();
        let prefix = format!("{a}.{b}.{c}.");
        if !networks.iter().any(|n| n.prefix == prefix) {
            networks.push(LocalNetwork {
                address: ip.to_string(),
                prefix,
            });
        }
    }
    networks
}

/// Reads the network part out of what a user or the UI supplies:
/// `192.168.1.`, `192.168.1` or a full address such as `192.168.1.50`.
pub fn parse_prefix(input: &str) -> Result<[u8; 3], String> {
    let parts: Vec<&str> = input.trim().trim_end_matches('.').split('.').collect();
    if !(3..=4).contains(&parts.len()) {
        return Err("enter the start of an address, such as 192.168.1.".into());
    }
    let mut octets = [0u8; 3];
    for (slot, part) in octets.iter_mut().zip(&parts) {
        *slot = part
            .parse()
            .map_err(|_| format!("{part:?} is not a number from 0 to 255"))?;
    }
    if !is_private(Ipv4Addr::new(octets[0], octets[1], octets[2], 1)) {
        return Err(
            "only local networks can be scanned (addresses starting 192.168., 10. or 172.16. to 172.31.)".into(),
        );
    }
    Ok(octets)
}

/// The 254 usable addresses under a prefix.
pub fn hosts_of(prefix: [u8; 3]) -> Vec<Ipv4Addr> {
    (1..=254)
        .map(|d| Ipv4Addr::new(prefix[0], prefix[1], prefix[2], d))
        .collect()
}

/// Asks every host whether it is a WLED device. Blocks until done.
///
/// `on_found` is called for each device that answers, `on_progress` after
/// every host with the number tried so far.
pub fn scan_hosts(
    hosts: Vec<Ipv4Addr>,
    http_port: u16,
    timeout: Duration,
    on_found: impl Fn(Ipv4Addr, WledInfo) + Send + Sync + 'static,
    on_progress: impl Fn(usize) + Send + Sync + 'static,
) {
    let hosts = Arc::new(hosts);
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(Mutex::new(0usize));
    let on_found = Arc::new(on_found);
    let on_progress = Arc::new(on_progress);
    // Most addresses do not answer at all and only cost a timeout, so many
    // can wait at once.
    let workers: Vec<_> = (0..hosts.len().min(32))
        .map(|_| {
            let (hosts, next, done) = (hosts.clone(), next.clone(), done.clone());
            let (on_found, on_progress) = (on_found.clone(), on_progress.clone());
            thread::spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(host) = hosts.get(i) else {
                        break;
                    };
                    if let Ok(info) = wled::fetch_info(&host.to_string(), http_port, timeout) {
                        on_found(*host, info);
                    }
                    let mut done = done.lock().unwrap();
                    *done += 1;
                    on_progress(*done);
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_are_read_from_what_people_type() {
        assert_eq!(parse_prefix("192.168.1.").unwrap(), [192, 168, 1]);
        assert_eq!(parse_prefix(" 192.168.1 ").unwrap(), [192, 168, 1]);
        assert_eq!(parse_prefix("10.0.42.17").unwrap(), [10, 0, 42]);
        assert_eq!(parse_prefix("172.20.0.").unwrap(), [172, 20, 0]);
        assert_eq!(parse_prefix("169.254.3.").unwrap(), [169, 254, 3]);
        assert!(parse_prefix("192.168").is_err());
        assert!(parse_prefix("192.168.1.2.3").is_err());
        assert!(parse_prefix("192.168.one.").is_err());
        assert!(parse_prefix("192.168.256.").is_err());
        assert!(parse_prefix("").is_err());
    }

    #[test]
    fn only_local_networks_can_be_scanned() {
        for public in ["8.8.8.", "1.1.1.", "172.32.0.", "192.169.0.", "100.64.0.", "127.0.0."] {
            assert!(parse_prefix(public).is_err(), "{public} should be refused");
        }
    }

    #[test]
    fn a_prefix_has_254_hosts() {
        let hosts = hosts_of([192, 168, 1]);
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(hosts[253], Ipv4Addr::new(192, 168, 1, 254));
    }

    #[test]
    fn local_networks_are_private_and_distinct() {
        // Whatever this machine has, the result must be well formed.
        let networks = local_networks();
        for n in &networks {
            let ip: Ipv4Addr = n.address.parse().unwrap();
            assert!(is_private(ip));
            assert!(n.address.starts_with(&n.prefix));
            assert!(n.prefix.ends_with('.'));
            assert!(parse_prefix(&n.prefix).is_ok());
        }
        let mut prefixes: Vec<_> = networks.iter().map(|n| &n.prefix).collect();
        prefixes.dedup();
        assert_eq!(prefixes.len(), networks.len());
    }
}
