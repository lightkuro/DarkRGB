//! The folder of shows on this computer.
//!
//! Each show is one file in `shows/`. A small text file remembers which
//! one was open. Deleting a show moves it to `trash/`; nothing here ever
//! erases a show.
//!
//! Beside the shows is `patterns.json`: effects with their settings,
//! whole looks and palettes that the user has kept to use in any show.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use darkrgb_core::{Layer, Show};
use serde::{Deserialize, Serialize};

/// Most shows listed. Far more than anyone keeps; a bound on the work.
const MAX_LISTED: usize = 500;

#[derive(Clone, Debug)]
pub struct Library {
    dir: PathBuf,
}

/// Most patterns and most palettes kept.
pub const MAX_PATTERNS: usize = 200;
pub const MAX_PALETTES: usize = 100;

/// What the user has kept to use in any show.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// Next id to hand out, for patterns and palettes alike.
    #[serde(default)]
    pub next_id: u32,
    #[serde(default)]
    pub patterns: Vec<Pattern>,
    #[serde(default)]
    pub palettes: Vec<SavedPalette>,
}

/// One effect with its settings, or a whole look of several.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub id: u32,
    pub name: String,
    /// True for a look: used, it becomes a scene. False for effects that
    /// are added to whatever is playing.
    #[serde(default)]
    pub look: bool,
    /// Bottom first. They name no lights, only kinds of light.
    pub layers: Vec<Layer>,
}

/// A palette of the user's own, by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPalette {
    pub id: u32,
    pub name: String,
    /// The colours, written as a palette setting holds them.
    pub value: String,
}

impl Saved {
    pub fn alloc_id(&mut self) -> u32 {
        let used = self
            .patterns
            .iter()
            .map(|p| p.id)
            .chain(self.palettes.iter().map(|p| p.id))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(used) + 1;
        self.next_id
    }
}

/// What the list of shows says about one of them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShowEntry {
    /// The file's name inside the shows folder; how the show is asked for.
    pub file: String,
    pub name: String,
    /// When it was last saved, in seconds since 1970.
    pub modified: u64,
    pub lights: usize,
    pub effects: usize,
    pub scenes: usize,
    /// False for a file that could not be understood.
    pub readable: bool,
}

/// A file name for a show that is safe on any system and not yet taken:
/// `Name.json`, then `Name (2).json`, and so on.
pub fn free_path(folder: &Path, show_name: &str) -> PathBuf {
    let mut stem: String = show_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    stem = stem.trim().chars().take(60).collect();
    if stem.is_empty() {
        stem = "show".into();
    }
    let mut path = folder.join(format!("{stem}.json"));
    let mut n = 2;
    while path.exists() {
        path = folder.join(format!("{stem} ({n}).json"));
        n += 1;
    }
    path
}

impl Library {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn shows_dir(&self) -> PathBuf {
        self.dir.join("shows")
    }

    fn saved_path(&self) -> PathBuf {
        self.dir.join("patterns.json")
    }

