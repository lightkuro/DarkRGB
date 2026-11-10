//! Turns a show plus audio features into pixel colours, frame by frame.

use std::collections::HashMap;

use crate::audio::AudioFeatures;
use crate::color::{OutputCurve, Rgb};
use crate::effect::{Effect, FrameCtx};
use crate::effects;
use crate::layout::{Fixture, Shape};
use crate::params::ParamReader;
use crate::show::{Blend, Layer, Show};

/// A pixel a layer draws on, with its position inside the layer's area.
struct Target {
    index: u32,
    x: f32,
    y: f32,
}

struct LayerState {
    effect_id: String,
    effect: Box<dyn Effect>,
    targets: Vec<Target>,
    aspect: f32,
    flat: bool,
}

/// Keeps effect state between frames and caches pixel positions.
///
/// Call [`Renderer::sync`] whenever the show changes, then
/// [`Renderer::render`] once per frame.
#[derive(Default)]
pub struct Renderer {
    /// Canvas position of every pixel, fixture by fixture.
    positions: Vec<[f32; 2]>,
    /// Fixtures and layer targets as they were when the caches were built.
    layout_key: Vec<Fixture>,
    target_key: Vec<(u32, Vec<u32>)>,
    layers: HashMap<u32, LayerState>,
    frame: Vec<Rgb>,
    scratch: Vec<Rgb>,
}

impl Renderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Canvas positions of all pixels in frame order.
    pub fn positions(&self) -> &[[f32; 2]] {
        &self.positions
    }

    /// Rebuilds whatever the latest change invalidated. Effect state (a
    /// scroll's history, say) survives parameter and layout changes.
    pub fn sync(&mut self, show: &Show) {
        let layout_changed = self.layout_key != show.fixtures;
        if layout_changed {
            self.positions.clear();
            for fixture in &show.fixtures {
                self.positions.extend(fixture.pixel_positions());
            }
            self.layout_key = show.fixtures.clone();
            self.frame.resize(self.positions.len(), Rgb::BLACK);
            self.scratch.resize(self.positions.len(), Rgb::BLACK);
        }

        let target_key: Vec<(u32, Vec<u32>)> = show.layers.iter().map(|l| (l.id, l.targets.clone())).collect();
        let targets_changed = layout_changed || target_key != self.target_key;
        self.target_key = target_key;

        self.layers.retain(|id, _| show.layers.iter().any(|l| l.id == *id));
        for layer in &show.layers {
            let Some(def) = effects::find(&layer.effect) else {
                self.layers.remove(&layer.id);
                continue;
            };
            let stale = self.layers.get(&layer.id).is_none_or(|s| s.effect_id != layer.effect);
            if stale {
                self.layers.insert(
                    layer.id,
                    LayerState {
                        effect_id: layer.effect.clone(),
                        effect: (def.make)(),
                        targets: Vec::new(),
                        aspect: 1.0,
                        flat: true,
                    },
                );
            }
            if stale || targets_changed {
                let state = self.layers.get_mut(&layer.id).expect("just inserted");
                let (targets, aspect, flat) = build_targets(show, layer, &self.positions);
                state.targets = targets;
                state.aspect = aspect;
                state.flat = flat;
            }
        }
    }

    /// Renders one frame. Colours are display-referred, before master
    /// brightness and device gamma.
    pub fn render(&mut self, show: &Show, time: f64, dt: f32, audio: &AudioFeatures) -> &[Rgb] {
        self.frame.fill(Rgb::BLACK);
        for layer in &show.layers {
            if !layer.enabled || layer.opacity <= 0.0 {
                continue;
            }
            let (Some(state), Some(def)) = (self.layers.get_mut(&layer.id), effects::find(&layer.effect)) else {
                continue;
            };
            let ctx = FrameCtx {
                time,
                dt,
                audio: *audio,
                aspect: state.aspect,
                flat: state.flat,
            };
            state.effect.configure(&ParamReader::new(&def.info.params, &layer.params));
            state.effect.update(&ctx);
            let opacity = layer.opacity.clamp(0.0, 1.0);
            for t in &state.targets {
                let src = state.effect.shade(t.x, t.y, &ctx).clamp();
                let dst = &mut self.frame[t.index as usize];
                *dst = match layer.blend {
                    Blend::Normal => dst.lerp(src, opacity),
                    Blend::Add => dst.add(src.scale(opacity)).clamp(),
                    Blend::Lighten => dst.max(src.scale(opacity)),
                    Blend::Multiply => dst.lerp(dst.mul(src), opacity),
                };
            }
        }
        let master = if show.master.blackout { 0.0 } else { show.master.brightness.clamp(0.0, 1.0) };
        if master < 1.0 {
            for px in &mut self.frame {
                *px = px.scale(master);
            }
        }
        &self.frame
    }

    /// The last rendered frame as 8-bit RGB with no gamma, for the preview.
    pub fn preview_bytes(&self, out: &mut Vec<u8>) {
        out.clear();
        out.reserve(self.frame.len() * 3);
        for px in &self.frame {
            out.extend_from_slice(&px.to_bytes());
        }
    }

    /// The last rendered frame as the bytes to send to one device: 8-bit
    /// RGB in the device's own pixel order, with its gamma and brightness
    /// limit applied. Pixels no fixture covers are black.
    pub fn device_bytes(&self, show: &Show, device_id: u32, out: &mut Vec<u8>) {
        out.clear();
        let Some(device) = show.device(device_id) else {
            return;
        };
        let pixels = device.pixel_count as usize;
        out.resize(pixels * 3, 0);
        let curve = OutputCurve::new(device.gamma, device.brightness);
        let mut offset = 0usize;
        for fixture in &show.fixtures {
            let count = fixture.pixel_count() as usize;
            if fixture.device == device_id {
                for i in 0..count {
                    let at = fixture.start as usize + i;
                    if at >= pixels {
                        break;
                    }
                    let px = self.frame.get(offset + i).copied().unwrap_or(Rgb::BLACK);
                    out[at * 3] = curve.apply(px.r);
                    out[at * 3 + 1] = curve.apply(px.g);
                    out[at * 3 + 2] = curve.apply(px.b);
                }
            }
            offset += count;
        }
    }
}

