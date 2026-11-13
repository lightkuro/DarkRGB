//! Colours and palettes.
//!
//! Effects work with display-referred values in 0..1 (what you would type
//! into a colour picker). Gamma for the LEDs is applied once, at output.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0.0, 0.0, 0.0);
    pub const WHITE: Rgb = Rgb::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Hue, saturation and value, each 0..1. Hue wraps.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(1.0) * 6.0;
        let i = h.floor();
        let f = h - i;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        match i as u32 % 6 {
            0 => Rgb::new(v, t, p),
            1 => Rgb::new(q, v, p),
            2 => Rgb::new(p, v, t),
            3 => Rgb::new(p, q, v),
            4 => Rgb::new(t, p, v),
            _ => Rgb::new(v, p, q),
        }
    }

    /// Parses `#rrggbb` or `rrggbb`.
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 || !s.is_ascii() {
            return None;
        }
        let n = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb::new(
            ((n >> 16) & 0xff) as f32 / 255.0,
            ((n >> 8) & 0xff) as f32 / 255.0,
            (n & 0xff) as f32 / 255.0,
        ))
    }

    pub fn to_hex(self) -> String {
        let [r, g, b] = self.to_bytes();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    pub fn scale(self, k: f32) -> Self {
        Rgb::new(self.r * k, self.g * k, self.b * k)
    }

    // `add` and `mul` are plain methods on purpose: operators would hide
    // that these are per-channel and unclamped.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, o: Rgb) -> Self {
        Rgb::new(self.r + o.r, self.g + o.g, self.b + o.b)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, o: Rgb) -> Self {
        Rgb::new(self.r * o.r, self.g * o.g, self.b * o.b)
    }

    pub fn max(self, o: Rgb) -> Self {
        Rgb::new(self.r.max(o.r), self.g.max(o.g), self.b.max(o.b))
    }

    pub fn lerp(self, o: Rgb, t: f32) -> Self {
        Rgb::new(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
        )
    }

    pub fn clamp(self) -> Self {
        Rgb::new(self.r.clamp(0.0, 1.0), self.g.clamp(0.0, 1.0), self.b.clamp(0.0, 1.0))
    }

    /// 8-bit values with no gamma, for previews and hex strings.
    pub fn to_bytes(self) -> [u8; 3] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        [q(self.r), q(self.g), q(self.b)]
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::from_hex(&s).ok_or_else(|| serde::de::Error::custom(format!("bad colour {s:?}")))
    }
}

/// A lookup table that turns a 0..1 value into an 8-bit LED level, with
/// gamma and a brightness limit baked in.
pub struct OutputCurve {
    table: [u8; 1024],
    /// The same levels before rounding, in 256ths of a step.
    fine: [u16; 1024],
}

impl OutputCurve {
    pub fn new(gamma: f32, brightness: f32) -> Self {
        let gamma = gamma.clamp(1.0, 4.0);
        let brightness = brightness.clamp(0.0, 1.0);
        let mut table = [0u8; 1024];
        let mut fine = [0u16; 1024];
        for (i, slot) in table.iter_mut().enumerate() {
            let v = i as f32 / 1023.0;
            let level = v.powf(gamma) * brightness * 255.0;
            *slot = (level + 0.5) as u8;
            fine[i] = (level * 256.0 + 0.5) as u16;
        }
        Self { table, fine }
    }

    /// The level for `v` when the step it falls short of is made up over
    /// time: `turn` (0..=255) is where this pixel is in its round, and
    /// the level is the upper of the two steps for as much of the round
    /// as `v` lies towards it. Over a whole round the average is exact.
    #[inline]
    pub fn apply_dithered(&self, v: f32, turn: u8) -> u8 {
        let fine = self.fine[(v.clamp(0.0, 1.0) * 1023.0 + 0.5) as usize] as u32;
        ((fine + turn as u32) >> 8).min(255) as u8
    }

    #[inline]
    pub fn apply(&self, v: f32) -> u8 {
        self.table[(v.clamp(0.0, 1.0) * 1023.0 + 0.5) as usize]
    }
}

/// A named colour gradient.
#[derive(Clone, Debug, Serialize)]
pub struct Palette {
    pub id: &'static str,
    pub name: &'static str,
    /// Evenly spaced colour stops.
    pub stops: Vec<Rgb>,
    /// True when the last stop should blend back into the first.
    pub cyclic: bool,
}

