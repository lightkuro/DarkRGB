//! Core model for OpenLightShow: colours, layout, effects and rendering.
//!
//! This crate does no I/O. Audio capture lives in `ols-audio`, device output
//! in `ols-output`, and the application in `ols-engine`.

pub mod audio;
pub mod color;
pub mod effect;
pub mod effects;
pub mod layout;
pub mod params;
pub mod render;
pub mod show;

pub use audio::{AudioFeatures, BANDS};
pub use color::{Palette, Rgb};
pub use effect::{Effect, EffectInfo, FrameCtx};
pub use layout::{Device, Fixture, Shape};
pub use params::{ParamKind, ParamSpec, ParamValue, Params};
pub use render::Renderer;
pub use show::{Blend, Layer, Scene, Show};
