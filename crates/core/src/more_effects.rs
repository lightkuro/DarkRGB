//! Effects for shows that other people watch: stripes, snow and fairy
//! lights for a house or a garden, colour steps and lightning for a dance
//! floor. They follow the same rules as the ones in `effects.rs`.

use crate::color::{Pal, Palette, Rgb, palette};
use crate::effect::{
    AMBIENT, Effect, EffectDef, EffectInfo, FlashUse, FrameCtx, HITS_AND_BEATS, HitWatch, hash2, smoothstep,
};
use crate::font;
use crate::params::{ParamReader, ParamSpec};

/// The effects defined here, in the order the picker shows them within
/// their groups.
pub(crate) fn hits_and_beats() -> Vec<EffectDef> {
    vec![Steps::def(), Lightning::def()]
}

pub(crate) fn ambient() -> Vec<EffectDef> {
    vec![Stripes::def(), Twinkle::def(), Snow::def(), Words::def()]
}

/// One of a palette's own colours, by number, wrapping round. Stripes and
/// steps want the colours as they were picked, not blends between them.
fn stop(palette: &Palette, index: i64) -> Rgb {
    let n = palette.stops.len() as i64;
    if n == 0 {
        return Rgb::BLACK;
    }
    palette.stops[index.rem_euclid(n) as usize]
}

// --------------------------------------------------------------- Stripes

struct Stripes {
    palette: Pal,
    count: f32,
    speed: f32,
    soft: f32,
    angle: usize,
    travelled: f32,
}

impl Stripes {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "stripes",
                name: "Stripes",
                description: "Bands of colour that travel along the lights, like a candy cane.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "candy_cane"),
                    ParamSpec::float("count", "Stripes", 2.0, 24.0, 8.0),
                    ParamSpec::float("speed", "Speed", -3.0, 3.0, 0.5),
                    ParamSpec::float("soft", "Soft edges", 0.0, 1.0, 0.15).advanced(),
                    ParamSpec::choice("angle", "Run", &["Along", "Across", "Diagonal"]).advanced(),
                ],
            },
            make: || {
                Box::new(Stripes {
                    palette: Pal::Borrowed(palette("candy_cane")),
                    count: 8.0,
                    speed: 0.5,
                    soft: 0.15,
                    angle: 0,
                    travelled: 0.0,
                })
            },
        }
    }
}

impl Effect for Stripes {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.count = p.float("count").round();
        self.speed = p.float("speed");
        self.soft = p.float("soft");
        self.angle = p.choice("angle");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // In stripes a second. Kept small, so it stays exact for hours.
        self.travelled = (self.travelled + ctx.dt * self.speed).rem_euclid(4096.0);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        let along = match (ctx.flat, self.angle) {
            (true, _) | (false, 0) => x,
            (false, 1) => y,
            _ => (x + y) * 0.5,
        };
        let u = along * self.count - self.travelled;
        let index = u.floor() as i64;
        let within = u - u.floor();
        let colour = stop(&self.palette, index);
        // Near an edge, blend halfway into the neighbour, so the stripe
        // does not jump a whole pixel at a time as it travels.
        let half = self.soft * 0.5;
        if half <= 1e-4 {
            colour
        } else if within < half {
            stop(&self.palette, index - 1).lerp(colour, 0.5 + within / self.soft)
        } else if within > 1.0 - half {
            stop(&self.palette, index + 1).lerp(colour, 0.5 + (1.0 - within) / self.soft)
        } else {
            colour
        }
    }
}

// --------------------------------------------------------------- Twinkle

struct Twinkle {
    palette: Pal,
    amount: f32,
    speed: f32,
    glow: f32,
}

impl Twinkle {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "twinkle",
                name: "Twinkle",
                description: "Fairy lights: each one its own colour, slowly brightening and dimming.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "fairy"),
                    ParamSpec::float("amount", "How many are lit", 0.05, 1.0, 0.7),
                    ParamSpec::float("speed", "Speed", 0.1, 4.0, 0.8),
                    ParamSpec::float("glow", "Dimmest", 0.0, 1.0, 0.12).advanced(),
                ],
            },
            make: || {
                Box::new(Twinkle {
                    palette: Pal::Borrowed(palette("fairy")),
                    amount: 0.7,
                    speed: 0.8,
                    glow: 0.12,
                })
            },
        }
    }
}

