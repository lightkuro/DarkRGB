//! Pixel controllers that speak the lighting industry's DMX-over-network
//! protocols: sACN (E1.31) and Art-Net. Both carry "universes" of up to
//! 512 channels. A frame of RGB pixels is cut into universes of 170
//! pixels (510 channels), numbered upward from the device's first.

/// Channels used in each universe.
pub const CHANNELS: usize = darkrgb_core::layout::PIXELS_PER_UNIVERSE as usize * 3;

pub const SACN_PORT: u16 = 5568;
pub const ARTNET_PORT: u16 = 6454;

/// sACN (ANSI E1.31) data packets.
pub mod sacn {
    /// Bytes before the channel data.
    pub const HEADER_LEN: usize = 126;
    const ACN_ID: &[u8; 12] = b"ASC-E1.17\0\0\0";
    /// Writes one data packet for `universe` into `out` (cleared first).
    /// `cid` identifies the sender, `data` is up to 512 channels.
    pub fn packet(out: &mut Vec<u8>, cid: &[u8; 16], source: &str, universe: u16, seq: u8, data: &[u8]) {
        let data = &data[..data.len().min(512)];
        let total = HEADER_LEN + data.len();
        let length = |from: usize| 0x7000u16 | (total - from) as u16;
        out.clear();
        // Root layer.
        out.extend_from_slice(&[0x00, 0x10, 0x00, 0x00]);
        out.extend_from_slice(ACN_ID);
        out.extend_from_slice(&length(16).to_be_bytes());
        out.extend_from_slice(&[0x00, 0x00, 0x00, 0x04]);
        out.extend_from_slice(cid);
        // Framing layer.
        out.extend_from_slice(&length(38).to_be_bytes());
        out.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        let mut name = [0u8; 64];
        let bytes = source.as_bytes();
        // Room is left for the terminating zero the standard asks for.
        name[..bytes.len().min(63)].copy_from_slice(&bytes[..bytes.len().min(63)]);
        out.extend_from_slice(&name);
        out.push(100); // priority, the default
        out.extend_from_slice(&[0x00, 0x00]); // no synchronisation universe
        out.push(seq);
        out.push(0); // options
        out.extend_from_slice(&universe.to_be_bytes());
        // Device management layer.
        out.extend_from_slice(&length(115).to_be_bytes());
        out.push(0x02); // set property
        out.push(0xA1); // address and data type
        out.extend_from_slice(&[0x00, 0x00]); // first address
        out.extend_from_slice(&[0x00, 0x01]); // address step
        out.extend_from_slice(&(data.len() as u16 + 1).to_be_bytes());
        out.push(0x00); // start code: dimmer data
        out.extend_from_slice(data);
    }

    /// Reads a data packet as a receiver would: universe, sequence number
    /// and channel data. `None` for anything else.
    pub fn parse(packet: &[u8]) -> Option<(u16, u8, &[u8])> {
        if packet.len() < HEADER_LEN || &packet[4..16] != ACN_ID {
            return None;
        }
        if packet[18..22] != [0, 0, 0, 4] || packet[40..44] != [0, 0, 0, 2] || packet[117] != 0x02 {
            return None;
        }
        let universe = u16::from_be_bytes([packet[113], packet[114]]);
        let count = u16::from_be_bytes([packet[123], packet[124]]) as usize;
        // The count includes the start code, which must be zero for levels.
        if count == 0 || packet[125] != 0 || packet.len() < HEADER_LEN + count - 1 {
            return None;
        }
        Some((universe, packet[111], &packet[HEADER_LEN..HEADER_LEN + count - 1]))
    }
}

/// Art-Net ArtDmx packets.
pub mod artnet {
    /// Bytes before the channel data.
    pub const HEADER_LEN: usize = 18;
    const ID: &[u8; 8] = b"Art-Net\0";
    /// Writes one ArtDmx packet for `universe` into `out` (cleared
    /// first). `data` is up to 512 channels; an odd length is padded with
    /// a zero, as the protocol wants an even count.
    pub fn packet(out: &mut Vec<u8>, universe: u16, seq: u8, data: &[u8]) {
        let data = &data[..data.len().min(512)];
        let padded = data.len() + data.len() % 2;
        out.clear();
        out.extend_from_slice(ID);
        out.extend_from_slice(&[0x00, 0x50]); // OpDmx, low byte first
        out.extend_from_slice(&[0, 14]); // protocol version
        // Zero means "no sequence", so the counter skips it.
        out.push(if seq == 0 { 1 } else { seq });
        out.push(0); // physical port, for information only
        out.push((universe & 0xFF) as u8);
        out.push((universe >> 8 & 0x7F) as u8);
        out.extend_from_slice(&(padded as u16).to_be_bytes());
        out.extend_from_slice(data);
        out.resize(HEADER_LEN + padded, 0);
    }

