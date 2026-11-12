//! A show laid out in time, usually against a song.
//!
//! A timeline is built from what a show already has. A **scene clip**
//! says "from here, this scene". A **pad clip** says "from here to there,
//! this pad is held": a flash, a strobe, a colour, a blackout, an effect
//! switched on. So anything that can be played live can be written down,
//! and a performance can be recorded by simply playing it.
//!
//! Nothing here remembers where it is. [`Timeline::at`] answers "what
//! should be on at this second?" from the clips alone, which is what lets
//! playback start anywhere, jump about and loop without special cases.

use serde::{Deserialize, Serialize};

/// The longest a timeline can be: six hours.
pub const MAX_LENGTH: f32 = 6.0 * 3600.0;
/// The most clips a timeline holds.
pub const MAX_CLIPS: usize = 5000;
/// The shortest a pad clip can be, in seconds.
pub const MIN_CLIP: f32 = 0.02;

/// The song a timeline plays against. The file is kept by the engine; the
/// show only names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Song {
    /// The file's name in the engine's songs folder.
    pub file: String,
    /// What the user's own file was called, to show.
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClipKind {
    /// Switch to this scene. It stays until the next scene clip.
    Scene { id: u32 },
    /// Hold this pad (a binding's id) for the length of the clip.
    Pad { id: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: u32,
    /// Seconds from the start of the timeline.
    pub start: f32,
    /// Seconds it lasts. A scene clip has no length of its own.
    #[serde(default)]
    pub length: f32,
    pub what: ClipKind,
}

impl Clip {
    pub fn end(&self) -> f32 {
        self.start + self.length
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    #[serde(default)]
    pub song: Option<Song>,
    /// Seconds from start to end.
    #[serde(default = "default_length")]
    pub length: f32,
    /// The tempo of the grid clips snap to. 0 for no grid.
    #[serde(default)]
    pub bpm: f32,
    /// Where the first beat of the grid falls, in seconds.
    #[serde(default)]
    pub offset: f32,
    /// Start again from the beginning on reaching the end.
    #[serde(default)]
    pub repeat: bool,
    #[serde(default)]
    pub clips: Vec<Clip>,
}

fn default_length() -> f32 {
    180.0
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            song: None,
            length: default_length(),
            bpm: 0.0,
            offset: 0.0,
            repeat: false,
            clips: Vec::new(),
        }
    }
}

/// What a timeline calls for at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moment {
    /// The scene that should be showing, if the timeline has reached one.
    pub scene: Option<u32>,
    /// The pads held, in order of id, each once.
    pub pads: Vec<u32>,
}

impl Timeline {
    /// True for a timeline nobody has touched, which is left out of files.
    pub fn is_untouched(&self) -> bool {
        *self == Self::default()
    }

    /// What should be on at `t` seconds.
    pub fn at(&self, t: f32) -> Moment {
        let mut scene: Option<(f32, u32)> = None;
        let mut pads = Vec::new();
        for clip in &self.clips {
            match clip.what {
                // The latest scene clip already begun. Of two at the same
                // instant, the later in the list.
                ClipKind::Scene { id } if clip.start <= t && scene.is_none_or(|(at, _)| clip.start >= at) => {
                    scene = Some((clip.start, id));
                }
                ClipKind::Pad { id } if clip.start <= t && t < clip.end() && !pads.contains(&id) => pads.push(id),
                _ => {}
            }
        }
        pads.sort_unstable();
        Moment {
            scene: scene.map(|(_, id)| id),
            pads,
        }
    }

    /// The beat of the grid nearest to `t`, or `t` itself with no grid.
    /// `parts` cuts each beat finer: 2 for half beats, 4 for quarters.
    pub fn snap(&self, t: f32, parts: u32) -> f32 {
        if self.bpm <= 0.0 {
            return t;
        }
        let step = 60.0 / self.bpm / parts.max(1) as f32;
        (((t - self.offset) / step).round() * step + self.offset).max(0.0)
    }