impl Effect for Twinkle {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.amount = p.float("amount");
        self.speed = p.float("speed");
        self.glow = p.float("glow");
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // Every LED is its own bulb, with its own colour and its own time.
        let bulb = ((x * 2048.0) as u32).wrapping_mul(4099) ^ ((y * 512.0) as u32).wrapping_mul(131);
        if hash2(bulb, 3) > self.amount {
            return Rgb::BLACK;
        }
        let colour = stop(&self.palette, (hash2(bulb, 11) * 4096.0) as i64);
        let pace = 0.5 + hash2(bulb, 23);
        let wave = (ctx.time as f32 * self.speed * pace + hash2(bulb, 37) * std::f32::consts::TAU).sin();
        // Mostly bright with a slow dip, as a bulb on a string looks.
        let level = smoothstep(-1.0, 0.6, wave);
        colour.scale(self.glow + (1.0 - self.glow) * level)
    }
}

// ------------------------------------------------------------------ Snow

struct Snow {
    colour: Rgb,
    amount: f32,
    speed: f32,
    size: f32,
    wind: f32,
    fallen: f32,
}

impl Snow {
    /// Columns that flakes fall in, across the width of a matrix.
    const LANES: f32 = 20.0;

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "snow",
                name: "Snow",
                description: "Flakes drifting down a matrix, or along a strip.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ffffff"),
                    ParamSpec::float("amount", "Amount", 0.05, 1.0, 0.45),
                    ParamSpec::float("speed", "Speed", 0.02, 2.0, 0.3),
                    ParamSpec::float("size", "Flake size", 0.5, 3.0, 1.0).advanced(),
                    ParamSpec::float("wind", "Wind", -1.0, 1.0, 0.15).advanced(),
                ],
            },
            make: || {
                Box::new(Snow {
                    colour: Rgb::WHITE,
                    amount: 0.45,
                    speed: 0.3,
                    size: 1.0,
                    wind: 0.15,
                    fallen: 0.0,
                })
            },
        }
    }
}

impl Effect for Snow {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.amount = p.float("amount");
        self.speed = p.float("speed");
        self.size = p.float("size");
        self.wind = p.float("wind");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        self.fallen = (self.fallen + ctx.dt * self.speed).rem_euclid(4096.0);
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        // On a strip the flakes travel along it; on a matrix they fall in
        // lanes, leaning with the wind.
        let (lane, across, along, flakes, length) = if ctx.flat {
            (0u32, 0.0, x, 14u32, 0.02 * self.size)
        } else {
            let leaning = (x + self.wind * y * 0.3).rem_euclid(1.0) * Self::LANES;
            (leaning as u32, leaning - leaning.floor() - 0.5, y, 2, 0.06 * self.size)
        };
        let mut light = 0.0f32;
        for flake in 0..flakes {
            let id = lane.wrapping_mul(31).wrapping_add(flake);
            if hash2(id, 5) > self.amount {
                continue;
            }
            let pace = 0.6 + 0.8 * hash2(id, 17);
            let at = (self.fallen * pace + hash2(id, 29)).fract();
            // The shorter way round, so a flake leaving the bottom is
            // already arriving at the top.
            let away = (along - at).abs();
            let away = away.min(1.0 - away);
            let body = (1.0 - away / length).max(0.0);
            // Narrower than its lane, and off to one side of it.
            let aside = across - (hash2(id, 41) - 0.5) * 0.5;
            let wide = if ctx.flat {
                1.0
            } else {
                (1.0 - aside.abs() / (0.45 * self.size)).max(0.0)
            };
            let shimmer = 0.75 + 0.25 * (ctx.time as f32 * 3.0 + hash2(id, 53) * 6.3).sin();
            light = light.max(body * wide * shimmer);
        }
        self.colour.scale(light.min(1.0))
    }
}

// ----------------------------------------------------------------- Steps

