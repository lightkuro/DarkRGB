//! Short moving pictures for matrices: what a GIF or a piece of video is
//! turned into before an effect plays it.
//!
//! A movie is a run of small frames, each with how long it stays up. It
//! is too big to keep inside a setting the way a still picture is, so a
//! setting holds only the movie's id and the frames are kept here, in a
//! store shared by every effect. The engine fills the store from the
//! files it keeps; nothing in this crate reads a file or decodes a GIF.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// The most dots a frame has along either side.
pub const MAX_SIDE: u32 = 64;
/// The most frames in one movie.
pub const MAX_FRAMES: usize = 3600;
/// The most a movie's frames may take up in all, unpacked.
pub const MAX_BYTES: usize = 24 * 1024 * 1024;
/// The longest a single frame stays up, and the shortest, in milliseconds.
pub const MAX_DELAY_MS: u16 = 10_000;
pub const MIN_DELAY_MS: u16 = 10;
/// The id of the movie that plays when none has been chosen.
pub const BUILT_IN: &str = "builtin";

const MAGIC: &[u8; 8] = b"DRGBMOV1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Movie {
    /// What the file it came from was called.
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Milliseconds each frame stays up.
    pub delays: Vec<u16>,
    /// Red, green, blue for each dot, row by row from the top, frame
    /// after frame.
    pub rgb: Vec<u8>,
}

impl Movie {
    pub fn frames(&self) -> usize {
        self.delays.len()
    }

    /// How long one showing lasts, in seconds.
    pub fn seconds(&self) -> f32 {
        self.delays.iter().map(|d| *d as f32).sum::<f32>() / 1000.0
    }

    /// Whether the sizes add up and are within what is allowed.
    pub fn sound(&self) -> bool {
        let frame = (self.width * self.height * 3) as usize;
        (1..=MAX_SIDE).contains(&self.width)
            && (1..=MAX_SIDE).contains(&self.height)
            && (1..=MAX_FRAMES).contains(&self.frames())
            && self.rgb.len() == frame * self.frames()
            && self.rgb.len() <= MAX_BYTES
            && self.delays.iter().all(|d| (MIN_DELAY_MS..=MAX_DELAY_MS).contains(d))
    }

    /// The colour of one dot of one frame.
    pub fn dot(&self, frame: usize, x: u32, y: u32) -> [u8; 3] {
        let (x, y) = (x.min(self.width - 1), y.min(self.height - 1));
        let at = (frame.min(self.frames() - 1) * (self.width * self.height) as usize + (y * self.width + x) as usize) * 3;
        [self.rgb[at], self.rgb[at + 1], self.rgb[at + 2]]
    }

    /// The frame that is up `seconds` into a showing, going round.
    pub fn frame_at(&self, seconds: f32) -> usize {
        let total = self.seconds();
        if total <= 0.0 || !seconds.is_finite() {
            return 0;
        }
        let mut left = seconds.rem_euclid(total) * 1000.0;
        for (i, delay) in self.delays.iter().enumerate() {
            left -= *delay as f32;
            if left < 0.0 {
                return i;
            }
        }
        self.frames() - 1
    }

    /// The movie as it is kept in a file and sent by the UI: a mark, the
    /// name, the size, the number of frames, how long each stays up, and
    /// then the frames.
    pub fn write(&self) -> Vec<u8> {
        let name = self.name.as_bytes();
        let name = &name[..name.len().min(255)];
        let mut out = Vec::with_capacity(32 + name.len() + self.delays.len() * 2 + self.rgb.len());
        out.extend_from_slice(MAGIC);
        out.push(name.len() as u8);
        out.extend_from_slice(name);
        out.extend_from_slice(&(self.width as u16).to_le_bytes());
        out.extend_from_slice(&(self.height as u16).to_le_bytes());
        out.extend_from_slice(&(self.frames() as u32).to_le_bytes());
        for delay in &self.delays {
            out.extend_from_slice(&delay.to_le_bytes());
        }
        out.extend_from_slice(&self.rgb);
        out
    }

