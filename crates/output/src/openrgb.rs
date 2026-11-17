//! OpenRGB: the lights inside and around a computer (keyboards, memory,
//! fans, motherboard headers) by way of the OpenRGB program, which talks
//! to the hardware and takes colours from other programs over TCP.
//!
//! Written from OpenRGB's SDK document, protocol version 0, which every
//! version of its server still speaks: a sixteen-byte header ("ORGB",
//! the device's number, what the packet is, how long its data is), then
//! the data, all little-endian.
//!
//! Sending must never hold up a frame, and TCP can: so each device has a
//! thread of its own that sends the newest frame it has been given and
//! reconnects by itself if the program on the other end goes away.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// The port OpenRGB's server listens on unless told otherwise.
pub const PORT: u16 = 6742;
/// What we tell OpenRGB we are called; it shows this in its list of clients.
const CLIENT_NAME: &str = "darkRGB";
/// OpenRGB's hardware is slow next to an LED strip: more frames a second
/// than this only queue up inside it.
const MAX_RATE: u32 = 40;

const REQUEST_CONTROLLER_COUNT: u32 = 0;
const REQUEST_CONTROLLER_DATA: u32 = 1;
const SET_CLIENT_NAME: u32 = 50;
const UPDATE_LEDS: u32 = 1050;
const SET_CUSTOM_MODE: u32 = 1100;
/// The most a description of one device may be. Real ones are a few
/// kilobytes; a keyboard with every key named, some tens.
const MAX_DESCRIPTION: usize = 4 * 1024 * 1024;

/// One of the devices OpenRGB knows about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Controller {
    /// Its number in OpenRGB's list, from nought.
    pub index: u32,
    pub name: String,
    pub leds: u32,
}

fn header(out: &mut Vec<u8>, device: u32, id: u32, size: u32) {
    out.clear();
    out.extend_from_slice(b"ORGB");
    out.extend_from_slice(&device.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
}

/// The packet that sets every LED of one device. `rgb` is three bytes
/// to an LED; OpenRGB wants four, the last unused.
pub fn update_leds(out: &mut Vec<u8>, device: u32, rgb: &[u8]) {
    let leds = (rgb.len() / 3).min(u16::MAX as usize);
    let size = 4 + 2 + 4 * leds as u32;
    header(out, device, UPDATE_LEDS, size);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&(leds as u16).to_le_bytes());
    for led in rgb.chunks_exact(3).take(leds) {
        out.extend_from_slice(&[led[0], led[1], led[2], 0]);
    }
}

/// What a client sends on connecting: its name, and for the device it
/// means to drive, the request to hand its LEDs over.
fn greeting(out: &mut Vec<u8>, device: Option<u32>) {
    let mut packet = Vec::new();
    header(&mut packet, 0, SET_CLIENT_NAME, CLIENT_NAME.len() as u32 + 1);
    packet.extend_from_slice(CLIENT_NAME.as_bytes());
    packet.push(0);
    out.clear();
    out.extend_from_slice(&packet);
    if let Some(device) = device {
        header(&mut packet, device, SET_CUSTOM_MODE, 0);
        out.extend_from_slice(&packet);
    }
}

/// Reads one packet's data, checking it is the answer that was asked for.
fn answer(stream: &mut TcpStream, id: u32) -> Result<Vec<u8>, String> {
    let mut head = [0u8; 16];
    stream
        .read_exact(&mut head)
        .map_err(|e| format!("OpenRGB stopped answering: {e}"))?;
    if &head[..4] != b"ORGB" {
        return Err("what answered on that port is not OpenRGB".into());
    }
    let got = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
    let size = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    if got != id || size > MAX_DESCRIPTION {
        return Err("OpenRGB sent an answer that was not expected".into());
    }
    let mut data = vec![0u8; size];
    stream
        .read_exact(&mut data)
        .map_err(|e| format!("OpenRGB stopped answering: {e}"))?;
    Ok(data)
}