    /// Drops clips whose scene or pad is gone. `scene` and `pad` say
    /// whether an id still exists.
    pub fn prune(&mut self, scene: impl Fn(u32) -> bool, pad: impl Fn(u32) -> bool) {
        self.clips.retain(|clip| match clip.what {
            ClipKind::Scene { id } => scene(id),
            ClipKind::Pad { id } => pad(id),
        });
    }

    /// Puts right what a file edited by hand may have wrong: lengths and
    /// times in range, the grid sane, clips inside the timeline and no
    /// more of them than there may be.
    pub fn repair(&mut self) {
        let finite = |v: f32, or: f32| if v.is_finite() { v } else { or };
        self.length = finite(self.length, default_length()).clamp(1.0, MAX_LENGTH);
        self.bpm = finite(self.bpm, 0.0);
        if !(20.0..=400.0).contains(&self.bpm) {
            self.bpm = 0.0;
        }
        self.offset = finite(self.offset, 0.0).clamp(0.0, self.length);
        self.clips.truncate(MAX_CLIPS);
        let length = self.length;
        for clip in &mut self.clips {
            clip.start = finite(clip.start, 0.0).clamp(0.0, length);
            clip.length = match clip.what {
                ClipKind::Scene { .. } => 0.0,
                ClipKind::Pad { .. } => finite(clip.length, MIN_CLIP).clamp(MIN_CLIP, MAX_LENGTH),
            };
        }
        if let Some(song) = &mut self.song
            && !song_file_ok(&song.file)
        {
            self.song = None;
        }
    }
}

/// The kinds of sound file a song may be, by the end of its name.
pub const SONG_TYPES: [&str; 9] = ["mp3", "wav", "ogg", "oga", "flac", "m4a", "aac", "opus", "webm"];

