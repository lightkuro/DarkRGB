//! Small pictures for matrices.
//!
//! A picture is kept as text inside the effect's setting, like a palette
//! of the user's own: `16x16:` and then its pixels, three bytes each, in
//! base64. So a scene, a copy or a kept effect carries its picture with
//! nothing to look up. The UI shrinks whatever image is chosen to at most
//! [`MAX_SIDE`] dots a side before sending it, so nothing here ever
//! decodes an image file.

/// The most dots a picture has along either side.
pub const MAX_SIDE: u32 = 64;

/// What a new Picture effect shows: a heart, eight dots square.
pub const DEFAULT: &str = "8x8:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/yhQ/yhQAAAAAAAA/yhQ/yhQAAAA/yhQ/yhQ/yhQ/yhQ/yhQ\
    /yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQ/yhQAAAA/yhQ/yhQ/yhQ/yhQ/yhQ/yhQAAAAAAAAAAAA\
    /yhQ/yhQ/yhQ/yhQAAAAAAAAAAAAAAAAAAAA/yhQ/yhQAAAAAAAAAAAA";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// Red, green, blue for each dot, row by row from the top.
    pub rgb: Vec<u8>,
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n =
            (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut have) = (0u32, 0u32);
    for c in text.bytes() {
        let value = ALPHABET.iter().position(|a| *a == c)? as u32;
        bits = bits << 6 | value;
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
            bits &= (1 << have) - 1;
        }
    }
    Some(out)
}

impl Picture {
    /// Reads a picture from a setting. `None` for anything that is not
    /// one, including one too large.
    pub fn parse(text: &str) -> Option<Self> {
        let (size, pixels) = text.split_once(':')?;
        let (width, height) = size.split_once('x')?;
        let (width, height): (u32, u32) = (width.parse().ok()?, height.parse().ok()?);
        if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
            return None;
        }
        // Checked before decoding, so a huge text is never unpacked.
        let bytes = (width * height * 3) as usize;
        if pixels.len() > bytes.div_ceil(3) * 4 {
            return None;
        }
        let rgb = decode(pixels)?;
        (rgb.len() == bytes).then_some(Self { width, height, rgb })
    }

    /// Writes the picture as a setting holds it.
    pub fn write(&self) -> String {
        format!("{}x{}:{}", self.width, self.height, encode(&self.rgb))
    }

    /// The colour of one dot, as bytes. Black outside the picture.
    pub fn dot(&self, x: u32, y: u32) -> [u8; 3] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0];
        }
        let at = ((y * self.width + x) * 3) as usize;
        [self.rgb[at], self.rgb[at + 1], self.rgb[at + 2]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_survives_being_written_and_read() {
        for (width, height) in [(1, 1), (2, 1), (1, 2), (5, 7), (16, 16), (64, 64)] {
            let rgb: Vec<u8> = (0..width * height * 3).map(|i| (i * 7 % 256) as u8).collect();
            let picture = Picture { width, height, rgb };
            let text = picture.write();
            assert_eq!(Picture::parse(&text), Some(picture), "{width}x{height}");
        }
        // The largest is small enough for one message to the engine.
        let largest = Picture {
            width: 64,
            height: 64,
            rgb: vec![9; 64 * 64 * 3],
        };
        assert!(largest.write().len() < 20_000);
    }

    #[test]
    fn the_default_is_a_heart() {
        let heart = Picture::parse(DEFAULT).expect("the default parses");
        assert_eq!((heart.width, heart.height), (8, 8));
        assert_eq!(heart.dot(0, 0), [0, 0, 0]);
        assert_eq!(heart.dot(1, 1), [255, 40, 80]);
        assert_eq!(heart.dot(3, 7), [255, 40, 80], "the point at the bottom");
        assert_eq!(heart.dot(8, 0), [0, 0, 0], "outside is black");
        assert_eq!(heart.write(), DEFAULT);
    }

    #[test]
    fn anything_else_is_not_a_picture() {
        for bad in [
            "",
            "heart",
            "8x8",
            "8x8:",
            "0x8:AAAA",
            "65x1:AAAA",
            "1x65:AAAA",
            "1x1:AAA",
            "1x1:AAAAAAAA",
            "1x1:A*AA",
            "-1x1:AAAA",
            "1x1x1:AAAA",
            "99999999999x1:AAAA",
        ] {
            assert_eq!(Picture::parse(bad), None, "{bad:?}");
        }
        assert!(Picture::parse("1x1:AAAA").is_some());
        // Padding is accepted, as browsers write it.
        assert!(Picture::parse("2x1:AAAAAAAA").is_some());
        let two = Picture {
            width: 1,
            height: 2,
            rgb: vec![1, 2, 3, 4, 5, 6],
        };
        assert_eq!(Picture::parse(&two.write()), Some(two));
        // A text far longer than its size says is refused unread.
        let huge = format!("1x1:{}", "A".repeat(1_000_000));
        assert_eq!(Picture::parse(&huge), None);
    }
}