/// The name and the number of LEDs out of a device's description.
///
/// The description starts with its own length and the kind of device,
/// then the name. It ends with the LEDs: how many, then a name and a
/// number for each, then how many colours, then one colour of four
/// bytes for each LED. What lies between (modes and zones) differs from
/// one version of OpenRGB to the next, so it is not read. The LEDs are
/// found from the end instead: a count of colours that ends exactly
/// where the description does, with a list of that many LEDs ending
/// exactly where that count begins.
pub fn describe(data: &[u8]) -> Option<(String, u32)> {
    let name_len = u16::from_le_bytes([*data.get(8)?, *data.get(9)?]) as usize;
    let name = data.get(10..10 + name_len)?;
    let name = String::from_utf8_lossy(name).trim_end_matches('\0').trim().to_string();
    let after_name = 10 + name_len;
    let word = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]) as usize;
    // Whether `count` LEDs, each a name with its length before it and a
    // number of four bytes after, fill `from..to` exactly.
    let leds_fill = |count: usize, from: usize, to: usize| {
        let mut at = from;
        for _ in 0..count {
            if at + 2 > to {
                return false;
            }
            at += 2 + word(at) + 4;
        }
        at == to
    };
    let most = (data.len().saturating_sub(after_name + 4)) / 4;
    let leds = (0..=most.min(u16::MAX as usize)).rev().find(|n| {
        let colours = data.len() - 4 * n - 2;
        if word(colours) != *n {
            return false;
        }
        // An LED takes at least six bytes, which bounds where its list
        // can begin.
        let last_start = colours.saturating_sub(2 + 6 * n);
        (after_name..=last_start).rev().any(|start| word(start) == *n && leds_fill(*n, start + 2, colours))
    })?;
    Some((name, leds as u32))
}