    /// Reads an ArtDmx packet: universe, sequence and channel data.
    pub fn parse(packet: &[u8]) -> Option<(u16, u8, &[u8])> {
        if packet.len() < HEADER_LEN || &packet[..8] != ID || packet[8..10] != [0x00, 0x50] {
            return None;
        }
        let universe = packet[14] as u16 | (packet[15] as u16 & 0x7F) << 8;
        let length = u16::from_be_bytes([packet[16], packet[17]]) as usize;
        if packet.len() < HEADER_LEN + length {
            return None;
        }
        Some((universe, packet[12], &packet[HEADER_LEN..HEADER_LEN + length]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_universe_carries_170_pixels() {
        assert_eq!(CHANNELS, 510);
    }

    #[test]
    fn an_sacn_packet_is_laid_out_as_the_standard_says() {
        let cid = [7u8; 16];
        let data: Vec<u8> = (0..510).map(|i| (i % 251) as u8).collect();
        let mut packet = Vec::new();
        sacn::packet(&mut packet, &cid, "darkRGB", 3, 42, &data);
        assert_eq!(packet.len(), 126 + 510);
        // Fixed fields at their offsets.
        assert_eq!(&packet[..4], [0x00, 0x10, 0x00, 0x00]);
        assert_eq!(&packet[4..16], b"ASC-E1.17\0\0\0");
        // Lengths count from their own field to the end, with 0x7 on top.
        assert_eq!(u16::from_be_bytes([packet[16], packet[17]]), 0x7000 | (636 - 16));
        assert_eq!(u16::from_be_bytes([packet[38], packet[39]]), 0x7000 | (636 - 38));
        assert_eq!(u16::from_be_bytes([packet[115], packet[116]]), 0x7000 | (636 - 115));
        assert_eq!(&packet[22..38], cid);
        assert_eq!(&packet[44..51], b"darkRGB");
        assert_eq!(packet[51], 0, "the name ends in a zero");
        assert_eq!(packet[108], 100, "priority");
        assert_eq!(packet[111], 42, "sequence");
        assert_eq!(&packet[113..115], [0, 3], "universe");
        assert_eq!(&packet[123..125], 511u16.to_be_bytes(), "channels plus the start code");
        assert_eq!(packet[125], 0, "start code");
        // And it reads back.
        assert_eq!(sacn::parse(&packet), Some((3, 42, &data[..])));
        // A long name is cut; other packets are not data.
        sacn::packet(&mut packet, &cid, &"x".repeat(200), 1, 0, &[1, 2, 3]);
        assert_eq!(packet[44 + 63], 0);
        assert_eq!(sacn::parse(&packet), Some((1, 0, &[1u8, 2, 3][..])));
        assert_eq!(sacn::parse(&packet[..50]), None);
        assert_eq!(sacn::parse(b"Art-Net\0 not sacn at all, and long enough to look at every field we check in the header ..............................................................."), None);
    }

    #[test]
    fn an_artnet_packet_is_laid_out_as_the_protocol_says() {
        let mut packet = Vec::new();
        artnet::packet(&mut packet, 0x0102, 9, &[10, 20, 30]);
        assert_eq!(&packet[..8], b"Art-Net\0");
        assert_eq!(&packet[8..10], [0x00, 0x50], "OpDmx, low byte first");
        assert_eq!(&packet[10..12], [0, 14]);
        assert_eq!(packet[12], 9);
        assert_eq!(
            (packet[14], packet[15]),
            (0x02, 0x01),
            "universe: low byte, then the net"
        );
        assert_eq!(&packet[16..18], [0, 4], "three channels are padded to four");
        assert_eq!(&packet[18..], [10, 20, 30, 0]);
        assert_eq!(artnet::parse(&packet), Some((0x0102, 9, &[10u8, 20, 30, 0][..])));
        // Sequence zero means "none", so it is never sent.
        artnet::packet(&mut packet, 0, 0, &[1, 2]);
        assert_eq!(packet[12], 1);
        // A full universe.
        artnet::packet(&mut packet, 5, 200, &[9u8; 510]);
        assert_eq!(packet.len(), 18 + 510);
        assert_eq!(
            artnet::parse(&packet).map(|(u, s, d)| (u, s, d.len())),
            Some((5, 200, 510))
        );
        assert_eq!(artnet::parse(b"Art-Net\0\x00\x20 a poll, not dmx"), None);
        assert_eq!(artnet::parse(&packet[..10]), None);
    }
}