/// Whether `file` is a name the engine itself would have given a song: a
/// plain name with one of the endings above, and nothing that could lead
/// out of the songs folder.
pub fn song_file_ok(file: &str) -> bool {
    let plain = !file.is_empty()
        && file.len() <= 80
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        && !file.starts_with('.');
    plain
        && file
            .rsplit_once('.')
            .is_some_and(|(stem, ext)| !stem.is_empty() && SONG_TYPES.contains(&ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(id: u32, clip: u32, start: f32) -> Clip {
        Clip {
            id: clip,
            start,
            length: 0.0,
            what: ClipKind::Scene { id },
        }
    }

    fn pad(id: u32, clip: u32, start: f32, length: f32) -> Clip {
        Clip {
            id: clip,
            start,
            length,
            what: ClipKind::Pad { id },
        }
    }

    #[test]
    fn a_scene_shows_from_its_clip_until_the_next() {
        let timeline = Timeline {
            clips: vec![scene(7, 1, 10.0), scene(8, 2, 4.0), scene(9, 3, 20.0)],
            ..Timeline::default()
        };
        assert_eq!(timeline.at(0.0).scene, None, "nothing before the first");
        assert_eq!(timeline.at(4.0).scene, Some(8), "from the very instant it starts");
        assert_eq!(timeline.at(9.99).scene, Some(8));
        assert_eq!(timeline.at(10.0).scene, Some(7));
        assert_eq!(timeline.at(500.0).scene, Some(9), "the last one stays");
        // Jumping back gives what was showing then: nothing is remembered.
        assert_eq!(timeline.at(5.0).scene, Some(8));
    }

    #[test]
    fn a_pad_is_held_for_the_length_of_its_clip() {
        let timeline = Timeline {
            clips: vec![pad(3, 1, 1.0, 0.5), pad(4, 2, 1.2, 2.0), pad(3, 3, 1.4, 1.0)],
            ..Timeline::default()
        };
        assert_eq!(timeline.at(0.9).pads, Vec::<u32>::new());
        assert_eq!(timeline.at(1.0).pads, [3]);
        assert_eq!(timeline.at(1.3).pads, [3, 4]);
        // Two clips of one pad overlapping hold it once, and it stays
        // held until the later one ends.
        assert_eq!(timeline.at(1.45).pads, [3, 4]);
        assert_eq!(timeline.at(2.39).pads, [3, 4]);
        assert_eq!(timeline.at(2.4).pads, [4], "a clip ends just before its end time");
        assert_eq!(timeline.at(3.2).pads, Vec::<u32>::new());
    }

    #[test]
    fn times_snap_to_the_grid_when_there_is_one() {
        let mut timeline = Timeline::default();
        assert_eq!(timeline.snap(1.234, 1), 1.234, "no grid, no snapping");
        timeline.bpm = 120.0;
        timeline.offset = 0.1;
        // Beats fall at 0.1, 0.6, 1.1, ...
        assert!((timeline.snap(0.7, 1) - 0.6).abs() < 1e-5);
        assert!((timeline.snap(0.9, 1) - 1.1).abs() < 1e-5);
        assert!((timeline.snap(0.9, 2) - 0.85).abs() < 1e-5, "half beats");
        assert_eq!(timeline.snap(-3.0, 1), 0.0, "never before the start");
    }

    #[test]
    fn clips_of_things_that_are_gone_are_dropped_and_bad_numbers_put_right() {
        let mut timeline = Timeline {
            clips: vec![
                scene(7, 1, 1.0),
                scene(8, 2, 2.0),
                pad(3, 3, 1.0, 1.0),
                pad(4, 4, 1.0, 1.0),
            ],
            ..Timeline::default()
        };
        timeline.prune(|id| id == 7, |id| id == 4);
        assert_eq!(timeline.clips.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 4]);

        let mut timeline = Timeline {
            length: f32::NAN,
            bpm: 9000.0,
            offset: -4.0,
            clips: vec![
                pad(1, 1, f32::INFINITY, -1.0),
                scene(2, 2, -5.0),
                pad(1, 3, 1.0e9, f32::NAN),
            ],
            song: Some(Song {
                file: "../../etc/passwd".into(),
                name: "x".into(),
            }),
            ..Timeline::default()
        };
        timeline.repair();
        assert_eq!((timeline.length, timeline.bpm, timeline.offset), (180.0, 0.0, 0.0));
        assert_eq!((timeline.clips[0].start, timeline.clips[0].length), (0.0, MIN_CLIP));
        assert_eq!(timeline.clips[1].start, 0.0);
        assert_eq!((timeline.clips[2].start, timeline.clips[2].length), (180.0, MIN_CLIP));
        assert_eq!(timeline.song, None);
        let again = timeline.clone();
        timeline.repair();
        assert_eq!(timeline, again, "repairing twice changes nothing more");
    }

    #[test]
    fn only_plain_sound_file_names_are_songs() {
        for good in ["a1b2c3.mp3", "9f-x_2.flac", "song.webm"] {
            assert!(song_file_ok(good), "{good}");
        }
        for bad in [
            "",
            "song",
            ".mp3",
            "song.exe",
            "../song.mp3",
            "a/b.mp3",
            "a b.mp3",
            "song.mp3.txt",
            "s\\x.wav",
        ] {
            assert!(!song_file_ok(bad), "{bad}");
        }
        assert!(!song_file_ok(&format!("{}.mp3", "a".repeat(90))));
    }

    #[test]
    fn an_untouched_timeline_is_left_out_of_a_file() {
        assert!(Timeline::default().is_untouched());
        let touched = Timeline {
            bpm: 120.0,
            ..Timeline::default()
        };
        assert!(!touched.is_untouched());
        // And a file with only some of it still opens.
        let partial: Timeline =
            serde_json::from_str(r#"{"clips":[{"id":1,"start":2.5,"what":{"type":"scene","id":4}}]}"#).unwrap();
        assert_eq!((partial.length, partial.clips[0].length), (180.0, 0.0));
    }
}