impl Palette {
    /// Colour at `t`. Cyclic palettes wrap; others clamp to 0..1.
    pub fn sample(&self, t: f32) -> Rgb {
        let n = self.stops.len();
        if n == 0 {
            return Rgb::BLACK;
        }
        if n == 1 {
            return self.stops[0];
        }
        if self.cyclic {
            let f = t.rem_euclid(1.0) * n as f32;
            let i = (f.floor() as usize) % n;
            self.stops[i].lerp(self.stops[(i + 1) % n], f - f.floor())
        } else {
            let f = t.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (f.floor() as usize).min(n - 2);
            self.stops[i].lerp(self.stops[i + 1], f - i as f32)
        }
    }
}

fn hex(s: &str) -> Rgb {
    Rgb::from_hex(s).expect("built-in palette colour")
}

fn make(id: &'static str, name: &'static str, cyclic: bool, stops: &[&str]) -> Palette {
    Palette {
        id,
        name,
        stops: stops.iter().map(|s| hex(s)).collect(),
        cyclic,
    }
}

/// Built-in palettes. The first one is the default.
pub fn palettes() -> &'static [Palette] {
    static PALETTES: std::sync::OnceLock<Vec<Palette>> = std::sync::OnceLock::new();
    PALETTES.get_or_init(|| {
        vec![
            make(
                "rainbow",
                "Rainbow",
                true,
                &[
                    "#ff0000", "#ffb000", "#b0ff00", "#00ff40", "#00ffe0", "#0060ff", "#7000ff", "#ff00b0",
                ],
            ),
            make(
                "sunset",
                "Sunset",
                false,
                &["#1b0a3c", "#7a1e6c", "#e0364f", "#ff8a3d", "#ffd86b"],
            ),
            make(
                "ocean",
                "Ocean",
                false,
                &["#020b2a", "#053a8c", "#0a84d8", "#2fd6d0", "#d6fff5"],
            ),
            make(
                "fire",
                "Fire",
                false,
                &["#140000", "#8a0a00", "#ff3a00", "#ffa200", "#fff3b0"],
            ),
            make(
                "forest",
                "Forest",
                false,
                &["#03140a", "#0b5a2a", "#2fae3f", "#b6e64a", "#f4ffb8"],
            ),
            make(
                "neon",
                "Neon",
                true,
                &["#ff2bd6", "#7a2bff", "#2b8cff", "#2bffd9", "#f5ff2b"],
            ),
            make(
                "ice",
                "Ice",
                false,
                &["#02060f", "#12356e", "#3f8fe0", "#a9e2ff", "#ffffff"],
            ),
            make(
                "candy",
                "Candy",
                true,
                &["#ff5fa2", "#ffc35f", "#5fffd4", "#5f9bff", "#c45fff"],
            ),
            make("mono", "White", false, &["#000000", "#ffffff"]),
            // For houses and gardens.
            make("candy_cane", "Candy cane", true, &["#e8112d", "#ffffff"]),
            make(
                "christmas",
                "Christmas",
                true,
                &["#e8112d", "#0f9d3a", "#ffd24a", "#ffffff"],
            ),
            make(
                "fairy",
                "Fairy lights",
                true,
                &["#ff3b30", "#ffb020", "#2fd15a", "#2f8bff", "#ff5fc8"],
            ),
            make(
                "warm",
                "Warm white",
                false,
                &["#2a0e00", "#b8560e", "#ffb45a", "#ffe2b0"],
            ),
            make("aurora", "Aurora", true, &["#0b6b4f", "#22e39a", "#27b6e6", "#7a4dff"]),
            // For dance floors.
            make(
                "blacklight",
                "Blacklight",
                true,
                &["#3a0ca3", "#7209b7", "#f72585", "#4361ee"],
            ),
        ]
    })
}

/// Finds a palette by id, falling back to the default.
pub fn palette(id: &str) -> &'static Palette {
    let all = palettes();
    all.iter().find(|p| p.id == id).unwrap_or(&all[0])
}

/// A palette as an effect holds it: a built-in one, or the user's own.
pub type Pal = std::borrow::Cow<'static, Palette>;