/// Asks OpenRGB what devices it has. Blocks for at most a few times
/// `timeout`.
pub fn list(host: &str, port: u16, timeout: Duration) -> Result<Vec<Controller>, String> {
    let addr = (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.find(SocketAddr::is_ipv4))
        .ok_or_else(|| format!("could not find {host} on the network"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|_| {
        format!(
            "OpenRGB did not answer at {host}:{port}. Start OpenRGB there and switch on its SDK server (the \"SDK Server\" tab, \"Start Server\")."
        )
    })?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let _ = stream.set_nodelay(true);
    let mut packet = Vec::new();
    greeting(&mut packet, None);
    let wrote = |stream: &mut TcpStream, packet: &[u8]| {
        stream
            .write_all(packet)
            .map_err(|e| format!("OpenRGB stopped answering: {e}"))
    };
    wrote(&mut stream, &packet)?;
    header(&mut packet, 0, REQUEST_CONTROLLER_COUNT, 0);
    wrote(&mut stream, &packet)?;
    let count = answer(&mut stream, REQUEST_CONTROLLER_COUNT)?;
    let count = u32::from_le_bytes(
        count
            .get(..4)
            .and_then(|c| c.try_into().ok())
            .ok_or("OpenRGB sent an answer that was not expected")?,
    );
    let mut found = Vec::new();
    for index in 0..count.min(256) {
        header(&mut packet, index, REQUEST_CONTROLLER_DATA, 0);
        wrote(&mut stream, &packet)?;
        let data = answer(&mut stream, REQUEST_CONTROLLER_DATA)?;
        if let Some((name, leds)) = describe(&data) {
            found.push(Controller { index, name, leds });
        }
    }
    Ok(found)
}

/// What the sending thread and its owner share.
#[derive(Default)]
struct Waiting {
    /// The newest frame not yet sent.
    frame: Option<Vec<u8>>,
    stop: bool,
}

/// A connection to one of OpenRGB's devices, kept up by a thread of its
/// own. Dropping it sends whatever frame is still waiting and ends the
/// thread.
pub struct Link {
    shared: Arc<(Mutex<Waiting>, Condvar)>,
    pub addr: SocketAddr,
    pub device: u32,
}

impl Link {
    pub fn new(addr: SocketAddr, device: u32) -> Self {
        let shared: Arc<(Mutex<Waiting>, Condvar)> = Arc::default();
        let theirs = shared.clone();
        thread::spawn(move || run(&theirs, addr, device));
        Self { shared, addr, device }
    }

    /// Hands over a frame to send, in place of any still waiting.
    pub fn send(&self, rgb: &[u8]) {
        let (waiting, wake) = &*self.shared;
        let mut waiting = waiting.lock().unwrap_or_else(|e| e.into_inner());
        let frame = waiting.frame.get_or_insert_with(Vec::new);
        frame.clear();
        frame.extend_from_slice(rgb);
        wake.notify_one();
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        let (waiting, wake) = &*self.shared;
        waiting.lock().unwrap_or_else(|e| e.into_inner()).stop = true;
        wake.notify_one();
    }
}

fn run(shared: &(Mutex<Waiting>, Condvar), addr: SocketAddr, device: u32) {
    let (waiting, wake) = shared;
    let mut stream: Option<TcpStream> = None;
    let mut packet = Vec::new();
    let mut tried = Instant::now() - Duration::from_secs(10);
    let gap = Duration::from_secs(1) / MAX_RATE;
    loop {
        let (frame, stop) = {
            let mut waiting = waiting.lock().unwrap_or_else(|e| e.into_inner());
            while waiting.frame.is_none() && !waiting.stop {
                waiting = wake.wait(waiting).unwrap_or_else(|e| e.into_inner());
            }
            (waiting.frame.take(), waiting.stop)
        };
        if let Some(frame) = frame {
            // Not connected: try again at most once a second, and let
            // the frames in between go.
            if stream.is_none() && tried.elapsed() >= Duration::from_secs(1) {
                tried = Instant::now();
                stream = TcpStream::connect_timeout(&addr, Duration::from_millis(800)).ok().and_then(|mut s| {
                    let _ = s.set_write_timeout(Some(Duration::from_millis(500)));
                    let _ = s.set_nodelay(true);
                    greeting(&mut packet, Some(device));
                    s.write_all(&packet).ok().map(|()| s)
                });
            }
            if let Some(open) = &mut stream {
                update_leds(&mut packet, device, &frame);
                if open.write_all(&packet).is_err() {
                    stream = None;
                }
            }
        }
        if stop {
            return;
        }
        thread::sleep(gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// A device's description as OpenRGB writes it: in the first version
    /// of the protocol, and with a maker's name after the device's, as
    /// later versions have it.
    fn description(name: &str, leds: u16, with_vendor: bool) -> Vec<u8> {
        let text = |out: &mut Vec<u8>, s: &str| {
            out.extend_from_slice(&(s.len() as u16 + 1).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        };
        let mut d = vec![0u8; 4];
        d.extend_from_slice(&5i32.to_le_bytes());
        text(&mut d, name);
        if with_vendor {
            text(&mut d, "A maker");
        }
        for s in ["A description", "1.0", "SERIAL", "HID: /dev/hidraw3"] {
            text(&mut d, s);
        }
        // One mode with no colours of its own, one zone, then the LEDs.
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&0i32.to_le_bytes());
        text(&mut d, "Direct");
        d.extend_from_slice(&[0u8; 4 * 9]);
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        text(&mut d, "Keys");
        d.extend_from_slice(&0i32.to_le_bytes());
        for _ in 0..3 {
            d.extend_from_slice(&(leds as u32).to_le_bytes());
        }
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&leds.to_le_bytes());
        for i in 0..leds {
            text(&mut d, &format!("LED {i}"));
            d.extend_from_slice(&(i as u32).to_le_bytes());
        }
        d.extend_from_slice(&leds.to_le_bytes());
        for i in 0..leds {
            // Colours that could be mistaken for a count if read carelessly.
            d.extend_from_slice(&[i as u8, 0, (leds - 1 - i) as u8, 0]);
        }
        let size = d.len() as u32;
        d[..4].copy_from_slice(&size.to_le_bytes());
        d
    }

    #[test]
    fn a_devices_name_and_led_count_are_read_whatever_the_version() {
        for with_vendor in [false, true] {
            // Sixteen is the awkward one: its LEDs' names end in digits,
            // and a digit before the end of a name reads as a number.
            for leds in [0u16, 1, 4, 16, 52, 87, 300] {
                let data = description("Shiny keyboard", leds, with_vendor);
                assert_eq!(describe(&data), Some(("Shiny keyboard".into(), leds as u32)), "{leds} LEDs");
            }
        }
        assert_eq!(describe(&[]), None);
        assert_eq!(describe(&[0; 9]), None);
    }

    #[test]
    fn packets_are_laid_out_as_the_document_says() {
        let mut packet = Vec::new();
        update_leds(&mut packet, 3, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(&packet[..4], b"ORGB");
        assert_eq!(packet[4..8], 3u32.to_le_bytes());
        assert_eq!(packet[8..12], 1050u32.to_le_bytes());
        assert_eq!(packet[12..16], 14u32.to_le_bytes());
        assert_eq!(packet[16..20], 14u32.to_le_bytes());
        assert_eq!(packet[20..22], 2u16.to_le_bytes());
        assert_eq!(packet[22..], [1, 2, 3, 0, 4, 5, 6, 0]);
        greeting(&mut packet, Some(2));
        assert_eq!(packet[8..12], 50u32.to_le_bytes());
        assert_eq!(&packet[16..24], b"darkRGB\0");
        assert_eq!(packet[28..32], 2u32.to_le_bytes());
        assert_eq!(packet[32..36], 1100u32.to_le_bytes());
        assert_eq!(packet.len(), 40);
    }

    /// A stand-in for OpenRGB: answers what a client asks, and reports
    /// every packet it is sent.
    fn server(devices: Vec<Vec<u8>>) -> (SocketAddr, mpsc::Receiver<(u32, u32, Vec<u8>)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let (tx, devices) = (tx.clone(), devices.clone());
                thread::spawn(move || {
                    loop {
                        let mut head = [0u8; 16];
                        if stream.read_exact(&mut head).is_err() {
                            return;
                        }
                        let field = |at: usize| u32::from_le_bytes(head[at..at + 4].try_into().unwrap());
                        let (device, id, size) = (field(4), field(8), field(12));
                        let mut data = vec![0u8; size as usize];
                        if stream.read_exact(&mut data).is_err() {
                            return;
                        }
                        let reply = match id {
                            REQUEST_CONTROLLER_COUNT => Some((devices.len() as u32).to_le_bytes().to_vec()),
                            REQUEST_CONTROLLER_DATA => devices.get(device as usize).cloned(),
                            _ => None,
                        };
                        if let Some(reply) = reply {
                            let mut packet = Vec::new();
                            header(&mut packet, device, id, reply.len() as u32);
                            packet.extend_from_slice(&reply);
                            if stream.write_all(&packet).is_err() {
                                return;
                            }
                        }
                        let _ = tx.send((device, id, data));
                    }
                });
            }
        });
        (addr, rx)
    }

    #[test]
    fn devices_are_listed_and_frames_reach_the_one_they_are_for() {
        let (addr, got) = server(vec![
            description("Keyboard", 87, true),
            description("Memory", 8, true),
        ]);
        let found = list("127.0.0.1", addr.port(), Duration::from_secs(2)).unwrap();
        assert_eq!(
            found,
            [
                Controller {
                    index: 0,
                    name: "Keyboard".into(),
                    leds: 87
                },
                Controller {
                    index: 1,
                    name: "Memory".into(),
                    leds: 8
                },
            ]
        );
        // The stand-in reports each packet just after answering it: wait
        // until the listing's own have all come in.
        while got.recv_timeout(Duration::from_millis(300)).is_ok() {}

        // A link says who it is, asks for the device's LEDs, then sends.
        let link = Link::new(addr, 1);
        link.send(&[255, 0, 0, 0, 255, 0]);
        let wait = Duration::from_secs(3);
        let (_, id, name) = got.recv_timeout(wait).unwrap();
        assert_eq!((id, name.as_slice()), (SET_CLIENT_NAME, b"darkRGB\0".as_slice()));
        let (device, id, _) = got.recv_timeout(wait).unwrap();
        assert_eq!((device, id), (1, SET_CUSTOM_MODE));
        let (device, id, data) = got.recv_timeout(wait).unwrap();
        assert_eq!((device, id), (1, UPDATE_LEDS));
        assert_eq!(data[6..], [255, 0, 0, 0, 0, 255, 0, 0]);
        // A frame handed over just before the link is dropped still goes.
        link.send(&[0, 0, 0, 0, 0, 0]);
        drop(link);
        let (_, id, data) = got.recv_timeout(wait).unwrap();
        assert_eq!(id, UPDATE_LEDS);
        assert_eq!(data[6..], [0; 8]);
    }

    #[test]
    fn a_program_that_is_not_there_or_is_not_openrgb_is_said_plainly() {
        // Nothing listening.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let error = list("127.0.0.1", port, Duration::from_millis(500)).unwrap_err();
        assert!(error.contains("SDK server"), "{error}");
        // Something else listening.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
                thread::sleep(Duration::from_millis(300));
            }
        });
        let error = list("127.0.0.1", addr.port(), Duration::from_secs(1)).unwrap_err();
        assert!(error.contains("not OpenRGB") || error.contains("stopped answering"), "{error}");
        // A link to nowhere takes frames and does nothing with them.
        let link = Link::new(SocketAddr::from(([127, 0, 0, 1], port)), 0);
        for _ in 0..5 {
            link.send(&[1, 2, 3]);
        }
    }
}
