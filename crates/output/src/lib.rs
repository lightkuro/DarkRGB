//! Device output: the DDP pixel protocol, WLED's HTTP API, and discovery.

pub mod ddp;
pub mod discovery;
pub mod outputs;
pub mod wled;

pub use outputs::{DeviceStats, Outputs};