struct Steps {
    palette: Pal,
    blocks: f32,
    every: usize,
    fade: f32,
    speed: f32,
    /// How many changes have happened, and how far into the current one.
    steps: f64,
}

impl Steps {
    /// Beats each colour stays for, for each "Change every" choice.
    const BEATS: [f64; 3] = [1.0, 2.0, 4.0];

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "steps",
                name: "Colour steps",
                description: "Blocks of colour that all change places on the beat.",
                category: HITS_AND_BEATS,
                audio: true,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::palette("palette", "Colours", "neon"),
                    ParamSpec::float("blocks", "Blocks", 1.0, 16.0, 4.0),
                    ParamSpec::choice("every", "Change every", &["Beat", "2 beats", "4 beats"]),
                    ParamSpec::float("fade", "Blend between colours", 0.0, 1.0, 0.15).advanced(),
                    ParamSpec::float("speed", "Changes a second with no beat", 0.2, 4.0, 1.0).advanced(),
                ],
            },
            make: || {
                Box::new(Steps {
                    palette: Pal::Borrowed(palette("neon")),
                    blocks: 4.0,
                    every: 0,
                    fade: 0.15,
                    speed: 1.0,
                    steps: 0.0,
                })
            },
        }
    }
}

impl Effect for Steps {
    fn configure(&mut self, p: &ParamReader) {
        self.palette = p.palette("palette");
        self.blocks = p.float("blocks").round();
        self.every = p.choice("every");
        self.fade = p.float("fade");
        self.speed = p.float("speed");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        if ctx.audio.bpm > 0.0 {
            self.steps = ctx.beats / Self::BEATS[self.every.min(2)];
        } else {
            // No tempo to follow: keep changing at a steady pace.
            self.steps = (self.steps + (ctx.dt * self.speed) as f64).rem_euclid(1_000_000.0);
        }
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let block = ((x * self.blocks) as i64).min(self.blocks as i64 - 1);
        let step = self.steps.floor() as i64;
        let within = (self.steps - self.steps.floor()) as f32;
        let now = stop(&self.palette, block + step);
        if self.fade <= 1e-4 || within >= self.fade {
            return now;
        }
        stop(&self.palette, block + step - 1).lerp(now, smoothstep(0.0, self.fade, within))
    }
}

// ------------------------------------------------------------- Lightning

struct Lightning {
    colour: Rgb,
    length: f32,
    rate: f32,
    forks: bool,
    any_hit: bool,
    watch: HitWatch,
    /// Seconds of the current burst still to come.
    left: f32,
    phase: f32,
    /// Changes with every burst, so no two look alike.
    burst: u32,
}

impl Lightning {
    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "lightning",
                name: "Lightning",
                description: "A burst of very fast flashes on every beat, or when fired from a pad.",
                category: HITS_AND_BEATS,
                audio: true,
                // A burst slowed to three flashes a second is not a burst.
                flash: FlashUse::Needed,
                params: vec![
                    ParamSpec::color("colour", "Colour", "#ffffff"),
                    ParamSpec::float("length", "Length of a burst (s)", 0.1, 1.5, 0.45),
                    ParamSpec::float("rate", "Flashes per second", 6.0, 25.0, 14.0),
                    ParamSpec::bool("forks", "Strike part of the lights", true).advanced(),
                    ParamSpec::choice("trigger", "New burst on", &["Beat", "Any hit"]).advanced(),
                ],
            },
            make: || {
                Box::new(Lightning {
                    colour: Rgb::WHITE,
                    length: 0.45,
                    rate: 14.0,
                    forks: true,
                    any_hit: false,
                    watch: HitWatch::default(),
                    left: 0.0,
                    phase: 0.0,
                    burst: 0,
                })
            },
        }
    }

    /// Which flash of the burst this is, and whether it is lit just now.
    fn flash(&self) -> Option<u32> {
        if self.left <= 0.0 {
            return None;
        }
        let index = self.phase.floor() as u32;
        // Lit for half of each flash; and some flashes are skipped, which
        // is what makes it look like weather and not like a strobe.
        let lit = self.phase.fract() < 0.5 && (index == 0 || hash2(self.burst, index) < 0.7);
        lit.then_some(index)
    }
}

