//! Devices on the network and the fixtures (strips, matrices) attached to them.

use serde::{Deserialize, Serialize};

/// Most pixels one device or fixture may have. Far above any real
/// controller, and low enough that a device (or a hand-edited file)
/// claiming billions cannot exhaust memory.
pub const MAX_PIXELS: u32 = 100_000;

/// What a device is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// A WLED controller on the network.
    #[default]
    Wled,
    /// Exists only in the preview. Nothing is ever sent for it. Used for
    /// the demo lights and for planning a layout before owning hardware.
    Virtual,
    /// A pixel controller that takes sACN (E1.31).
    Sacn,
    /// A pixel controller that takes Art-Net.
    Artnet,
}

/// Something that shows pixels: a controller on the network, or a
/// virtual light that exists only in the preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub kind: DeviceKind,
    /// True for the built-in demo lights, so they can be removed as a set.
    #[serde(default)]
    pub demo: bool,
    /// IP address or host name. Empty for a virtual light.
    pub address: String,
    /// UDP port for DDP. Real WLED devices always use 4048.
    #[serde(default = "default_ddp_port")]
    pub port: u16,
    /// Total pixels the device drives.
    pub pixel_count: u32,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Gamma applied before sending. LEDs are linear, screens are not.
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    /// Upper limit on brightness for this device, 0..1.
    #[serde(default = "one")]
    pub brightness: f32,
    /// Hardware address, used to recognise a device whose IP changed.
    #[serde(default)]
    pub mac: Option<String>,
    /// Firmware version reported by the device, for display.
    #[serde(default)]
    pub version: Option<String>,
    /// For sACN and Art-Net: the universe its first pixel is in. The
    /// rest follow, 170 pixels to a universe.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub universe: u16,
}

fn is_zero(n: &u16) -> bool {
    *n == 0
}

/// Whole RGB pixels that fit in one universe of 512 channels.
pub const PIXELS_PER_UNIVERSE: u32 = 170;

/// How many universes this many pixels need.
pub fn universes(pixels: u32) -> u32 {
    pixels.div_ceil(PIXELS_PER_UNIVERSE).max(1)
}

impl DeviceKind {
    /// The first and last universe the protocol has, for the kinds that
    /// have universes at all.
    pub fn universe_range(self) -> Option<(u16, u16)> {
        match self {
            Self::Sacn => Some((1, 63999)),
            Self::Artnet => Some((0, 32767)),
            Self::Wled | Self::Virtual => None,
        }
    }

    /// Whether `pixels` starting at universe `first` stay inside the
    /// universes the protocol has. On failure, the range and the last
    /// universe that would be needed.
    pub fn universes_fit(self, first: u16, pixels: u32) -> Result<(), (u16, u16, u32)> {
        let Some((low, high)) = self.universe_range() else {
            return Ok(());
        };
        let last = first as u32 + universes(pixels) - 1;
        if first < low || last > high as u32 {
            return Err((low, high, last));
        }
        Ok(())
    }
}

impl Device {
    /// A WLED device with default settings. `id` is assigned when the
    /// device is added to a show.
    pub fn wled(name: &str, address: &str, port: u16, pixel_count: u32) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            kind: DeviceKind::Wled,
            demo: false,
            address: address.to_string(),
            port,
            pixel_count,
            enabled: true,
            gamma: default_gamma(),
            brightness: 1.0,
            mac: None,
            version: None,
            universe: 0,
        }
    }

    /// Puts the first universe where the protocol allows it. Returns
    /// whether anything changed. For files edited by hand.
    pub fn mend_universe(&mut self) -> bool {
        let wanted = match self.kind.universe_range() {
            None => 0,
            Some((low, high)) => {
                let room = (high as u32 + 1)
                    .saturating_sub(universes(self.pixel_count))
                    .max(low as u32);
                (self.universe as u32).clamp(low as u32, room) as u16
            }
        };
        std::mem::replace(&mut self.universe, wanted) != wanted
    }

    /// A pixel controller that takes sACN or Art-Net, starting at `universe`.
    pub fn dmx(kind: DeviceKind, name: &str, address: &str, port: u16, pixel_count: u32, universe: u16) -> Self {
        Self {
            kind,
            universe,
            ..Self::wled(name, address, port, pixel_count)
        }
    }

    /// A light that exists only in the preview.
    pub fn virtual_light(name: &str, pixel_count: u32) -> Self {
        Self {
            kind: DeviceKind::Virtual,
            ..Self::wled(name, "", default_ddp_port(), pixel_count)
        }
    }

    /// Whether frames should be sent over the network for this device.
    pub fn sends(&self) -> bool {
        self.kind != DeviceKind::Virtual && self.enabled
    }
}

