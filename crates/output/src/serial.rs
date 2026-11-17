//! Strips on a serial port, sent the Adalight protocol.
//!
//! Adalight is what most Arduino and ESP sketches for LED strips listen
//! for, and WLED takes it on its USB port too. Each frame is a header of
//! six bytes, `A d a`, the number of LEDs less one (high byte, low byte)
//! and a check byte (the two XORed with 0x55), then three bytes a LED.
//!
//! Not yet tried against a real strip: written from the protocol as the
//! Adalight sketch and WLED read it.

use std::io::Write;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Writes one Adalight frame of `rgb` (three bytes a LED) into `out`.
pub fn frame(out: &mut Vec<u8>, rgb: &[u8]) {
    let leds = (rgb.len() / 3).max(1);
    let count = (leds - 1).min(u16::MAX as usize) as u16;
    let [hi, lo] = count.to_be_bytes();
    out.clear();
    out.extend_from_slice(&[b'A', b'd', b'a', hi, lo, hi ^ lo ^ 0x55]);
    out.extend_from_slice(&rgb[..(leds * 3).min(rgb.len())]);
}

/// The most frames a second a port can carry for `leds` LEDs: a byte on
/// the wire takes ten bits.
pub fn most_frames(baud: u32, leds: u32) -> f32 {
    baud as f32 / 10.0 / (6.0 + 3.0 * leds as f32)
}

/// Whether a port's name says it is on USB, where boards that drive
/// strips are, rather than built into the computer.
pub fn looks_usb(name: &str) -> bool {
    ["ttyUSB", "ttyACM", "cu.usb", "tty.usb"]
        .iter()
        .any(|p| name.contains(p))
}

/// The serial ports on this computer, by name (`/dev/ttyUSB0`, `COM3`),
/// with whether each is on USB. Those on USB come first.
pub fn ports() -> Vec<(String, bool)> {
    let mut found: Vec<(String, bool)> = serialport::available_ports()
        .map(|list| {
            list.into_iter()
                .map(|p| {
                    let usb = matches!(p.port_type, serialport::SerialPortType::UsbPort(_)) || looks_usb(&p.port_name);
                    (p.port_name, usb)
                })
                .collect()
        })
        .unwrap_or_default();
    // Without the system's device list, look for the usual names.
    #[cfg(unix)]
    if !found.iter().any(|(_, usb)| *usb)
        && let Ok(dir) = std::fs::read_dir("/dev")
    {
        for entry in dir.flatten() {
            let name = format!("/dev/{}", entry.file_name().to_string_lossy());
            if looks_usb(&name) {
                found.push((name, true));
            }
        }
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    found.dedup_by(|a, b| a.0 == b.0);
    found
}

#[derive(Default)]
struct Waiting {
    frame: Option<Vec<u8>>,
    stop: bool,
}

/// A port kept open by a thread of its own. Frames handed over while one
/// is still being written replace each other, so a slow port shows the
/// newest picture and never falls behind. Dropping it ends the thread.
pub struct Link {
    shared: Arc<(Mutex<Waiting>, Condvar)>,
    pub port: String,
    pub baud: u32,
}

impl Link {
    pub fn new(port: &str, baud: u32) -> Self {
        let shared: Arc<(Mutex<Waiting>, Condvar)> = Arc::default();
        let theirs = shared.clone();
        let (name, speed) = (port.to_string(), baud);
        thread::spawn(move || run(&theirs, &name, speed));
        Self {
            shared,
            port: port.to_string(),
            baud,
        }
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

fn run(shared: &(Mutex<Waiting>, Condvar), port: &str, baud: u32) {
    let (waiting, wake) = shared;
    let mut open: Option<Box<dyn serialport::SerialPort>> = None;
    let mut tried: Option<Instant> = None;
    let mut warned = false;
    let mut packet = Vec::new();
    loop {
        let (frame, stop) = {
            let mut waiting = waiting.lock().unwrap_or_else(|e| e.into_inner());
            while waiting.frame.is_none() && !waiting.stop {
                waiting = wake.wait(waiting).unwrap_or_else(|e| e.into_inner());
            }
            (waiting.frame.take(), waiting.stop)
        };
        if let Some(frame) = frame {
            // Not open: try again every two seconds, letting the frames
            // in between go. A board that has just been plugged in, or
            // that restarts when its port opens, is found again.
            if open.is_none() && tried.is_none_or(|t| t.elapsed() >= Duration::from_secs(2)) {
                tried = Some(Instant::now());
                match serialport::new(port, baud).timeout(Duration::from_millis(500)).open() {
                    Ok(p) => {
                        log::info!("serial: {port} open at {baud}");
                        warned = false;
                        open = Some(p);
                    }
                    Err(e) if !warned => {
                        log::warn!("serial: cannot open {port}: {e}");
                        warned = true;
                    }
                    Err(_) => {}
                }
            }
            if let Some(p) = &mut open {
                self::frame(&mut packet, &frame);
                if p.write_all(&packet).and_then(|()| p.flush()).is_err() {
                    log::warn!("serial: {port} stopped taking frames");
                    open = None;
                }
            }
        }
        if stop {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_starts_with_the_adalight_header() {
        let mut out = Vec::new();
        // 300 LEDs: count 299 = 0x012B, check 0x01 ^ 0x2B ^ 0x55 = 0x7F.
        let rgb = vec![7u8; 900];
        frame(&mut out, &rgb);
        assert_eq!(&out[..6], &[b'A', b'd', b'a', 0x01, 0x2B, 0x7F]);
        assert_eq!(out.len(), 906);
        assert!(out[6..].iter().all(|b| *b == 7));
        // One LED: a count of nought.
        frame(&mut out, &[1, 2, 3]);
        assert_eq!(out, [b'A', b'd', b'a', 0, 0, 0x55, 1, 2, 3]);
    }

    #[test]
    fn a_slow_port_carries_fewer_frames() {
        // 115200 baud, 60 LEDs: 11520 bytes a second, 186 a frame.
        assert!((most_frames(115_200, 60) - 61.9).abs() < 0.1);
        assert!(most_frames(115_200, 300) < 13.0);
        assert!(most_frames(2_000_000, 300) > 200.0);
    }

    #[test]
    fn a_port_that_is_not_there_is_tried_quietly_and_let_go() {
        let link = Link::new("/dev/no-such-port-darkrgb", 115_200);
        link.send(&[255, 0, 0]);
        thread::sleep(Duration::from_millis(50));
        drop(link);
    }
}
