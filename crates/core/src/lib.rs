//! Core model for darkRGB: colours, layout, effects and rendering.
//!
//! This crate does no I/O. Audio capture lives in `darkrgb-audio`, device output
//! in `darkrgb-output`, and the application in `darkrgb-engine`.

pub mod audio;
pub mod color;
pub mod effect;
pub mod effects;
mod extra_effects;
pub mod font;
pub mod layout;
pub mod looks;
mod more_effects;
pub mod params;
pub mod picture;
pub mod render;
pub mod safety;
pub mod show;
pub mod timeline;

pub use audio::{AudioFeatures, BANDS, Band};
pub use color::{Pal, Palette, Rgb};
pub use effect::{Effect, EffectInfo, FlashUse, FrameCtx};
pub use layout::{Device, DeviceKind, Fixture, Shape};
pub use params::{ParamKind, ParamSpec, ParamValue, Params};
pub use render::{Cues, LayerHit, Renderer};
pub use show::{
    Action, Autopilot, Binding, Blend, FlashSettings, Idle, KeyMode, Kind, Knob, KnobTarget, Launch, Layer, LayerGroup,
    LightGroup, Link, MidiNote, Mirror, Mood, Only, Scene, Schedule, Show, Sound, Source, Swatch,
};
