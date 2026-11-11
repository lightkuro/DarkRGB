//! Core model for darkRGB: colours, layout, effects and rendering.
//!
//! This crate does no I/O. Audio capture lives in `darkrgb-audio`, device output
//! in `darkrgb-output`, and the application in `darkrgb-engine`.

pub mod audio;
pub mod color;
pub mod effect;
pub mod effects;
pub mod layout;
pub mod params;
pub mod render;
pub mod safety;
pub mod show;

pub use audio::{AudioFeatures, BANDS, Band};
pub use color::{Palette, Rgb};
pub use effect::{Effect, EffectInfo, FrameCtx};
pub use layout::{Device, DeviceKind, Fixture, Shape};
pub use params::{ParamKind, ParamSpec, ParamValue, Params};
pub use render::Renderer;
pub use show::{Action, Binding, Blend, KeyMode, Launch, Layer, LayerGroup, LightGroup, Scene, Show, Sound};