impl Effect for Lightning {
    fn configure(&mut self, p: &ParamReader) {
        self.colour = p.color("colour");
        self.length = p.float("length");
        self.rate = p.float("rate");
        self.forks = p.bool("forks");
        self.any_hit = p.choice("trigger") == 1;
    }

    fn update(&mut self, ctx: &FrameCtx) {
        if self.watch.fired(&ctx.audio, self.any_hit) {
            self.left = self.length;
            self.phase = 0.0;
            self.burst = self.burst.wrapping_add(1);
        } else if self.left > 0.0 {
            self.left -= ctx.dt;
            self.phase += ctx.dt * self.rate;
        }
    }

    fn shade(&self, x: f32, _y: f32, _ctx: &FrameCtx) -> Rgb {
        let Some(index) = self.flash() else {
            return Rgb::BLACK;
        };
        if self.forks && index > 0 {
            // After the first flash, each one strikes a different stretch.
            let centre = hash2(self.burst, index + 101);
            let width = 0.25 + 0.5 * hash2(self.burst, index + 202);
            if (x - centre).abs() > width * 0.5 {
                return Rgb::BLACK;
            }
        }
        // Fading as the burst runs out.
        self.colour
            .scale((self.left / self.length.max(0.05)).clamp(0.0, 1.0).sqrt())
    }
}

// ----------------------------------------------------------------- Words

struct Words {
    text: Vec<char>,
    colour: Rgb,
    speed: f32,
    height: f32,
    /// How far the words have travelled, in dots.
    travelled: f32,
}

impl Words {
    /// Dots from the start of one character to the start of the next.
    const PITCH: usize = font::WIDTH + 1;

    fn def() -> EffectDef {
        EffectDef {
            info: EffectInfo {
                id: "words",
                name: "Words",
                description: "A line of text across a matrix, still or scrolling.",
                category: AMBIENT,
                audio: false,
                flash: FlashUse::None,
                params: vec![
                    ParamSpec::words("text", "Words", "darkRGB", 60),
                    ParamSpec::color("colour", "Colour", "#ffffff"),
                    ParamSpec::float("speed", "Scroll speed", 0.0, 12.0, 4.0),
                    ParamSpec::float("height", "Height", 0.3, 1.0, 1.0).advanced(),
                ],
            },
            make: || {
                Box::new(Words {
                    text: "darkRGB".chars().collect(),
                    colour: Rgb::WHITE,
                    speed: 4.0,
                    height: 1.0,
                    travelled: 0.0,
                })
            },
        }
    }

    /// Whether the dot at this column of the whole line and this row is lit.
    fn dot(&self, column: i64, row: usize) -> bool {
        if column < 0 {
            return false;
        }
        let (index, within) = (column as usize / Self::PITCH, column as usize % Self::PITCH);
        self.text.get(index).is_some_and(|c| font::lit(*c, within, row))
    }
}

impl Effect for Words {
    fn configure(&mut self, p: &ParamReader) {
        let text = p.words("text");
        // Compared first: this runs every frame and the words seldom change.
        if !self.text.iter().copied().eq(text.chars()) {
            self.text = text.chars().collect();
            self.travelled = 0.0;
        }
        self.colour = p.color("colour");
        self.speed = p.float("speed");
        self.height = p.float("height");
    }

    fn update(&mut self, ctx: &FrameCtx) {
        // In characters a second.
        self.travelled += ctx.dt * self.speed * Self::PITCH as f32;
    }