pub fn default_ddp_port() -> u16 {
    4048
}
fn default_gamma() -> f32 {
    2.2
}
fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    /// Pixels in a line.
    Strip {
        count: u32,
        #[serde(default)]
        reverse: bool,
    },
    /// Pixels in a grid, row by row from the top left.
    Matrix {
        cols: u32,
        rows: u32,
        /// Every second row runs right to left. WLED untangles its own
        /// matrices, so this stays off for WLED.
        #[serde(default)]
        serpentine: bool,
    },
}

impl Shape {
    pub fn pixel_count(&self) -> u32 {
        match *self {
            Shape::Strip { count, .. } => count,
            Shape::Matrix { cols, rows, .. } => cols.saturating_mul(rows),
        }
    }
}

/// A strip or matrix placed on the canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fixture {
    pub id: u32,
    pub name: String,
    /// The device this fixture is wired to.
    pub device: u32,
    /// Index of the fixture's first pixel on that device.
    #[serde(default)]
    pub start: u32,
    pub shape: Shape,
    /// Centre on the canvas.
    pub x: f32,
    pub y: f32,
    /// Size on the canvas. A strip ignores `height`.
    pub width: f32,
    pub height: f32,
    /// Clockwise rotation in degrees.
    #[serde(default)]
    pub rotation: f32,
    /// The group of lights this fixture is filed under, if any.
    #[serde(default)]
    pub group: Option<u32>,
    /// For a strip that does not run straight: the corners it passes
    /// through, each as a place in the fixture's box from (0, 0) at the
    /// top left to (1, 1) at the bottom right. Its LEDs are spread evenly
    /// along the line through them. Empty for a straight strip.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<[f32; 2]>,
    /// The line through `path` runs on from its last corner to its first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
}

/// The most corners a strip's path can have.
pub const MAX_PATH_POINTS: usize = 64;

impl Fixture {
    pub fn pixel_count(&self) -> u32 {
        self.shape.pixel_count()
    }