/// Works out which pixels a layer draws on and where each one sits inside
/// the box around all of them.
fn build_targets(show: &Show, layer: &Layer, positions: &[[f32; 2]]) -> (Vec<Target>, f32, bool) {
    let mut picked: Vec<(u32, [f32; 2])> = Vec::new();
    let mut offset = 0u32;
    // A single unrotated strip is a line even when it has been placed at an
    // angle, so it is treated as flat and mapped along its own length.
    let mut lone_strip: Option<&Fixture> = None;
    let mut fixtures_hit = 0;
    for fixture in &show.fixtures {
        let count = fixture.pixel_count();
        if layer.targets.is_empty() || layer.targets.contains(&fixture.id) {
            fixtures_hit += 1;
            lone_strip = matches!(fixture.shape, Shape::Strip { .. }).then_some(fixture);
            for i in 0..count {
                picked.push((offset + i, positions[(offset + i) as usize]));
            }
        }
        offset += count;
    }
    if picked.is_empty() {
        return (Vec::new(), 1.0, true);
    }

    if let (1, Some(strip)) = (fixtures_hit, lone_strip) {
        let n = picked.len() as f32;
        let reverse = matches!(strip.shape, Shape::Strip { reverse: true, .. });
        let targets = picked
            .iter()
            .enumerate()
            .map(|(i, (index, _))| {
                let along = (i as f32 + 0.5) / n;
                Target {
                    index: *index,
                    x: if reverse { 1.0 - along } else { along },
                    y: 0.5,
                }
            })
            .collect();
        return (targets, 1.0, true);
    }

    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for (_, p) in &picked {
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    let (w, h) = (max[0] - min[0], max[1] - min[1]);
    // Anything much thinner than it is long counts as a line.
    let longest = w.max(h).max(1e-6);
    let flat = w.min(h) < longest * 0.02;
    let norm = |v: f32, lo: f32, span: f32| if span > longest * 0.02 { (v - lo) / span } else { 0.5 };
    let targets = picked
        .iter()
        .map(|(index, p)| {
            let (x, y) = (norm(p[0], min[0], w), norm(p[1], min[1], h));
            // A vertical line still runs along x as far as effects care.
            if flat && h > w {
                Target { index: *index, x: y, y: 0.5 }
            } else {
                Target { index: *index, x, y }
            }
        })
        .collect();
    let aspect = if flat { 1.0 } else { w / h };
    (targets, aspect, flat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Device;
    use crate::params::ParamValue;

    fn device(pixels: u32) -> Device {
        Device {
            id: 0,
            name: "d".into(),
            address: "127.0.0.1".into(),
            port: 4048,
            pixel_count: pixels,
            enabled: true,
            gamma: 1.0,
            brightness: 1.0,
            mac: None,
            version: None,
        }
    }

    fn solid_show(colour: &str) -> Show {
        let mut show = Show::default();
        show.layers.clear();
        let id = show.add_layer("solid");
        show.layer_mut(id)
            .unwrap()
            .params
            .insert("colour".into(), ParamValue::Text(colour.into()));
        show
    }

    fn render(show: &Show) -> (Renderer, Vec<Rgb>) {
        let mut r = Renderer::new();
        r.sync(show);
        let frame = r.render(show, 0.0, 1.0 / 60.0, &AudioFeatures::default()).to_vec();
        (r, frame)
    }

    #[test]
    fn solid_fills_every_pixel_and_reaches_the_device() {
        let mut show = solid_show("#ff8000");
        let dev = show.add_device(device(5), None);
        let (r, frame) = render(&show);
        assert_eq!(frame.len(), 5);
        assert!(frame.iter().all(|p| p.to_bytes() == [255, 128, 0]));
        let mut bytes = Vec::new();
        r.device_bytes(&show, dev, &mut bytes);
        assert_eq!(bytes, [255, 128, 0].repeat(5));
    }

    #[test]
    fn master_brightness_and_blackout_dim_the_frame() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(2), None);
        show.master.brightness = 0.5;
        let (_, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [128, 128, 128]);
        show.master.blackout = true;
        let (_, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [0, 0, 0]);
    }

    #[test]
    fn device_gamma_and_brightness_apply_only_to_device_bytes() {
        let mut show = solid_show("#808080");
        let dev = show.add_device(device(1), None);
        show.devices[0].gamma = 2.0;
        show.devices[0].brightness = 0.5;
        let (r, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [128, 128, 128]);
        let mut bytes = Vec::new();
        r.device_bytes(&show, dev, &mut bytes);
        // (128/255)^2 * 0.5 * 255 = 32
        assert_eq!(bytes, [32, 32, 32]);
    }

    #[test]
    fn layers_blend_in_order() {
        let mut show = solid_show("#400000");
        show.add_device(device(1), None);
        let top = show.add_layer("solid");
        show.layer_mut(top)
            .unwrap()
            .params
            .insert("colour".into(), ParamValue::Text("#004000".into()));
        let (_, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [64, 64, 0], "second layer adds by default");

        show.layer_mut(top).unwrap().blend = Blend::Normal;
        let (_, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [0, 64, 0]);

        show.layer_mut(top).unwrap().opacity = 0.5;
        let (_, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [32, 32, 0]);

        show.layer_mut(top).unwrap().enabled = false;
        let (_, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [64, 0, 0]);
    }

    #[test]
    fn a_layer_can_target_one_fixture() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(2), None);
        show.add_device(device(3), None);
        let second = show.fixtures[1].id;
        show.layers[0].targets = vec![second];
        let (_, frame) = render(&show);
        let lit: Vec<bool> = frame.iter().map(|p| p.r > 0.5).collect();
        assert_eq!(lit, [false, false, true, true, true]);
    }

    #[test]
    fn fixture_start_offsets_place_pixels_on_the_device() {
        let mut show = solid_show("#ffffff");
        let dev = show.add_device(device(6), None);
        show.fixtures[0].shape = Shape::Strip {
            count: 2,
            reverse: false,
        };
        show.fixtures[0].start = 3;
        let (r, _) = render(&show);
        let mut bytes = Vec::new();
        r.device_bytes(&show, dev, &mut bytes);
        let lit: Vec<bool> = bytes.chunks(3).map(|c| c[0] > 0).collect();
        assert_eq!(lit, [false, false, false, true, true, false]);
    }

    #[test]
    fn a_fixture_longer_than_its_device_is_clipped() {
        let mut show = solid_show("#ffffff");
        let dev = show.add_device(device(4), None);
        show.fixtures[0].shape = Shape::Strip {
            count: 10,
            reverse: false,
        };
        let (r, frame) = render(&show);
        assert_eq!(frame.len(), 10);
        let mut bytes = Vec::new();
        r.device_bytes(&show, dev, &mut bytes);
        assert_eq!(bytes.len(), 12);
    }

    #[test]
    fn an_effect_spans_fixtures_side_by_side() {
        // Two strips placed end to end share one left-to-right gradient.
        let mut show = Show::default();
        show.layers.clear();
        let id = show.add_layer("flow");
        let layer = show.layer_mut(id).unwrap();
        layer.params.insert("palette".into(), ParamValue::Text("mono".into()));
        layer.params.insert("speed".into(), ParamValue::Number(0.0));
        layer.params.insert("scale".into(), ParamValue::Number(1.0));
        show.add_device(device(10), None);
        show.add_device(device(10), None);
        for (i, f) in show.fixtures.iter_mut().enumerate() {
            f.x = i as f32;
            f.y = 0.0;
            f.width = 1.0;
        }
        let (_, frame) = render(&show);
        let v: Vec<f32> = frame.iter().map(|p| p.r).collect();
        // Mono bounces: dark in the middle of the pair, brighter at both ends,
        // and the two strips are continuous where they meet.
        assert!((v[9] - v[10]).abs() < 0.1, "seam should be continuous: {v:?}");
        assert!(v[0] > v[9] && v[19] > v[10], "{v:?}");
    }

    #[test]
    fn effect_state_survives_a_parameter_change() {
        let mut show = Show::default();
        show.layers.clear();
        let id = show.add_layer("scroll");
        show.add_device(device(20), None);
        let mut r = Renderer::new();
        r.sync(&show);
        let mut audio = AudioFeatures::default();
        audio.active = true;
        audio.bass = 1.0;
        for i in 0..30 {
            r.render(&show, i as f64 / 60.0, 1.0 / 60.0, &audio);
        }
        let before: f32 = r.frame.iter().map(|p| p.r).sum();
        assert!(before > 1.0);
        show.layer_mut(id)
            .unwrap()
            .params
            .insert("speed".into(), ParamValue::Number(0.1));
        r.sync(&show);
        r.render(&show, 0.5, 1.0 / 60.0, &audio);
        let after: f32 = r.frame.iter().map(|p| p.r).sum();
        assert!(after > before * 0.8, "history was lost: {before} -> {after}");
    }

    #[test]
    fn unknown_effects_are_skipped() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(1), None);
        show.layers[0].effect = "from-a-newer-version".into();
        let (_, frame) = render(&show);
        assert_eq!(frame[0].to_bytes(), [0, 0, 0]);
    }
}