    /// Reads what [`Movie::write`] wrote. `None` for anything else, and
    /// for a movie whose sizes do not add up. Frames that stay up for
    /// too short or too long a time are brought within what is allowed.
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(MAGIC)?;
        let (len, rest) = rest.split_first()?;
        if rest.len() < *len as usize + 8 {
            return None;
        }
        let (name, rest) = rest.split_at(*len as usize);
        let name = String::from_utf8_lossy(name).into_owned();
        let width = u16::from_le_bytes([rest[0], rest[1]]) as u32;
        let height = u16::from_le_bytes([rest[2], rest[3]]) as u32;
        let frames = u32::from_le_bytes([rest[4], rest[5], rest[6], rest[7]]) as usize;
        let rest = &rest[8..];
        if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) || !(1..=MAX_FRAMES).contains(&frames) {
            return None;
        }
        // Checked before anything is copied, so a file that lies about
        // its size is never unpacked.
        if rest.len() != frames * 2 + frames * (width * height * 3) as usize {
            return None;
        }
        let (delays, rgb) = rest.split_at(frames * 2);
        let delays = delays
            .chunks_exact(2)
            .map(|d| u16::from_le_bytes([d[0], d[1]]).clamp(MIN_DELAY_MS, MAX_DELAY_MS))
            .collect();
        let movie = Self {
            name,
            width,
            height,
            delays,
            rgb: rgb.to_vec(),
        };
        movie.sound().then_some(movie)
    }

    /// A name for the movie made from what is in it, so the same movie
    /// chosen twice is kept once. Only tells movies apart; keeps nothing
    /// secret.
    pub fn id(&self) -> String {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut feed = |bytes: &[u8]| {
            for b in bytes {
                hash = (hash ^ *b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        };
        feed(&(self.width as u16).to_le_bytes());
        feed(&(self.height as u16).to_le_bytes());
        for delay in &self.delays {
            feed(&delay.to_le_bytes());
        }
        feed(&self.rgb);
        format!("{hash:016x}")
    }

    /// What plays when no movie has been chosen: a ball that bounces
    /// round a small frame, changing colour as it goes.
    fn built_in() -> Self {
        const SIDE: u32 = 16;
        const FRAMES: usize = 32;
        let mut rgb = Vec::with_capacity(FRAMES * (SIDE * SIDE * 3) as usize);
        for frame in 0..FRAMES {
            let turn = frame as f32 / FRAMES as f32 * std::f32::consts::TAU;
            let (bx, by) = (7.5 + 5.0 * turn.cos(), 7.5 + 5.0 * (2.0 * turn).sin());
            let colour = crate::color::Rgb::from_hsv(frame as f32 / FRAMES as f32, 0.9, 1.0);
            for y in 0..SIDE {
                for x in 0..SIDE {
                    let away = ((x as f32 - bx).powi(2) + (y as f32 - by).powi(2)).sqrt();
                    let glow = (1.0 - away / 3.5).clamp(0.0, 1.0);
                    rgb.extend_from_slice(&colour.scale(glow * glow).to_bytes());
                }
            }
        }
        Self {
            name: "Bouncing ball".into(),
            width: SIDE,
            height: SIDE,
            delays: vec![60; FRAMES],
            rgb,
        }
    }
}

/// True for text that could be a movie's id: what [`Movie::id`] gives,
/// or the built-in one's.
pub fn id_ok(id: &str) -> bool {
    id == BUILT_IN || (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
}

fn store() -> &'static RwLock<HashMap<String, Arc<Movie>>> {
    static STORE: OnceLock<RwLock<HashMap<String, Arc<Movie>>>> = OnceLock::new();
    STORE.get_or_init(|| {
        let mut movies = HashMap::new();
        movies.insert(BUILT_IN.to_string(), Arc::new(Movie::built_in()));
        RwLock::new(movies)
    })
}

/// The movie kept under an id, if there is one.
pub fn get(id: &str) -> Option<Arc<Movie>> {
    store().read().unwrap_or_else(|e| e.into_inner()).get(id).cloned()
}

/// Keeps a movie for effects to play. Returns the id it is kept under.
pub fn put(movie: Movie) -> String {
    let id = movie.id();
    store()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id.clone(), Arc::new(movie));
    id
}

/// Forgets a movie. The built-in one stays.
pub fn remove(id: &str) -> bool {
    id != BUILT_IN && store().write().unwrap_or_else(|e| e.into_inner()).remove(id).is_some()
}

/// Every movie that is kept, apart from the built-in one: its id and the
/// movie, in order of name.
pub fn all() -> Vec<(String, Arc<Movie>)> {
    let mut movies: Vec<(String, Arc<Movie>)> = store()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(id, _)| id.as_str() != BUILT_IN)
        .map(|(id, movie)| (id.clone(), movie.clone()))
        .collect();
    movies.sort_by(|a, b| (a.1.name.to_lowercase(), &a.0).cmp(&(b.1.name.to_lowercase(), &b.0)));
    movies
}

