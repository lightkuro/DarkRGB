//! Device output: the DDP pixel protocol, sACN and Art-Net, OpenRGB,
//! serial strips, WLED's HTTP API, and discovery.

pub mod ddp;
pub mod discovery;
pub mod dmx;
pub mod openrgb;
pub mod outputs;
pub mod scan;
pub mod serial;
pub mod wled;

pub use outputs::{DeviceStats, Outputs};
