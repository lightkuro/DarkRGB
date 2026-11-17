//! Moving pictures: GIFs and pieces of video kept on this computer for
//! the Moving picture effect.
//!
//! A GIF is decoded here. A video is not: the page that sends it has a
//! video player built in, so it plays the file, shrinks each frame and
//! sends the frames ready-made (see `darkrgb_core::movie`). Either way
//! what is kept is a file of small frames named after its contents.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use darkrgb_core::movie::{self, MAX_BYTES, MAX_DELAY_MS, MAX_FRAMES, MIN_DELAY_MS, Movie};

use crate::app::Shared;
use crate::commands;

/// The largest file that is looked at.
pub const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;
/// The largest a GIF's own picture may be along either side before it is
/// shrunk. Larger ones are refused instead of being unpacked.
const MAX_GIF_SIDE: usize = 2048;
/// How long a GIF frame stays up when the file says "no time at all",
/// as browsers have it.
const DEFAULT_DELAY_MS: u16 = 100;
/// What a kept movie's file name ends in.
const EXT: &str = "drgbmov";

/// Turns a GIF into a movie: every frame as it would be seen, shrunk to
/// fit. A GIF with more frames than a movie holds is cut short.
pub fn from_gif(name: &str, bytes: &[u8]) -> Result<Movie, String> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options
        .read_info(Cursor::new(bytes))
        .map_err(|e| format!("that does not look like a GIF: {e}"))?;
    let (width, height) = (decoder.width() as usize, decoder.height() as usize);
    if width == 0 || height == 0 || width > MAX_GIF_SIDE || height > MAX_GIF_SIDE {
        return Err(format!(
            "that GIF is {width} by {height}; at most {MAX_GIF_SIDE} a side can be read"
        ));
    }
    // The picture as it stands, which each frame draws over part of.
    let mut canvas = vec![0u8; width * height * 4];
    let mut delays = Vec::new();
    let mut rgb = Vec::new();
    let (mut small_w, mut small_h) = (0u32, 0u32);
    let mut flat = vec![0u8; width * height * 3];
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|e| format!("the GIF could not be read to its end: {e}"))?
    {
        let (left, top) = (frame.left as usize, frame.top as usize);
        let (fw, fh) = (frame.width as usize, frame.height as usize);
        let before = (frame.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        for y in 0..fh.min(height.saturating_sub(top)) {
            for x in 0..fw.min(width.saturating_sub(left)) {
                let from = (y * fw + x) * 4;
                // A see-through dot leaves what was there.
                if frame.buffer[from + 3] > 0 {
                    let to = ((top + y) * width + left + x) * 4;
                    canvas[to..to + 4].copy_from_slice(&frame.buffer[from..from + 4]);
                }
            }
        }
        for (dot, out) in canvas.chunks_exact(4).zip(flat.chunks_exact_mut(3)) {
            // Nothing drawn yet shows as black.
            out.copy_from_slice(if dot[3] > 0 { &dot[..3] } else { &[0, 0, 0] });
        }
        let (w, h, small) = movie::shrink(width as u32, height as u32, 1, &flat);
        (small_w, small_h) = (w, h);
        if delays.len() >= MAX_FRAMES || rgb.len() + small.len() > MAX_BYTES {
            break;
        }
        rgb.extend_from_slice(&small);
        // GIFs count in hundredths of a second.
        let delay = match frame.delay {
            0 | 1 => DEFAULT_DELAY_MS,
            hundredths => hundredths.saturating_mul(10),
        };
        delays.push(delay.clamp(MIN_DELAY_MS, MAX_DELAY_MS));
        match frame.dispose {
            gif::DisposalMethod::Background => {
                for y in top..(top + fh).min(height) {
                    canvas[(y * width + left.min(width)) * 4..(y * width + (left + fw).min(width)) * 4].fill(0);
                }
            }
            gif::DisposalMethod::Previous => {
                if let Some(before) = before {
                    canvas = before;
                }
            }
            _ => {}
        }
    }
    if delays.is_empty() {
        return Err("that GIF has no pictures in it".into());
    }
    let movie = Movie {
        name: name.to_string(),
        width: small_w,
        height: small_h,
        delays,
        rgb,
    };
    movie
        .sound()
        .then_some(movie)
        .ok_or_else(|| "that GIF could not be made into a moving picture".to_string())
}

