//! The colours on the computer's screen, as the window that shares it
//! sends them: a small picture, a few times a second.
//!
//! The screen is shared by the window (or a browser showing darkRGB),
//! through the browser's own prompt, so the program needs no capture code
//! of its own for each system. The newest picture is kept here for the
//! "Screen colours" effect; it is forgotten once it is a few seconds old,
//! so the lights do not hold a frozen screen after sharing stops.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::color::Rgb;

/// The largest picture taken, either way.
pub const MAX_SIDE: u32 = 128;
/// A picture older than this is taken to mean the screen is no longer shared.
pub const STALE: Duration = Duration::from_secs(3);

/// One picture of the screen, row by row from the top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Rgb>,
}

impl Screen {
    /// A picture from bytes, three a pixel. None if the sizes do not agree.
    pub fn from_rgb(width: u32, height: u32, bytes: &[u8]) -> Option<Self> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        if bytes.len() != (width * height * 3) as usize {
            return None;
        }
        let pixels = bytes
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| Rgb::new(p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0))
            .collect();
        Some(Self { width, height, pixels })
    }

    pub fn at(&self, col: u32, row: u32) -> Rgb {
        let (c, r) = (col.min(self.width - 1), row.min(self.height - 1));
        self.pixels[(r * self.width + c) as usize]
    }

    /// The mean colour of the columns from `x0` to `x1` and the rows from
    /// `y0` to `y1`, each a fraction of the screen.
    pub fn mean(&self, x0: f32, x1: f32, y0: f32, y1: f32) -> Rgb {
        let cols = |a: f32, b: f32, n: u32| {
            let first = ((a.clamp(0.0, 1.0) * n as f32) as u32).min(n - 1);
            let last = ((b.clamp(0.0, 1.0) * n as f32).ceil() as u32).clamp(first + 1, n);
            first..last
        };
        let (mut sum, mut count) = (Rgb::BLACK, 0.0f32);
        for row in cols(y0.min(y1), y0.max(y1), self.height) {
            for col in cols(x0.min(x1), x0.max(x1), self.width) {
                sum = sum.add(self.at(col, row));
                count += 1.0;
            }
        }
        sum.scale(1.0 / count.max(1.0))
    }
}

static LATEST: RwLock<Option<(Instant, Arc<Screen>)>> = RwLock::new(None);

/// Keeps a new picture of the screen.
pub fn set(screen: Screen) {
    *LATEST.write().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), Arc::new(screen)));
}

/// Forgets the screen, as when sharing stops.
pub fn clear() {
    *LATEST.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The newest picture, unless it is too old to show.
pub fn latest() -> Option<Arc<Screen>> {
    let latest = LATEST.read().unwrap_or_else(|e| e.into_inner());
    latest
        .as_ref()
        .filter(|(at, _)| at.elapsed() < STALE)
        .map(|(_, screen)| screen.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_read_from_bytes_and_averaged_by_area() {
        // Two by two: red, green on top; blue, white below.
        let bytes = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let s = Screen::from_rgb(2, 2, &bytes).unwrap();
        assert_eq!(s.at(1, 0), Rgb::new(0.0, 1.0, 0.0));
        assert_eq!(s.mean(0.0, 0.5, 0.0, 0.5), Rgb::new(1.0, 0.0, 0.0));
        assert_eq!(s.mean(0.0, 1.0, 0.5, 1.0), Rgb::new(0.5, 0.5, 1.0));
        let all = s.mean(0.0, 1.0, 0.0, 1.0);
        assert!((all.r - 0.5).abs() < 1e-6 && (all.g - 0.5).abs() < 1e-6 && (all.b - 0.5).abs() < 1e-6);
        assert!(Screen::from_rgb(2, 2, &bytes[..9]).is_none());
        assert!(Screen::from_rgb(0, 2, &[]).is_none());
        assert!(Screen::from_rgb(MAX_SIDE + 1, 1, &vec![0; (MAX_SIDE as usize + 1) * 3]).is_none());
    }
}
