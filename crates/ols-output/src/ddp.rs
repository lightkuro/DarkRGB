//! DDP (Distributed Display Protocol), the pixel stream WLED prefers.
//!
//! A frame is split into packets of at most 1440 data bytes (480 RGB
//! pixels). Each packet says where its data goes in the frame, and the last
//! one carries the push flag, which tells the device to show the frame.
//!
//! Header, 10 bytes:
//!
//! | Byte | Meaning                                        |
//! |------|------------------------------------------------|
//! | 0    | flags: `0x40` version 1, `0x01` push           |
//! | 1    | sequence number 1..15 (0 means "not used")     |
//! | 2    | data type, `0x0B` for 8-bit RGB                |
//! | 3    | destination id, 1 is the default output        |
//! | 4-7  | offset of this packet's data, big-endian       |
//! | 8-9  | length of this packet's data, big-endian       |

use std::io;
use std::net::{SocketAddr, UdpSocket};

pub const DDP_PORT: u16 = 4048;
pub const HEADER_LEN: usize = 10;
/// Largest data payload per packet. Keeps packets under a typical MTU.
pub const MAX_DATA: usize = 1440;

const FLAG_VERSION_1: u8 = 0x40;
const FLAG_PUSH: u8 = 0x01;
const TYPE_RGB8: u8 = 0x0B;
const ID_DISPLAY: u8 = 1;

/// Writes one packet into `buf` and returns its total length.
fn write_packet(buf: &mut [u8; HEADER_LEN + MAX_DATA], seq: u8, offset: usize, data: &[u8], push: bool) -> usize {
    debug_assert!(data.len() <= MAX_DATA);
    buf[0] = FLAG_VERSION_1 | if push { FLAG_PUSH } else { 0 };
    buf[1] = seq & 0x0F;
    buf[2] = TYPE_RGB8;
    buf[3] = ID_DISPLAY;
    buf[4..8].copy_from_slice(&(offset as u32).to_be_bytes());
    buf[8..10].copy_from_slice(&(data.len() as u16).to_be_bytes());
    buf[HEADER_LEN..HEADER_LEN + data.len()].copy_from_slice(data);
    HEADER_LEN + data.len()
}

/// Splits a frame of RGB bytes into DDP packets.
pub fn packets(frame: &[u8], seq: u8) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(frame.len() / MAX_DATA + 1);
    let mut buf = [0u8; HEADER_LEN + MAX_DATA];
    let chunks = frame.len().div_ceil(MAX_DATA).max(1);
    for i in 0..chunks {
        let start = i * MAX_DATA;
        let end = (start + MAX_DATA).min(frame.len());
        let n = write_packet(&mut buf, seq, start, &frame[start..end], i + 1 == chunks);
        out.push(buf[..n].to_vec());
    }
    out
}

/// A decoded packet, as a receiver sees it.
#[derive(Debug, PartialEq, Eq)]
pub struct Packet<'a> {
    pub push: bool,
    pub seq: u8,
    pub offset: usize,
    pub data: &'a [u8],
}

/// Decodes a packet. Returns `None` for anything that is not DDP version 1
/// RGB pixel data, or whose length field disagrees with its size.
pub fn parse(bytes: &[u8]) -> Option<Packet<'_>> {
    if bytes.len() < HEADER_LEN || bytes[0] & 0xC0 != FLAG_VERSION_1 {
        return None;
    }
    // A timecode flag adds four bytes we do not use.
    let header = if bytes[0] & 0x10 != 0 { HEADER_LEN + 4 } else { HEADER_LEN };
    let offset = u32::from_be_bytes(bytes[4..8].try_into().ok()?) as usize;
    let len = u16::from_be_bytes(bytes[8..10].try_into().ok()?) as usize;
    let data = bytes.get(header..header + len)?;
    Some(Packet {
        push: bytes[0] & FLAG_PUSH != 0,
        seq: bytes[1] & 0x0F,
        offset,
        data,
    })
}

/// Sends frames to DDP devices from one UDP socket.
pub struct Sender {
    socket: UdpSocket,
    seq: u8,
    buf: Box<[u8; HEADER_LEN + MAX_DATA]>,
}