    /// Reads what the user has kept. A file that cannot be understood is
    /// set aside, not overwritten, and the list starts empty.
    pub fn load_saved(&self) -> Saved {
        let path = self.saved_path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Saved::default();
        };
        match serde_json::from_str::<Saved>(&text) {
            Ok(mut saved) => {
                saved.patterns.truncate(MAX_PATTERNS);
                saved.palettes.truncate(MAX_PALETTES);
                saved
            }
            Err(e) => {
                log::warn!("could not read {}: {e}", path.display());
                let _ = std::fs::rename(&path, path.with_extension("json.bak"));
                Saved::default()
            }
        }
    }

    /// Writes what the user has kept, whole or not at all.
    pub fn store_saved(&self, saved: &Saved) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.saved_path();
        let json = serde_json::to_string_pretty(saved).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }

    fn trash_dir(&self) -> PathBuf {
        self.dir.join("trash")
    }

    fn pointer(&self) -> PathBuf {
        self.dir.join("current.txt")
    }

    /// The show to load when the program starts: the one that was open
    /// last, else the one saved most recently, else a new one. A show from
    /// before there was a library (a single `show.json`) is moved in first.
    pub fn start(&self) -> PathBuf {
        let shows = self.shows_dir();
        if let Err(e) = std::fs::create_dir_all(&shows) {
            log::warn!("could not create {}: {e}", shows.display());
        }
        let single = self.dir.join("show.json");
        if single.exists() {
            let name = std::fs::read_to_string(&single)
                .ok()
                .and_then(|text| serde_json::from_str::<Show>(&text).ok())
                .map_or_else(|| "My show".to_string(), |show| show.name);
            let moved = free_path(&shows, &name);
            match std::fs::rename(&single, &moved) {
                Ok(()) => {
                    log::info!("moved {} into the library as {}", single.display(), moved.display());
                    self.set_current(&moved);
                }
                Err(e) => log::warn!("could not move {} into the library: {e}", single.display()),
            }
        }
        let remembered = std::fs::read_to_string(self.pointer())
            .ok()
            .and_then(|file| self.path_of(file.trim()).ok());
        if let Some(path) = remembered {
            return path;
        }
        let newest = self.list().into_iter().max_by_key(|entry| entry.modified);
        match newest {
            Some(entry) => shows.join(entry.file),
            None => shows.join("Untitled show.json"),
        }
    }

    /// Remembers which show is open, for the next start.
    pub fn set_current(&self, path: &Path) {
        let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
            return;
        };
        if let Err(e) = std::fs::write(self.pointer(), file) {
            log::warn!("could not remember the open show: {e}");
        }
    }

    /// The path of a show named by its file name. Only plain file names
    /// inside the shows folder are accepted, so a request cannot reach
    /// any other file.
    pub fn path_of(&self, file: &str) -> Result<PathBuf, String> {
        let plain = !file.is_empty()
            && file.ends_with(".json")
            && !file.starts_with('.')
            && !file.chars().any(|c| matches!(c, '/' | '\\' | ':') || c.is_control());
        if !plain {
            return Err("that is not the name of a show".into());
        }
        let path = self.shows_dir().join(file);
        if path.is_file() {
            Ok(path)
        } else {
            Err("that show is no longer there".into())
        }
    }

    /// Every show in the folder, most recently saved first.
    pub fn list(&self) -> Vec<ShowEntry> {
        let Ok(entries) = std::fs::read_dir(self.shows_dir()) else {
            return Vec::new();
        };
        let mut list: Vec<ShowEntry> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let file = path.file_name()?.to_str()?.to_string();
                if !file.ends_with(".json") || file.starts_with('.') || !path.is_file() {
                    return None;
                }
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                let show = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| serde_json::from_str::<Show>(&text).ok());
                Some(match show {
                    Some(show) => ShowEntry {
                        file,
                        name: show.name,
                        modified,
                        lights: show.devices.len(),
                        effects: show.layers.len(),
                        scenes: show.scenes.len(),
                        readable: true,
                    },
                    None => ShowEntry {
                        name: file.trim_end_matches(".json").to_string(),
                        file,
                        modified,
                        lights: 0,
                        effects: 0,
                        scenes: 0,
                        readable: false,
                    },
                })
            })
            .take(MAX_LISTED)
            .collect();
        list.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
        list
    }

    /// A path for a new show of this name that no show has yet.
    pub fn new_path(&self, name: &str) -> PathBuf {
        free_path(&self.shows_dir(), name)
    }

    /// Moves a show's file to the trash folder and says where it went.
    pub fn to_trash(&self, path: &Path) -> std::io::Result<PathBuf> {
        let trash = self.trash_dir();
        std::fs::create_dir_all(&trash)?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("show");
        let to = free_path(&trash, stem);
        std::fs::rename(path, &to)?;
        Ok(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_library(name: &str) -> Library {
        let dir = std::env::temp_dir().join(format!("darkrgb-library-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Library::new(dir)
    }

    fn write(library: &Library, file: &str, name: &str) -> PathBuf {
        std::fs::create_dir_all(library.shows_dir()).unwrap();
        let path = library.shows_dir().join(file);
        let show = Show {
            name: name.into(),
            ..Show::default()
        };
        std::fs::write(&path, serde_json::to_string(&show).unwrap()).unwrap();
        path
    }

    #[test]
    fn a_fresh_library_starts_a_new_show() {
        let library = temp_library("fresh");
        let path = library.start();
        assert_eq!(path, library.shows_dir().join("Untitled show.json"));
        assert!(library.shows_dir().is_dir());
        assert!(library.list().is_empty());
    }

    #[test]
    fn a_show_from_before_the_library_is_moved_in_and_opened() {
        let library = temp_library("migrate");
        let old = library.dir.join("show.json");
        let show = Show {
            name: "Friday: set / 1".into(),
            ..Show::default()
        };
        std::fs::write(&old, serde_json::to_string(&show).unwrap()).unwrap();
        let path = library.start();
        assert!(!old.exists(), "moved, not copied");
        assert_eq!(path.file_name().unwrap(), "Friday_ set _ 1.json");
        assert_eq!(library.list()[0].name, "Friday: set / 1");
        // The next start opens the same show.
        assert_eq!(library.start(), path);
    }

    #[test]
    fn the_last_open_show_is_remembered_and_a_missing_one_falls_back() {
        let library = temp_library("pointer");
        let a = write(&library, "A.json", "A");
        let b = write(&library, "B.json", "B");
        library.set_current(&a);
        assert_eq!(library.start(), a);
        library.set_current(&b);
        assert_eq!(library.start(), b);
        std::fs::remove_file(&b).unwrap();
        assert_eq!(library.start(), a, "the remembered show is gone; the other is opened");
    }

    #[test]
    fn only_plain_names_inside_the_folder_are_accepted() {
        let library = temp_library("names");
        write(&library, "Mine.json", "Mine");
        assert!(library.path_of("Mine.json").is_ok());
        for bad in [
            "",
            "Mine",
            "../Mine.json",
            "..\\Mine.json",
            "/etc/passwd.json",
            "sub/Mine.json",
            "C:Mine.json",
            ".hidden.json",
            "Mine.json\n",
        ] {
            assert!(library.path_of(bad).is_err(), "{bad:?} was accepted");
        }
        assert!(library.path_of("Gone.json").unwrap_err().contains("no longer"));
    }

    #[test]
    fn the_list_counts_what_is_in_each_show_and_survives_a_broken_file() {
        let library = temp_library("list");
        write(&library, "Good.json", "Good one");
        std::fs::write(library.shows_dir().join("Broken.json"), "{ nope").unwrap();
        std::fs::write(library.shows_dir().join("notes.txt"), "not a show").unwrap();
        let list = library.list();
        assert_eq!(list.len(), 2, "{list:?}");
        let good = list.iter().find(|e| e.file == "Good.json").unwrap();
        assert_eq!((good.name.as_str(), good.effects, good.readable), ("Good one", 1, true));
        let broken = list.iter().find(|e| e.file == "Broken.json").unwrap();
        assert_eq!((broken.name.as_str(), broken.readable), ("Broken", false));
    }

    #[test]
    fn a_deleted_show_goes_to_the_trash_and_names_never_collide() {
        let library = temp_library("trash");
        let first = write(&library, "Same.json", "Same");
        let trashed = library.to_trash(&first).unwrap();
        assert!(!first.exists() && trashed.is_file());
        let second = write(&library, "Same.json", "Same");
        let again = library.to_trash(&second).unwrap();
        assert_ne!(trashed, again);
        assert!(trashed.is_file() && again.is_file(), "both are kept");
        // A new show of a name in use gets a name of its own.
        write(&library, "Same.json", "Same");
        assert_eq!(library.new_path("Same").file_name().unwrap(), "Same (2).json");
        assert_eq!(
            free_path(&library.shows_dir(), "../../etc").file_name().unwrap(),
            "______etc.json"
        );
    }
}
