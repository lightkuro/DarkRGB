//! Devices on the network and the fixtures (strips, matrices) attached to them.

use serde::{Deserialize, Serialize};

/// Something on the network that receives pixel data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: u32,
    pub name: String,
    /// IP address or host name.
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
}

impl Fixture {
    pub fn pixel_count(&self) -> u32 {
        self.shape.pixel_count()
    }

    /// Canvas position of every pixel, in wiring order.
    pub fn pixel_positions(&self) -> Vec<[f32; 2]> {
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let place = |lx: f32, ly: f32| [self.x + lx * cos - ly * sin, self.y + lx * sin + ly * cos];
        // Centre of cell `i` out of `n`, across a span centred on zero.
        let cell = |i: u32, n: u32, span: f32| ((i as f32 + 0.5) / n as f32 - 0.5) * span;

        match self.shape {
            Shape::Strip { count, reverse } => (0..count)
                .map(|i| {
                    let i = if reverse { count - 1 - i } else { i };
                    place(cell(i, count, self.width), 0.0)
                })
                .collect(),
            Shape::Matrix {
                cols,
                rows,
                serpentine,
            } => {
                let mut out = Vec::with_capacity((cols * rows) as usize);
                for row in 0..rows {
                    for col in 0..cols {
                        let col = if serpentine && row % 2 == 1 { cols - 1 - col } else { col };
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
        }
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
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