    fn shade(&self, x: f32, y: f32, ctx: &FrameCtx) -> Rgb {
        if self.text.is_empty() {
            return Rgb::BLACK;
        }
        let long = (self.text.len() * Self::PITCH) as f32;
        // The font's seven rows fill the chosen share of the height, in
        // the middle. A strip has one row of lights, so it shows whether
        // each column of the words has any dot in it.
        let (row, wide) = if ctx.flat {
            (None, long.min(60.0))
        } else {
            let top = (1.0 - self.height) / 2.0;
            let row = (y - top) / self.height * font::HEIGHT as f32;
            if !(0.0..font::HEIGHT as f32).contains(&row) {
                return Rgb::BLACK;
            }
            // Dots are square: as many columns as the shape has room for.
            (
                Some(row as usize),
                font::HEIGHT as f32 / self.height * ctx.aspect.max(0.1),
            )
        };
        let column = if self.speed > 0.0 {
            // In from the right, out at the left, then round again.
            let lap = long + wide;
            (x * wide + self.travelled % lap - wide).floor() as i64
        } else {
            // Still: in the middle when it fits, from the start when not.
            (x * wide - ((wide - long + 1.0) / 2.0).max(0.0)).floor() as i64
        };
        let lit = match row {
            Some(row) => self.dot(column, row),
            None => (0..font::HEIGHT).any(|row| self.dot(column, row)),
        };
        if lit { self.colour } else { Rgb::BLACK }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioFeatures;
    use crate::effects::find;
    use crate::params::{ParamValue, Params};

    fn ctx(time: f64, audio: AudioFeatures, flat: bool) -> FrameCtx {
        FrameCtx {
            time,
            dt: 1.0 / 60.0,
            audio,
            beats: audio.beat_count as f64 + audio.beat_phase as f64,
            aspect: 1.0,
            flat,
            flash_cap: f32::INFINITY,
        }
    }

    fn make(id: &str, params: &Params) -> Box<dyn Effect> {
        let def = find(id).unwrap();
        let mut fx = (def.make)();
        fx.configure(&ParamReader::new(&def.info.params, params));
        fx
    }

    fn with(pairs: &[(&str, ParamValue)]) -> Params {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    fn brightness(c: Rgb) -> f32 {
        c.r.max(c.g).max(c.b)
    }

    /// A tempo of 120 with the beat counted from the time.
    fn music(t: f64) -> AudioFeatures {
        AudioFeatures {
            bpm: 120.0,
            beat_count: (t * 2.0) as u32,
            beat_phase: (t * 2.0).fract() as f32,
            active: true,
            level: 0.6,
            ..AudioFeatures::default()
        }
    }

    #[test]
    fn stripes_show_the_palettes_own_colours_and_travel() {
        let hard = with(&[("soft", ParamValue::Number(0.0)), ("count", ParamValue::Number(4.0))]);
        let mut fx = make("stripes", &hard);
        let still = ctx(0.0, AudioFeatures::default(), true);
        let colours = &palette("candy_cane").stops;
        // Four stripes across: each quarter is one colour of the palette.
        let first = fx.shade(0.1, 0.5, &still);
        let second = fx.shade(0.35, 0.5, &still);
        assert_eq!(first, colours[0]);
        assert_eq!(second, colours[1 % colours.len()]);
        assert_ne!(first, second);
        // Half a stripe later (one second at 0.5 a second), the edge has moved.
        let edge_before = fx.shade(0.26, 0.5, &still);
        for _ in 0..60 {
            fx.update(&still);
        }
        assert_ne!(fx.shade(0.26, 0.5, &still), edge_before, "the stripes travel");

        // On a matrix they can run across instead of along.
        let across = with(&[
            ("soft", ParamValue::Number(0.0)),
            ("angle", ParamValue::Text("Across".into())),
        ]);
        let fx = make("stripes", &across);
        let matrix = ctx(0.0, AudioFeatures::default(), false);
        assert_eq!(fx.shade(0.1, 0.3, &matrix), fx.shade(0.9, 0.3, &matrix));
        assert_ne!(fx.shade(0.5, 0.05, &matrix), fx.shade(0.5, 0.18, &matrix));
    }

    #[test]
    fn twinkle_lights_about_as_many_bulbs_as_asked_and_never_goes_out() {
        let lit = |amount: f32| {
            let fx = make("twinkle", &with(&[("amount", ParamValue::Number(amount))]));
            let c = ctx(3.0, AudioFeatures::default(), true);
            (0..400)
                .filter(|i| brightness(fx.shade(*i as f32 / 400.0, 0.5, &c)) > 0.0)
                .count()
        };
        let (few, most) = (lit(0.2), lit(0.9));
        assert!((40..=130).contains(&few), "{few} of 400 lit at 0.2");
        assert!((320..=390).contains(&most), "{most} of 400 lit at 0.9");

        // A lit bulb changes over time and keeps its resting glow.
        let fx = make("twinkle", &with(&[("amount", ParamValue::Number(1.0))]));
        let seen: Vec<f32> = (0..200)
            .map(|frame| brightness(fx.shade(0.37, 0.5, &ctx(frame as f64 * 0.1, AudioFeatures::default(), true))))
            .collect();
        let (low, high) = seen
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        assert!(high > 0.9 && (0.1..0.5).contains(&low), "from {low} to {high}");
    }

    #[test]
    fn snow_falls_down_a_matrix_and_along_a_strip() {
        // Where the light is in one lane, top to bottom, over time.
        let mut fx = make(
            "snow",
            &with(&[("amount", ParamValue::Number(1.0)), ("wind", ParamValue::Number(0.0))]),
        );
        let c = ctx(0.0, AudioFeatures::default(), false);
        let brightest_row = |fx: &dyn Effect| {
            (0..200)
                .map(|i| (i as f32 / 200.0, brightness(fx.shade(0.52, i as f32 / 200.0, &c))))
                .fold((0.0, 0.0), |best, row| if row.1 > best.1 { row } else { best })
        };
        let (before, light) = brightest_row(fx.as_ref());
        assert!(light > 0.3, "a flake is somewhere in the lane");
        for _ in 0..12 {
            fx.update(&c);
        }
        let (after, _) = brightest_row(fx.as_ref());
        let moved = (after - before).rem_euclid(1.0);
        assert!(moved > 0.01 && moved < 0.5, "moved down by {moved}");

        // With little snow, most of the picture is dark.
        let fx = make("snow", &with(&[("amount", ParamValue::Number(0.1))]));
        let dark = (0..400)
            .filter(|i| brightness(fx.shade((*i % 20) as f32 / 20.0, (*i / 20) as f32 / 20.0, &c)) < 0.05)
            .count();
        assert!(dark > 300, "{dark} of 400 dark");

        // On a strip there is snow too.
        let fx = make("snow", &with(&[("amount", ParamValue::Number(1.0))]));
        let strip = ctx(0.0, AudioFeatures::default(), true);
        assert!((0..300).any(|i| brightness(fx.shade(i as f32 / 300.0, 0.5, &strip)) > 0.5));
    }

    #[test]
    fn steps_change_colour_on_the_beat_and_keep_changing_without_one() {
        let hard = with(&[("fade", ParamValue::Number(0.0)), ("blocks", ParamValue::Number(2.0))]);
        let mut fx = make("steps", &hard);
        let at = |fx: &mut Box<dyn Effect>, t: f64| {
            let c = ctx(t, music(t), true);
            fx.update(&c);
            (fx.shade(0.2, 0.5, &c), fx.shade(0.8, 0.5, &c))
        };
        let (left, right) = at(&mut fx, 0.1);
        assert_ne!(left, right, "two blocks, two colours");
        assert_eq!(at(&mut fx, 0.4), (left, right), "steady within a beat");
        let (next_left, _) = at(&mut fx, 0.6);
        assert_eq!(next_left, right, "on the beat each block takes the next colour");

        // Every 4 beats: the same colour two seconds on is still the first step.
        let slow = with(&[
            ("fade", ParamValue::Number(0.0)),
            ("every", ParamValue::Text("4 beats".into())),
        ]);
        let mut fx = make("steps", &slow);
        assert_eq!(at(&mut fx, 0.1).0, at(&mut fx, 1.9).0);
        assert_ne!(at(&mut fx, 0.1).0, at(&mut fx, 2.1).0);

        // No tempo: one change a second by default.
        let mut fx = make("steps", &hard);
        let quiet = ctx(0.0, AudioFeatures::default(), true);
        let first = fx.shade(0.2, 0.5, &quiet);
        for _ in 0..66 {
            fx.update(&quiet);
        }
        assert_ne!(fx.shade(0.2, 0.5, &quiet), first);
    }

    #[test]
    fn words_are_drawn_in_dots_and_scroll() {
        // A matrix exactly one character wide and high: 6 by 7 dots.
        let mut c = ctx(0.0, AudioFeatures::default(), false);
        c.aspect = 6.0 / 7.0;
        let still = with(&[
            ("text", ParamValue::Text("T".into())),
            ("speed", ParamValue::Number(0.0)),
        ]);
        let fx = make("words", &still);
        let picture: Vec<String> = (0..7)
            .map(|row| {
                (0..6)
                    .map(|col| {
                        let lit = brightness(fx.shade((col as f32 + 0.5) / 6.0, (row as f32 + 0.5) / 7.0, &c)) > 0.0;
                        if lit { '#' } else { '.' }
                    })
                    .collect()
            })
            .collect();
        assert_eq!(
            picture,
            ["#####.", "..#...", "..#...", "..#...", "..#...", "..#...", "..#..."],
            "a T, with the gap that follows every character"
        );

        // Scrolling: it starts off the right-hand edge and comes across.
        let moving = with(&[("text", ParamValue::Text("HI".into()))]);
        let mut fx = make("words", &moving);
        let lit = |fx: &dyn Effect| {
            (0..42)
                .filter(|i| brightness(fx.shade((i % 6) as f32 / 6.0 + 0.08, (i / 6) as f32 / 7.0 + 0.07, &c)) > 0.0)
                .count()
        };
        assert_eq!(lit(fx.as_ref()), 0, "nothing has arrived yet");
        let mut seen = 0;
        for _ in 0..240 {
            fx.update(&c);
            seen = seen.max(lit(fx.as_ref()));
        }
        assert!(seen >= 10, "the letters came past: {seen} dots at most");
        // Empty words are dark, and characters outside the font are question marks.
        let fx = make("words", &with(&[("text", ParamValue::Text("".into()))]));
        assert_eq!(fx.shade(0.5, 0.5, &c), Rgb::BLACK);
        let fx = make(
            "words",
            &with(&[
                ("text", ParamValue::Text("é".into())),
                ("speed", ParamValue::Number(0.0)),
            ]),
        );
        assert!(
            (0..42).any(|i| brightness(fx.shade((i % 6) as f32 / 6.0 + 0.08, (i / 6) as f32 / 7.0 + 0.07, &c)) > 0.0)
        );
        // On a strip, columns with dots light up.
        let strip = ctx(0.0, AudioFeatures::default(), true);
        let fx = make("words", &still);
        assert!((0..60).any(|i| brightness(fx.shade(i as f32 / 60.0, 0.5, &strip)) > 0.0));
    }

    #[test]
    fn lightning_is_a_short_burst_of_fast_flashes_on_a_beat() {
        let whole = with(&[("forks", ParamValue::Bool(false))]);
        let mut fx = make("lightning", &whole);
        // Dark until a beat lands.
        let quiet = ctx(0.0, AudioFeatures::default(), true);
        fx.update(&quiet);
        assert_eq!(fx.shade(0.5, 0.5, &quiet), Rgb::BLACK);

        let beat = AudioFeatures {
            beat_count: 1,
            bpm: 120.0,
            active: true,
            ..AudioFeatures::default()
        };
        let c = ctx(0.0, beat, true);
        let (mut flashes, mut was_lit, mut last_lit) = (0, false, 0);
        for frame in 0..120 {
            fx.update(&c);
            let lit = brightness(fx.shade(0.5, 0.5, &c)) > 0.0;
            if lit && !was_lit {
                flashes += 1;
            }
            if lit {
                last_lit = frame;
            }
            was_lit = lit;
        }
        assert!((3..=7).contains(&flashes), "{flashes} flashes in one burst");
        assert!(
            last_lit < 32,
            "over within about half a second, was lit at frame {last_lit}"
        );

        // It is declared as needing fast flashes, so a show with the
        // flash limit on leaves it out.
        assert_eq!(find("lightning").unwrap().info.flash, FlashUse::Needed);
    }
}
