//! Audio capture and analysis.

pub mod analysis;
#[cfg(not(target_os = "linux"))]
mod backend_cpal;
#[cfg(target_os = "linux")]
mod backend_pulse;
pub mod capture;

pub use analysis::Analyzer;
pub use capture::{list_inputs, AudioCapture, AudioState, AudioStatus, InputDevice, SYSTEM_SOURCE};