    /// Puts right a path from a file edited by hand: corners inside the
    /// box, not too many of them, and none at all on a matrix.
    pub fn mend_path(&mut self) {
        if !matches!(self.shape, Shape::Strip { .. }) {
            self.path.clear();
        }
        self.path.retain(|p| p[0].is_finite() && p[1].is_finite());
        self.path.truncate(MAX_PATH_POINTS);
        if self.path.len() < 2 {
            self.path.clear();
        }
        for p in &mut self.path {
            *p = [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)];
        }
        self.closed &= self.path.len() >= 3;
        if self.bent() && !(self.height.is_finite() && self.height > 0.0) {
            self.height = self.width;
        }
    }

    /// A strip that follows a path instead of running straight.
    pub fn bent(&self) -> bool {
        matches!(self.shape, Shape::Strip { .. }) && self.path.len() >= 2
    }

    /// The corners of a bent strip in the fixture's own frame, centred on
    /// zero, with the first repeated at the end when the path is closed.
    fn corners(&self) -> Vec<[f32; 2]> {
        let mut corners: Vec<[f32; 2]> = self
            .path
            .iter()
            .map(|p| [(p[0] - 0.5) * self.width, (p[1] - 0.5) * self.height])
            .collect();
        if self.closed && corners.len() >= 2 {
            corners.push(corners[0]);
        }
        corners
    }

    /// How long a bent strip's path is on the canvas. For a straight one,
    /// its width.
    pub fn run_length(&self) -> f32 {
        if !self.bent() {
            return self.width;
        }
        let corners = self.corners();
        corners
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
            .sum()
    }

    /// Where each of `count` LEDs sits along the path, in the fixture's
    /// own frame: evenly spaced by distance, half a step in from each end.
    fn along_path(&self, count: u32) -> Vec<[f32; 2]> {
        let corners = self.corners();
        let total = self.run_length();
        let mut out = Vec::with_capacity(count as usize);
        if total <= 1e-9 {
            out.resize(count as usize, corners[0]);
            return out;
        }
        let (mut leg, mut before) = (0usize, 0.0f32);
        for i in 0..count {
            let at = (i as f32 + 0.5) / count as f32 * total;
            // Walk on to the leg this distance falls in.
            loop {
                let (a, b) = (corners[leg], corners[leg + 1]);
                let length = (b[0] - a[0]).hypot(b[1] - a[1]);
                if at <= before + length || leg + 2 >= corners.len() {
                    let t = if length > 1e-9 {
                        ((at - before) / length).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                    break;
                }
                before += length;
                leg += 1;
            }
        }
        out
    }

    /// Canvas position of every pixel, in wiring order.
    pub fn pixel_positions(&self) -> Vec<[f32; 2]> {
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let place = |lx: f32, ly: f32| [self.x + lx * cos - ly * sin, self.y + lx * sin + ly * cos];
        // Centre of cell `i` out of `n`, across a span centred on zero.
        let cell = |i: u32, n: u32, span: f32| ((i as f32 + 0.5) / n as f32 - 0.5) * span;

        match self.shape {
            Shape::Strip { count, reverse } if self.bent() => {
                let mut along = self.along_path(count);
                if reverse {
                    along.reverse();
                }
                along.into_iter().map(|[lx, ly]| place(lx, ly)).collect()
            }
            Shape::Strip { count, reverse } => (0..count)
                .map(|i| {
                    let i = if reverse { count - 1 - i } else { i };
                    place(cell(i, count, self.width), 0.0)
                })
                .collect(),
            Shape::Matrix { cols, rows, serpentine } => {
                let mut out = Vec::with_capacity((cols * rows) as usize);
                for row in 0..rows {
                    for col in 0..cols {
                        let col = if serpentine && row % 2 == 1 {
                            cols - 1 - col
                        } else {
                            col
                        };
                        out.push(place(cell(col, cols, self.width), cell(row, rows, self.height)));
                    }
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(shape: Shape) -> Fixture {
        Fixture {
            id: 1,
            name: "f".into(),
            device: 1,
            start: 0,
            shape,
            x: 10.0,
            y: 5.0,
            width: 4.0,
            height: 2.0,
            rotation: 0.0,
            group: None,
            path: Vec::new(),
            closed: false,
        }
    }

    #[test]
    fn a_bent_strip_spreads_its_leds_evenly_along_its_corners() {
        // An L in a box 4 wide and 2 high: down the left side (2 long),
        // then along the bottom (4 long). Six LEDs, one for each unit.
        let mut f = fixture(Shape::Strip {
            count: 6,
            reverse: false,
        });
        f.path = vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        assert!(f.bent());
        assert!((f.run_length() - 6.0).abs() < 1e-5);
        let p = f.pixel_positions();
        let expect = [[8.0, 4.5], [8.0, 5.5], [8.5, 6.0], [9.5, 6.0], [10.5, 6.0], [11.5, 6.0]];
        for (got, want) in p.iter().zip(expect) {
            assert!(close(*got, want), "{got:?} should be {want:?}");
        }
        // Wired from the other end.
        f.shape = Shape::Strip {
            count: 6,
            reverse: true,
        };
        assert!(close(f.pixel_positions()[0], [11.5, 6.0]));

        // Closed, the line runs back to where it began: a square of 12.
        let mut f = fixture(Shape::Strip {
            count: 12,
            reverse: false,
        });
        f.path = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        f.closed = true;
        assert!((f.run_length() - 12.0).abs() < 1e-5);
        let p = f.pixel_positions();
        assert!(
            close(p[0], [8.5, 4.0]) && close(p[11], [8.0, 4.5]),
            "{:?} {:?}",
            p[0],
            p[11]
        );
        // Turned a quarter, the whole shape turns about the fixture's centre.
        f.rotation = 90.0;
        assert!(
            close(f.pixel_positions()[0], [11.0, 3.5]),
            "{:?}",
            f.pixel_positions()[0]
        );

        // Corners on top of each other do no harm, and one corner is no path.
        let mut f = fixture(Shape::Strip {
            count: 3,
            reverse: false,
        });
        f.path = vec![[0.5, 0.5], [0.5, 0.5]];
        assert!(f.pixel_positions().iter().all(|p| close(*p, [10.0, 5.0])));
        f.path = vec![[0.2, 0.2]];
        assert!(!f.bent());
        assert!(close(f.pixel_positions()[0], [10.0 - 4.0 / 3.0, 5.0]));
        // A matrix takes no notice of a path, and a show from before has none.
        let mut m = fixture(Shape::Matrix {
            cols: 2,
            rows: 2,
            serpentine: false,
        });
        let before = m.pixel_positions();
        m.path = vec![[0.0, 0.0], [1.0, 1.0]];
        assert_eq!(m.pixel_positions(), before);
        let text = serde_json::to_string(&fixture(Shape::Strip {
            count: 3,
            reverse: false,
        }))
        .unwrap();
        assert!(!text.contains("path") && !text.contains("closed"));
        assert!(serde_json::from_str::<Fixture>(&text).unwrap().path.is_empty());
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    #[test]
    fn universes_hold_170_pixels_and_stay_in_range() {
        assert_eq!(
            (
                universes(0),
                universes(1),
                universes(170),
                universes(171),
                universes(1000)
            ),
            (1, 1, 1, 2, 6)
        );
        assert_eq!(DeviceKind::Sacn.universes_fit(1, 170), Ok(()));
        assert_eq!(DeviceKind::Sacn.universes_fit(0, 1), Err((1, 63999, 0)));
        assert_eq!(DeviceKind::Sacn.universes_fit(63999, 171), Err((1, 63999, 64000)));
        assert_eq!(DeviceKind::Artnet.universes_fit(0, 1), Ok(()));
        assert_eq!(DeviceKind::Artnet.universes_fit(32767, 171), Err((0, 32767, 32768)));
        // The others have no universes to get wrong.
        assert_eq!(DeviceKind::Wled.universes_fit(9, 100_000), Ok(()));

        // A file edited by hand is put right.
        let mut device = Device::dmx(DeviceKind::Sacn, "d", "10.0.0.9", 5568, 400, 0);
        assert!(device.mend_universe());
        assert_eq!(device.universe, 1);
        assert!(!device.mend_universe());
        device.universe = 63999;
        assert!(device.mend_universe());
        assert_eq!(device.universe, 63997, "three universes end at the last one");
        let mut wled = Device::wled("d", "10.0.0.9", 4048, 10);
        wled.universe = 5;
        assert!(wled.mend_universe());
        assert_eq!(wled.universe, 0);
        // And a file from before universes existed still opens.
        let old = serde_json::to_string(&Device::wled("d", "10.0.0.9", 4048, 10)).unwrap();
        assert!(!old.contains("universe"));
        assert_eq!(serde_json::from_str::<Device>(&old).unwrap().universe, 0);
        let kinds = serde_json::to_string(&[DeviceKind::Sacn, DeviceKind::Artnet]).unwrap();
        assert_eq!(kinds, r#"["sacn","artnet"]"#);
    }

    #[test]
    fn strip_runs_left_to_right() {
        let f = fixture(Shape::Strip {
            count: 4,
            reverse: false,
        });
        let p = f.pixel_positions();
        assert_eq!(p.len(), 4);
        assert!(close(p[0], [8.5, 5.0]));
        assert!(close(p[3], [11.5, 5.0]));
    }

    #[test]
    fn reversed_strip_runs_right_to_left() {
        let f = fixture(Shape::Strip {
            count: 4,
            reverse: true,
        });
        let p = f.pixel_positions();
        assert!(close(p[0], [11.5, 5.0]));
        assert!(close(p[3], [8.5, 5.0]));
    }

    #[test]
    fn rotated_strip_runs_downward() {
        let mut f = fixture(Shape::Strip {
            count: 2,
            reverse: false,
        });
        f.rotation = 90.0;
        let p = f.pixel_positions();
        assert!(close(p[0], [10.0, 4.0]));
        assert!(close(p[1], [10.0, 6.0]));
    }

    #[test]
    fn matrix_is_row_major_from_top_left() {
        let f = fixture(Shape::Matrix {
            cols: 2,
            rows: 2,
            serpentine: false,
        });
        let p = f.pixel_positions();
        assert!(close(p[0], [9.0, 4.5]));
        assert!(close(p[1], [11.0, 4.5]));
        assert!(close(p[2], [9.0, 5.5]));
        assert!(close(p[3], [11.0, 5.5]));
    }

    #[test]
    fn serpentine_matrix_flips_odd_rows() {
        let f = fixture(Shape::Matrix {
            cols: 2,
            rows: 2,
            serpentine: true,
        });
        let p = f.pixel_positions();
        assert!(close(p[2], [11.0, 5.5]));
        assert!(close(p[3], [9.0, 5.5]));
    }
}