/// What the UI is told about a kept movie.
fn describe(id: &str, movie: &Movie) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": movie.name,
        "width": movie.width,
        "height": movie.height,
        "frames": movie.frames(),
        "seconds": movie.seconds(),
    })
}

impl Shared {
    /// The folder moving pictures are kept in, beside the songs. `None`
    /// when nothing is being kept (a show opened without a file): movies
    /// then last as long as the program runs.
    pub fn movies_dir(&self) -> Option<PathBuf> {
        if let Some(library) = &self.library {
            return Some(library.songs_dir().with_file_name("movies"));
        }
        let beside = self.save_path.lock().unwrap().as_ref()?.parent()?.to_path_buf();
        Some(beside.join(format!("{}-movies", crate::app::APP_NAME)))
    }

    /// Reads the movies kept on this computer, so effects can play them.
    /// Files that are not movies are left alone.
    pub fn load_movies(&self) {
        let Some(dir) = self.movies_dir() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some(EXT) {
                continue;
            }
            let too_big = entry.metadata().map(|m| m.len() as usize > MAX_UPLOAD_BYTES).unwrap_or(true);
            match (!too_big).then(|| std::fs::read(&path).ok()).flatten().and_then(|b| Movie::read(&b)) {
                Some(movie) => {
                    movie::put(movie);
                }
                None => log::warn!("{} is not a moving picture; left as it is", path.display()),
            }
        }
    }

    /// The movies there are to choose from.
    pub fn movies_status(&self) -> Vec<serde_json::Value> {
        movie::all().iter().map(|(id, movie)| describe(id, movie)).collect()
    }

    /// Keeps a moving picture the user chose: a GIF as it is, or frames
    /// the page has already made from a video. The same one sent twice
    /// is kept once. Returns what the UI needs to choose it.
    pub fn store_movie(&self, name: &str, kind: &str, bytes: &[u8]) -> Result<serde_json::Value, String> {
        let name = commands::name(name, "The name")?;
        if bytes.is_empty() {
            return Err("that file is empty".into());
        }
        let movie = match kind {
            "gif" => from_gif(&name, bytes)?,
            "frames" => {
                let mut movie = Movie::read(bytes).ok_or("those frames could not be read")?;
                movie.name = name;
                movie
            }
            _ => return Err("only GIFs and frames made from a video can be kept".into()),
        };
        let id = movie.id();
        if let Some(dir) = self.movies_dir() {
            std::fs::create_dir_all(&dir).map_err(|e| format!("could not make a folder for moving pictures: {e}"))?;
            let path = dir.join(format!("{id}.{EXT}"));
            if !path.is_file() {
                std::fs::write(&path, movie.write()).map_err(|e| format!("could not keep the moving picture: {e}"))?;
            }
        }
        let described = describe(&id, &movie);
        movie::put(movie);
        self.library_rev.send_modify(|r| *r += 1);
        Ok(described)
    }

    /// Forgets a kept movie. Effects that played it show the stand-in.
    pub fn remove_movie(&self, id: &str) -> Result<Option<String>, String> {
        if !movie::id_ok(id) || !movie::remove(id) {
            return Err("that moving picture is no longer there".into());
        }
        if let Some(dir) = self.movies_dir() {
            let _ = std::fs::remove_file(dir.join(format!("{id}.{EXT}")));
        }
        self.library_rev.send_modify(|r| *r += 1);
        Ok(None)
    }

    /// The frames of a kept movie, as they are sent and stored.
    pub fn movie_bytes(&self, id: &str) -> Option<Arc<Movie>> {
        movie::id_ok(id).then(|| movie::get(id)).flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkrgb_core::Show;

    /// A GIF of two frames, four dots by two: all red, then with its
    /// right half turned blue by a frame that covers only that half.
    fn two_frame_gif() -> Vec<u8> {
        let mut out = Vec::new();
        {
            let palette = [255, 0, 0, 0, 0, 255];
            let mut encoder = gif::Encoder::new(&mut out, 4, 2, &palette).unwrap();
            let mut first = gif::Frame::from_indexed_pixels(4, 2, vec![0u8; 8], None);
            first.delay = 5;
            encoder.write_frame(&first).unwrap();
            let mut second = gif::Frame::from_indexed_pixels(2, 2, vec![1u8; 4], None);
            (second.left, second.delay) = (2, 20);
            encoder.write_frame(&second).unwrap();
        }
        out
    }

    #[test]
    fn a_gif_becomes_frames_as_they_would_be_seen() {
        let movie = from_gif("Flag", &two_frame_gif()).unwrap();
        assert_eq!((movie.width, movie.height, movie.frames()), (4, 2, 2));
        assert_eq!(movie.delays, [50, 200]);
        assert_eq!(movie.dot(0, 0, 0), [255, 0, 0]);
        assert_eq!(movie.dot(0, 3, 1), [255, 0, 0]);
        // The second frame drew over the right half only.
        assert_eq!(movie.dot(1, 0, 0), [255, 0, 0]);
        assert_eq!(movie.dot(1, 3, 1), [0, 0, 255]);
        // Things that are not GIFs say so and do nothing else.
        assert!(from_gif("x", b"not a gif at all").is_err());
        assert!(from_gif("x", &two_frame_gif()[..20]).is_err());
    }

    #[test]
    fn a_big_gif_is_shrunk_to_fit() {
        let mut out = Vec::new();
        {
            let palette = [0, 255, 0, 0, 0, 0];
            let mut encoder = gif::Encoder::new(&mut out, 200, 100, &palette).unwrap();
            let frame = gif::Frame::from_indexed_pixels(200, 100, vec![0u8; 20_000], None);
            encoder.write_frame(&frame).unwrap();
        }
        let movie = from_gif("Big", &out).unwrap();
        assert_eq!((movie.width, movie.height), (64, 32));
        assert_eq!(movie.dot(0, 63, 31), [0, 255, 0]);
        assert_eq!(movie.delays, [DEFAULT_DELAY_MS], "no time given means a tenth of a second");
    }

    #[test]
    fn movies_are_kept_listed_found_again_after_a_restart_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("darkrgb-movies-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let shared = Shared::new(Show::default(), Some(dir.join("show.json")), None);
        let kept = shared.store_movie("flag.gif", "gif", &two_frame_gif()).unwrap();
        let id = kept["id"].as_str().unwrap().to_string();
        assert_eq!((kept["name"].as_str(), kept["frames"].as_u64()), (Some("flag.gif"), Some(2)));
        assert!(shared.movies_status().iter().any(|m| m["id"] == id.as_str()));
        let file = shared.movies_dir().unwrap().join(format!("{id}.{EXT}"));
        assert!(file.is_file());
        // The same picture again, as frames this time, is the same movie.
        let frames = movie::get(&id).unwrap().write();
        let again = shared.store_movie("renamed", "frames", &frames).unwrap();
        assert_eq!(again["id"], id.as_str());
        assert_eq!(std::fs::read_dir(shared.movies_dir().unwrap()).unwrap().count(), 1);
        // Frames that are not frames, and kinds that are not kinds.
        assert!(shared.store_movie("x", "frames", b"nonsense").is_err());
        assert!(shared.store_movie("x", "mp4", b"nonsense").is_err());
        assert!(shared.store_movie("x", "gif", b"").is_err());

        // After a restart it is read back from its file.
        assert!(movie::remove(&id));
        assert!(movie::get(&id).is_none());
        shared.load_movies();
        assert_eq!(movie::get(&id).unwrap().frames(), 2);
        // Forgotten: gone from the list and from the folder.
        shared.remove_movie(&id).unwrap();
        assert!(movie::get(&id).is_none() && !file.is_file());
        assert!(shared.remove_movie(&id).is_err());
        assert!(shared.remove_movie("../../etc/passwd").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