/// Most colours a palette the user makes can have.
pub const MAX_STOPS: usize = 8;

/// A palette of the user's own from its written form: two to eight
/// colours separated by commas, followed by `|loop` when the last colour
/// should blend back into the first. `#e8112d,#ffffff|loop` is a candy
/// cane. Written out like this, a palette travels inside the setting that
/// uses it: into scenes, copies and saved patterns, with nothing to look up.
pub fn parse_custom(text: &str) -> Option<Palette> {
    if !text.starts_with('#') {
        return None;
    }
    let (colours, cyclic) = match text.strip_suffix("|loop") {
        Some(colours) => (colours, true),
        None => (text, false),
    };
    let stops: Vec<Rgb> = colours.split(',').map(Rgb::from_hex).collect::<Option<_>>()?;
    (2..=MAX_STOPS).contains(&stops.len()).then_some(Palette {
        id: "custom",
        name: "Your own",
        stops,
        cyclic,
    })
}

/// The written form of a palette of the user's own.
pub fn write_custom(palette: &Palette) -> String {
    let colours: Vec<String> = palette.stops.iter().map(|c| c.to_hex()).collect();
    format!("{}{}", colours.join(","), if palette.cyclic { "|loop" } else { "" })
}

/// Reads a palette setting: the id of a built-in palette, or the user's
/// own colours written out. Anything else gives the default palette.
pub fn resolve(text: &str) -> Pal {
    match parse_custom(text) {
        Some(own) => Pal::Owned(own),
        None => Pal::Borrowed(palette(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Rgb::from_hex("#12ab ef".replace(' ', "").as_str()).unwrap();
        assert_eq!(c.to_hex(), "#12abef");
        assert!(Rgb::from_hex("#12").is_none());
        assert!(Rgb::from_hex("zzzzzz").is_none());
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(Rgb::from_hsv(0.0, 1.0, 1.0).to_bytes(), [255, 0, 0]);
        assert_eq!(Rgb::from_hsv(1.0 / 3.0, 1.0, 1.0).to_bytes(), [0, 255, 0]);
        assert_eq!(Rgb::from_hsv(2.0 / 3.0, 1.0, 1.0).to_bytes(), [0, 0, 255]);
        // Hue wraps.
        assert_eq!(Rgb::from_hsv(1.0, 1.0, 1.0).to_bytes(), [255, 0, 0]);
    }

    #[test]
    fn output_curve_limits() {
        let c = OutputCurve::new(2.2, 0.5);
        assert_eq!(c.apply(0.0), 0);
        assert_eq!(c.apply(1.0), 128);
        assert_eq!(c.apply(7.0), 128);
        let linear = OutputCurve::new(1.0, 1.0);
        assert_eq!(linear.apply(0.5), 128);
    }

    #[test]
    fn palette_sampling() {
        let mono = palette("mono");
        assert_eq!(mono.sample(-1.0).to_bytes(), [0, 0, 0]);
        assert_eq!(mono.sample(2.0).to_bytes(), [255, 255, 255]);
        // A palette of the user's own, written out.
        let cane = parse_custom("#e8112d,#FFFFFF|loop").unwrap();
        assert_eq!((cane.stops.len(), cane.cyclic), (2, true));
        assert_eq!(write_custom(&cane), "#e8112d,#ffffff|loop");
        assert!(!parse_custom("#000000,#ffffff").unwrap().cyclic);
        for bad in [
            "",
            "rainbow",
            "#ffffff",
            "#ffffff,nope",
            "#fff,#000",
            "#ffffff,#000000|spin",
        ] {
            assert!(parse_custom(bad).is_none(), "{bad:?}");
        }
        let nine = ["#111111"; 9].join(",");
        assert!(parse_custom(&nine).is_none(), "at most eight colours");
        assert_eq!(resolve("#e8112d,#ffffff").stops.len(), 2);
        assert_eq!(resolve("ocean").id, "ocean");
        assert_eq!(resolve("#broken").id, "rainbow");
        let rainbow = palette("rainbow");
        assert_eq!(rainbow.sample(0.0), rainbow.sample(1.0));
        assert_eq!(palette("no-such-palette").id, "rainbow");
    }
}