impl Sender {
    pub fn new() -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        // Never let a full send buffer stall the frame clock. A dropped
        // frame is better than a late one.
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            seq: 0,
            buf: Box::new([0u8; HEADER_LEN + MAX_DATA]),
        })
    }

    /// Sends one frame. Returns the number of packets sent.
    pub fn send_frame(&mut self, addr: SocketAddr, frame: &[u8]) -> io::Result<usize> {
        self.seq = if self.seq >= 15 { 1 } else { self.seq + 1 };
        let chunks = frame.len().div_ceil(MAX_DATA).max(1);
        for i in 0..chunks {
            let start = i * MAX_DATA;
            let end = (start + MAX_DATA).min(frame.len());
            let n = write_packet(&mut self.buf, self.seq, start, &frame[start..end], i + 1 == chunks);
            self.socket.send_to(&self.buf[..n], addr)?;
        }
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_frame_is_one_pushed_packet() {
        let frame = [1u8, 2, 3, 4, 5, 6];
        let p = packets(&frame, 3);
        assert_eq!(p.len(), 1);
        // Checked against the header layout WLED documents, byte by byte.
        assert_eq!(p[0], [0x41, 0x03, 0x0B, 0x01, 0, 0, 0, 0, 0, 6, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn large_frame_splits_at_480_pixels_and_pushes_last() {
        let frame: Vec<u8> = (0..1000 * 3).map(|i| (i % 251) as u8).collect();
        let p = packets(&frame, 1);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].len(), HEADER_LEN + 1440);
        assert_eq!(p[0][0], 0x40, "first packet must not push");
        assert_eq!(p[1][0], 0x40);
        assert_eq!(p[2][0], 0x41, "last packet pushes");
        assert_eq!(&p[1][4..8], &1440u32.to_be_bytes());
        assert_eq!(&p[2][4..8], &2880u32.to_be_bytes());
        assert_eq!(&p[2][8..10], &120u16.to_be_bytes());

        let mut rebuilt = vec![0u8; frame.len()];
        for raw in &p {
            let packet = parse(raw).unwrap();
            rebuilt[packet.offset..packet.offset + packet.data.len()].copy_from_slice(packet.data);
        }
        assert_eq!(rebuilt, frame);
    }

    #[test]
    fn exact_multiple_does_not_add_an_empty_packet() {
        let p = packets(&vec![7u8; 2880], 1);
        assert_eq!(p.len(), 2);
        assert_eq!(p[1][0], 0x41);
    }

    #[test]
    fn empty_frame_still_pushes() {
        let p = packets(&[], 1);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0], [0x41, 0x01, 0x0B, 0x01, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn parse_rejects_junk() {
        assert!(parse(&[]).is_none());
        assert!(parse(&[0x41, 1, 0x0B, 1, 0, 0, 0, 0, 0, 9, 1, 2]).is_none(), "length past the end");
        assert!(parse(&[0x01, 1, 0x0B, 1, 0, 0, 0, 0, 0, 0]).is_none(), "wrong version");
    }

    #[test]
    fn parse_skips_a_timecode() {
        let raw = [0x51, 1, 0x0B, 1, 0, 0, 0, 0, 0, 3, 9, 9, 9, 9, 1, 2, 3];
        let packet = parse(&raw).unwrap();
        assert_eq!(packet.data, [1, 2, 3]);
        assert!(packet.push);
    }

    #[test]
    fn sender_delivers_frames_over_udp() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let addr = receiver.local_addr().unwrap();
        let mut sender = Sender::new().unwrap();
        let frame: Vec<u8> = (0..600 * 3).map(|i| (i % 200) as u8).collect();
        assert_eq!(sender.send_frame(addr, &frame).unwrap(), 2);

        let mut rebuilt = vec![0u8; frame.len()];
        let mut buf = [0u8; 2048];
        let mut pushed = false;
        for _ in 0..2 {
            let (n, _) = receiver.recv_from(&mut buf).unwrap();
            let packet = parse(&buf[..n]).unwrap();
            assert_eq!(packet.seq, 1);
            rebuilt[packet.offset..packet.offset + packet.data.len()].copy_from_slice(packet.data);
            pushed |= packet.push;
        }
        assert!(pushed);
        assert_eq!(rebuilt, frame);
    }
}