/// Shrinks frames to fit within [`MAX_SIDE`] dots a side, keeping their
/// shape. Each dot of the result is the average of the dots it covers.
/// `rgb` holds `frames` frames of `width` by `height`. Returns the new
/// width and height with the new frames.
pub fn shrink(width: u32, height: u32, frames: usize, rgb: &[u8]) -> (u32, u32, Vec<u8>) {
    let longest = width.max(height);
    if longest <= MAX_SIDE {
        return (width, height, rgb.to_vec());
    }
    let (w, h) = (
        (width * MAX_SIDE).div_ceil(longest).clamp(1, MAX_SIDE),
        (height * MAX_SIDE).div_ceil(longest).clamp(1, MAX_SIDE),
    );
    let mut out = Vec::with_capacity(frames * (w * h * 3) as usize);
    let frame_len = (width * height * 3) as usize;
    for frame in 0..frames {
        let from = &rgb[frame * frame_len..(frame + 1) * frame_len];
        for y in 0..h {
            let (top, bottom) = (y * height / h, ((y + 1) * height).div_ceil(h).max(y * height / h + 1));
            for x in 0..w {
                let (left, right) = (x * width / w, ((x + 1) * width).div_ceil(w).max(x * width / w + 1));
                let mut sum = [0u32; 3];
                for sy in top..bottom.min(height) {
                    for sx in left..right.min(width) {
                        let at = ((sy * width + sx) * 3) as usize;
                        for c in 0..3 {
                            sum[c] += from[at + c] as u32;
                        }
                    }
                }
                let count = ((bottom.min(height) - top) * (right.min(width) - left)).max(1);
                out.extend(sum.map(|s| (s / count) as u8));
            }
        }
    }
    (w, h, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_frames() -> Movie {
        Movie {
            name: "Blink".into(),
            width: 2,
            height: 1,
            delays: vec![100, 300],
            rgb: vec![255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255],
        }
    }

    #[test]
    fn a_movie_round_trips_and_anything_else_is_refused() {
        let movie = two_frames();
        let bytes = movie.write();
        assert_eq!(Movie::read(&bytes), Some(movie.clone()));
        // The same frames give the same id whatever the name.
        let mut renamed = movie.clone();
        renamed.name = "Other".into();
        assert_eq!(renamed.id(), movie.id());
        assert!(id_ok(&movie.id()) && id_ok(BUILT_IN));
        assert!(!id_ok("../../etc") && !id_ok("ABCDEF0123456789") && !id_ok(""));
        // Cut short, padded, mislabelled or claiming to be huge.
        assert_eq!(Movie::read(&bytes[..bytes.len() - 1]), None);
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(Movie::read(&longer), None);
        assert_eq!(Movie::read(b"GIF89a"), None);
        assert_eq!(Movie::read(&[]), None);
        let mut huge = bytes.clone();
        let at = MAGIC.len() + 1 + 5;
        huge[at..at + 2].copy_from_slice(&5000u16.to_le_bytes());
        assert_eq!(Movie::read(&huge), None);
        // A frame that would never change, or flicker faster than any
        // light can show, is brought within reason.
        let mut odd = movie.clone();
        odd.delays = vec![0, 60_000];
        let read = Movie::read(&odd.write()).unwrap();
        assert_eq!(read.delays, [MIN_DELAY_MS, MAX_DELAY_MS]);
    }

    #[test]
    fn frames_are_found_by_time_and_go_round() {
        let movie = two_frames();
        assert!((movie.seconds() - 0.4).abs() < 1e-6);
        assert_eq!(movie.frame_at(0.0), 0);
        assert_eq!(movie.frame_at(0.09), 0);
        assert_eq!(movie.frame_at(0.11), 1);
        assert_eq!(movie.frame_at(0.39), 1);
        assert_eq!(movie.frame_at(0.41), 0, "round again");
        assert_eq!(movie.frame_at(-0.05), 1, "and backwards");
        assert_eq!(movie.frame_at(f32::NAN), 0);
        assert_eq!(movie.dot(0, 0, 0), [255, 0, 0]);
        assert_eq!(movie.dot(1, 1, 0), [0, 0, 255]);
        assert_eq!(movie.dot(9, 9, 9), [0, 0, 255], "past the end is the last");
    }

    #[test]
    fn the_store_keeps_movies_and_always_has_one_to_play() {
        let built_in = get(BUILT_IN).unwrap();
        assert!(built_in.sound());
        assert!(!remove(BUILT_IN));
        let id = put(two_frames());
        assert_eq!(get(&id).unwrap().name, "Blink");
        assert!(all().iter().any(|(kept, _)| *kept == id));
        assert!(all().iter().all(|(kept, _)| kept != BUILT_IN));
        assert!(remove(&id));
        assert!(get(&id).is_none() && !remove(&id));
    }

    #[test]
    fn big_frames_are_shrunk_to_fit_keeping_their_shape() {
        // 128 by 64: the left half red, the right half blue.
        let (w, h) = (128u32, 64u32);
        let mut rgb = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(if x < w / 2 { &[255, 0, 0] } else { &[0, 0, 255] });
            }
        }
        let (sw, sh, small) = shrink(w, h, 1, &rgb);
        assert_eq!((sw, sh), (64, 32));
        assert_eq!(small.len(), 64 * 32 * 3);
        assert_eq!(&small[..3], [255, 0, 0]);
        assert_eq!(&small[(63 * 3)..(64 * 3)], [0, 0, 255]);
        // Sizes that do not divide evenly still fit, and average.
        let (sw, sh, small) = shrink(100, 3, 1, &vec![90u8; 100 * 3 * 3]);
        assert_eq!((sw, sh), (64, 2));
        assert!(small.iter().all(|v| *v == 90));
        // Small enough already: left alone.
        assert_eq!(shrink(2, 1, 2, &two_frames().rgb), (2, 1, two_frames().rgb));
    }
}
