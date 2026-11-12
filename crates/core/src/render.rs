//! Turns a show plus audio features into pixel colours, frame by frame.

use std::collections::HashMap;

use crate::audio::AudioFeatures;
use crate::color::{OutputCurve, Rgb};
use crate::effect::{Effect, FlashUse, FrameCtx};
use crate::effects;
use crate::layout::{Fixture, Shape};
use crate::params::{ParamKind, ParamReader, ParamSpec, ParamValue, Params};
use crate::show::{Blend, Layer, Show};
use crate::show::{LINK_ANGLE, LINK_STRENGTH, Source};

/// A hit fired by hand for one layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerHit {
    pub layer: u32,
    /// 1 at the moment of the hit, fading to 0.
    pub pulse: f32,
    /// How many hits the layer has been sent so far.
    pub count: u32,
}

/// What a performer adds to a frame on top of the music.
#[derive(Clone, Debug, Default)]
pub struct Cues {
    pub hits: Vec<LayerHit>,
}

/// A pixel a layer draws on, with its position inside the layer's area.
struct Target {
    index: u32,
    x: f32,
    y: f32,
}

/// Pixels that share one frame of reference: all of a layer's pixels, or
/// one fixture's when the layer draws on each light by itself.
struct Segment {
    targets: Vec<Target>,
    aspect: f32,
    flat: bool,
    /// Degrees the pattern is turned on this segment before the layer's
    /// own angle: what keeps it upright on a matrix that has been turned.
    turn: f32,
}

struct LayerState {
    effect_id: String,
    effect: Box<dyn Effect>,
    segments: Vec<Segment>,
    /// How far the layer's spin has carried the pattern round, in degrees.
    spun: f32,
}

/// A pattern turned by some angle inside a box of a given shape.
///
/// The pattern keeps filling the box: its own width and height become the
/// box's extent along the turned axes, so every point of the box still
/// lands between 0 and 1 and no effect ever sees a place outside itself.
#[derive(Clone, Copy)]
struct Turn {
    sin: f32,
    cos: f32,
    /// Width over height of the box.
    aspect: f32,
    /// The extent of the box along the pattern's x and y, in box heights.
    wide: f32,
    high: f32,
}

impl Turn {
    fn new(degrees: f32, aspect: f32) -> Self {
        let aspect = aspect.max(1e-3);
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            sin,
            cos,
            aspect,
            wide: (aspect * cos.abs() + sin.abs()).max(1e-6),
            high: (aspect * sin.abs() + cos.abs()).max(1e-6),
        }
    }

    /// The shape of the pattern's own frame: what an effect should take
    /// the box's width over height to be.
    fn aspect(&self) -> f32 {
        self.wide / self.high
    }

    /// Where a point of the box falls in the pattern.
    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let (px, py) = ((x - 0.5) * self.aspect, y - 0.5);
        (
            ((px * self.cos + py * self.sin) / self.wide + 0.5).clamp(0.0, 1.0),
            ((py * self.cos - px * self.sin) / self.high + 0.5).clamp(0.0, 1.0),
        )
    }
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
    target_key: Vec<(u32, Vec<u32>, bool)>,
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

        let target_key: Vec<(u32, Vec<u32>, bool)> = show
            .layers
            .iter()
            // Which lights, each by itself or not, turning with them or not.
            .map(|l| (l.id * 2 + l.turns as u32, l.targets.clone(), l.each))
            .collect();
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
                        segments: Vec::new(),
                        spun: 0.0,
                    },
                );
            }
            if stale || targets_changed {
                let state = self.layers.get_mut(&layer.id).expect("just inserted");
                state.segments = build_segments(show, layer, &self.positions);
            }
        }
    }

    /// Renders one frame from the show and the music alone.
    pub fn render(&mut self, show: &Show, time: f64, dt: f32, audio: &AudioFeatures) -> &[Rgb] {
        self.render_with(show, time, dt, audio, &Cues::default())
    }

    /// Renders one frame. Colours are display-referred, before device
    /// gamma. `cues` carries what was fired by hand.
    pub fn render_with(&mut self, show: &Show, time: f64, dt: f32, audio: &AudioFeatures, cues: &Cues) -> &[Rgb] {
        self.frame.fill(Rgb::BLACK);
        let recolour: Option<String> = show
            .live
            .colours
            .and_then(|id| show.live.swatches.iter().find(|s| s.id == id))
            .map(|s| s.value.clone());
        for layer in &show.layers {
            // A folder can switch its layers off and scale their strength.
            let folder = layer.group.and_then(|g| show.layer_groups.iter().find(|x| x.id == g));
            let folder_strength = folder.map_or(1.0, |f| if f.enabled { f.opacity } else { 0.0 });
            // A link can raise a strength of nothing, so only a layer with
            // no links is skipped for being set to nothing.
            if !layer.enabled || folder_strength <= 0.0 || (layer.opacity <= 0.0 && layer.links.is_empty()) {
                continue;
            }
            let (Some(state), Some(def)) = (self.layers.get_mut(&layer.id), effects::find(&layer.effect)) else {
                continue;
            };
            // An effect that only makes sense flashing fast sits out while
            // the show's flash limit is on.
            if def.info.flash == FlashUse::Needed && show.flash.on() {
                continue;
            }
            // Each layer hears its own part of the sound, or none of it.
            let mut heard = if layer.sound.manual {
                audio.silenced()
            } else {
                audio.routed(layer.sound.band, layer.sound.gain)
            };
            // A hit fired by hand reaches this layer whatever it listens to.
            if let Some(hit) = cues.hits.iter().find(|h| h.layer == layer.id) {
                let pulse = hit.pulse.clamp(0.0, 1.0);
                heard.beat = heard.beat.max(pulse);
                heard.onset = heard.onset.max(pulse);
                for onset in &mut heard.onsets {
                    *onset = onset.max(pulse);
                }
                heard.beat_count = heard.beat_count.wrapping_add(hit.count);
            }
            // Following the shape of the song: in its quieter parts, effects
            // that listen are dimmer and slower. Effects played by hand, and
            // ones that ignore sound, are left alone.
            let lull = if def.info.audio && !layer.sound.manual {
                show.audio.dynamics.clamp(0.0, 1.0) * (1.0 - heard.intensity.clamp(0.0, 1.0))
            } else {
                0.0
            };
            // Settings that follow the music or a fader, moved for this frame.
            let (linked, own_strength, swing) = linked_settings(layer, &def.info.params, |source| {
                source.value(&heard, audio, &show.live.faders, time)
            });
            // The pattern's angle: where it is set, how far it has spun,
            // and how far a link is pushing it.
            state.spun = (state.spun + layer.spin * dt).rem_euclid(360.0);
            let angle = layer.angle + state.spun + swing;
            // A colour pad in force: every palette setting takes its colours.
            let linked = match &recolour {
                Some(value) if def.info.params.iter().any(|p| matches!(p.kind, ParamKind::Palette)) => {
                    let mut params = linked.unwrap_or_else(|| layer.params.clone());
                    for spec in &def.info.params {
                        if matches!(spec.kind, ParamKind::Palette) {
                            params.insert(spec.id.to_string(), ParamValue::Text(value.clone()));
                        }
                    }
                    Some(params)
                }
                _ => linked,
            };
            let opacity = (own_strength * folder_strength).clamp(0.0, 1.0) * (1.0 - 0.7 * lull);
            let dt = dt * (1.0 - 0.5 * lull);
            let Some(first) = state.segments.first() else {
                continue;
            };
            let mut ctx = FrameCtx {
                time,
                dt,
                audio: heard,
                beats: heard.beat_count as f64 + heard.beat_phase.clamp(0.0, 1.0) as f64,
                aspect: first.aspect,
                flat: first.flat,
                flash_cap: show.flash.cap(),
            };
            state.effect.configure(&ParamReader::new(
                &def.info.params,
                linked.as_ref().unwrap_or(&layer.params),
            ));
            // The effect moves on once a frame, whatever it is drawn on.
            state.effect.update(&ctx);
            // "Brightness follows sound": dim the layer by how quiet its
            // part of the sound is. Zero leaves the effect as it is.
            let pulse = layer.sound.pulse.clamp(0.0, 1.0);
            let follow = 1.0 - pulse + pulse * heard.level;
            for segment in &state.segments {
                // Each light, drawn by itself, has its own shape.
                ctx.aspect = segment.aspect;
                ctx.flat = segment.flat;
                // A line of lights has no second direction to turn in.
                let turned = angle + segment.turn;
                let turn =
                    (!segment.flat && turned.rem_euclid(360.0).abs() > 1e-3).then(|| Turn::new(turned, segment.aspect));
                if let Some(turn) = &turn {
                    ctx.aspect = turn.aspect();
                }
                for t in &segment.targets {
                    let (x, y) = match &turn {
                        Some(turn) => turn.apply(t.x, t.y),
                        None => (t.x, t.y),
                    };
                    let src = state.effect.shade(x, y, &ctx).clamp().scale(follow);
                    let dst = &mut self.frame[t.index as usize];
                    *dst = match layer.blend {
                        Blend::Normal => dst.lerp(src, opacity),
                        Blend::Add => dst.add(src.scale(opacity)).clamp(),
                        Blend::Lighten => dst.max(src.scale(opacity)),
                        Blend::Multiply => dst.lerp(dst.mul(src), opacity),
                    };
                }
            }
        }
        let master = if show.master.blackout {
            0.0
        } else {
            show.master.brightness.clamp(0.0, 1.0) * show.master.limit.clamp(0.0, 1.0)
        };
        if master < 1.0 {
            for px in &mut self.frame {
                *px = px.scale(master);
            }
        }
        // Groups of lights can be dimmed or switched off as one.
        if show.light_groups.iter().any(|g| !g.enabled || g.brightness < 1.0) {
            let mut offset = 0usize;
            for fixture in &show.fixtures {
                let count = fixture.pixel_count() as usize;
                let group = fixture.group.and_then(|g| show.light_groups.iter().find(|x| x.id == g));
                let level = group.map_or(1.0, |g| if g.enabled { g.brightness.clamp(0.0, 1.0) } else { 0.0 });
                if level < 1.0 {
                    for px in self.frame.iter_mut().skip(offset).take(count) {
                        *px = px.scale(level);
                    }
                }
                offset += count;
            }
        }
        &self.frame
    }

    /// The last rendered frame.
    pub fn frame(&self) -> &[Rgb] {
        &self.frame
    }

    /// The last rendered frame, for whatever is applied to the finished
    /// picture, such as the flash limit.
    pub fn frame_mut(&mut self) -> &mut [Rgb] {
        &mut self.frame
    }

    /// Mixes another frame into the last rendered one: `amount` 0 keeps
    /// this frame, 1 replaces it with `other`. Used to fade from one
    /// scene to the next. Does nothing if the sizes differ.
    pub fn mix_in(&mut self, other: &[Rgb], amount: f32) {
        if other.len() != self.frame.len() {
            return;
        }
        let amount = amount.clamp(0.0, 1.0);
        for (px, from) in self.frame.iter_mut().zip(other) {
            *px = px.lerp(*from, amount);
        }
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

/// The frames of reference a layer draws in: one around everything it
/// plays on, or one per light when it draws on each by itself.
/// A layer's settings with its links applied, and its strength likewise.
/// The settings are `None` when no link touches them, which is the usual
/// case and costs nothing.
/// The layer's settings as its links leave them for this frame: the
/// effect's settings if any moved, the strength, and how many degrees the
/// pattern is pushed round (half a turn at most, either way).
fn linked_settings(layer: &Layer, specs: &[ParamSpec], value_of: impl Fn(Source) -> f32) -> (Option<Params>, f32, f32) {
    let mut strength = layer.opacity;
    let mut swing = 0.0;
    let mut params: Option<Params> = None;
    for link in &layer.links {
        let push = value_of(link.source) * link.amount.clamp(-1.0, 1.0);
        if link.param == LINK_STRENGTH {
            strength = (strength + push).clamp(0.0, 1.0);
            continue;
        }
        if link.param == LINK_ANGLE {
            swing += push * 180.0;
            continue;
        }
        let Some(spec) = specs.iter().find(|s| s.id == link.param) else {
            continue;
        };
        let ParamKind::Float { min, max, .. } = spec.kind else {
            continue;
        };
        let set = ParamReader::new(specs, &layer.params).float(spec.id);
        let moved = (set + push * (max - min)).clamp(min, max);
        params
            .get_or_insert_with(|| layer.params.clone())
            .insert(link.param.clone(), ParamValue::Number(moved));
    }
    (params, strength, swing)
}

fn build_segments(show: &Show, layer: &Layer, positions: &[[f32; 2]]) -> Vec<Segment> {
    if !layer.each {
        let (targets, aspect, flat) = build_targets(show, layer, positions);
        return if targets.is_empty() {
            Vec::new()
        } else {
            vec![Segment {
                targets,
                aspect,
                flat,
                turn: 0.0,
            }]
        };
    }
    let mut segments = Vec::new();
    let mut offset = 0u32;
    for fixture in &show.fixtures {
        let count = fixture.pixel_count();
        if Show::layer_hits(layer, fixture) && count > 0 {
            segments.push(own_segment(fixture, offset, count, positions, layer.turns));
        }
        offset += count;
    }
    segments
}

/// A fixture in its own frame: along its length for a strip, across its
/// own columns and rows for a matrix, however it is turned on the canvas.
fn own_segment(fixture: &Fixture, offset: u32, count: u32, positions: &[[f32; 2]], turns: bool) -> Segment {
    match fixture.shape {
        Shape::Strip { reverse, .. } => Segment {
            targets: (0..count)
                .map(|i| {
                    let along = (i as f32 + 0.5) / count as f32;
                    Target {
                        index: offset + i,
                        x: if reverse { 1.0 - along } else { along },
                        y: 0.5,
                    }
                })
                .collect(),
            aspect: 1.0,
            flat: true,
            turn: 0.0,
        },
        Shape::Matrix { .. } => {
            // Turn each pixel's place on the canvas back into the matrix's own.
            let (sin, cos) = (-fixture.rotation).to_radians().sin_cos();
            let (w, h) = (fixture.width.max(1e-6), fixture.height.max(1e-6));
            Segment {
                targets: (0..count)
                    .map(|i| {
                        let p = positions[(offset + i) as usize];
                        let (dx, dy) = (p[0] - fixture.x, p[1] - fixture.y);
                        Target {
                            index: offset + i,
                            x: ((dx * cos - dy * sin) / w + 0.5).clamp(0.0, 1.0),
                            y: ((dx * sin + dy * cos) / h + 0.5).clamp(0.0, 1.0),
                        }
                    })
                    .collect(),
                aspect: w / h,
                flat: false,
                // Turned back by as much as the matrix is turned, the
                // pattern is the right way up in the room.
                turn: if turns { 0.0 } else { -fixture.rotation },
            }
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
        if Show::layer_hits(layer, fixture) {
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
                Target {
                    index: *index,
                    x: y,
                    y: 0.5,
                }
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
            gamma: 1.0,
            ..Device::wled("d", "127.0.0.1", 4048, pixels)
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

    /// A two-by-one picture, red on the left and blue on the right, on a
    /// four-by-four matrix.
    fn picture_show(rotation: f32) -> (Show, u32) {
        let mut show = Show::default();
        show.layers.clear();
        let layer = show.add_layer("picture");
        show.layer_mut(layer)
            .unwrap()
            .params
            .insert("picture".into(), ParamValue::Text("2x1:/wAAAAD/".into()));
        show.add_device(device(16), Some((4, 4)));
        show.fixtures[0].rotation = rotation;
        (show, layer)
    }

    /// The colour of the light nearest each side of the stage: left,
    /// right, top, bottom, as 'r', 'b' or '?' when the side is mixed.
    fn sides(show: &Show) -> String {
        let (r, frame) = render(show);
        let positions = r.positions().to_vec();
        let centre = [show.fixtures[0].x, show.fixtures[0].y];
        let reach = |axis: usize| {
            positions
                .iter()
                .map(|p| (p[axis] - centre[axis]).abs())
                .fold(0.0, f32::max)
        };
        let side = |axis: usize, sign: f32| {
            let colours: Vec<char> = positions
                .iter()
                .zip(&frame)
                .filter(|(p, _)| (p[axis] - centre[axis]) * sign > reach(axis) * 0.6)
                .map(|(_, c)| if c.r > 0.5 { 'r' } else { 'b' })
                .collect();
            assert!(!colours.is_empty());
            if colours.iter().all(|c| *c == colours[0]) {
                colours[0]
            } else {
                '?'
            }
        };
        [side(0, -1.0), side(0, 1.0), side(1, -1.0), side(1, 1.0)]
            .iter()
            .collect()
    }

    #[test]
    fn a_pattern_turns_by_the_layers_angle() {
        let (mut show, layer) = picture_show(0.0);
        assert_eq!(sides(&show), "rb??", "red on the left, blue on the right");
        // A quarter turn clockwise: what was on the left is now on top.
        show.layer_mut(layer).unwrap().angle = 90.0;
        assert_eq!(sides(&show), "??rb");
        show.layer_mut(layer).unwrap().angle = 180.0;
        assert_eq!(sides(&show), "br??");
        show.layer_mut(layer).unwrap().angle = 270.0;
        assert_eq!(sides(&show), "??br");
        // At 45 degrees the red half is the top-left corner: every light
        // is still lit, none falls outside the pattern.
        show.layer_mut(layer).unwrap().angle = 45.0;
        let (_, frame) = render(&show);
        assert!(frame.iter().all(|c| c.r > 0.5 || c.b > 0.5));
        assert!(frame.iter().any(|c| c.r > 0.5) && frame.iter().any(|c| c.b > 0.5));
    }

    #[test]
    fn a_turned_matrix_keeps_its_pattern_upright_unless_told_to_turn_with_it() {
        for rotation in [0.0, 90.0, 180.0, 270.0] {
            let (mut show, layer) = picture_show(rotation);
            show.layer_mut(layer).unwrap().each = true;
            assert_eq!(sides(&show), "rb??", "upright at {rotation}");
            // Drawn once across everything, it was always upright.
            show.layer_mut(layer).unwrap().each = false;
            assert_eq!(sides(&show), "rb??", "across at {rotation}");
        }
        // Turning with the light: a quarter turn of the matrix puts its
        // own left side at the top of the stage.
        let (mut show, layer) = picture_show(90.0);
        show.layer_mut(layer).unwrap().each = true;
        show.layer_mut(layer).unwrap().turns = true;
        assert_eq!(sides(&show), "??rb");
        // A diamond: upright, the left corner is red and the right blue.
        let (mut show, layer) = picture_show(45.0);
        show.layer_mut(layer).unwrap().each = true;
        let (r, frame) = render(&show);
        let centre = show.fixtures[0].x;
        for (p, c) in r.positions().iter().zip(&frame) {
            if (p[0] - centre).abs() > show.fixtures[0].width * 0.1 {
                assert_eq!(c.r > 0.5, p[0] < centre, "{p:?}");
            }
        }
    }

    #[test]
    fn a_spin_carries_the_pattern_round_and_a_link_swings_it() {
        let (mut show, layer) = picture_show(0.0);
        show.layer_mut(layer).unwrap().spin = 90.0;
        let mut r = Renderer::new();
        r.sync(&show);
        // One second at 90 degrees a second is a quarter turn.
        for _ in 0..60 {
            r.render(&show, 0.0, 1.0 / 60.0, &AudioFeatures::default());
        }
        let top: Vec<bool> = r
            .positions()
            .iter()
            .zip(r.frame())
            .filter(|(p, _)| p[1] < show.fixtures[0].y)
            .map(|(_, c)| c.r > 0.5)
            .collect();
        assert!(top.iter().all(|red| *red), "red has come round to the top");

        // A fader at full with a link of one half swings it a quarter turn.
        let (mut show, layer) = picture_show(0.0);
        show.layer_mut(layer).unwrap().links.push(crate::show::Link {
            param: LINK_ANGLE.into(),
            source: Source::Fader { n: 1 },
            amount: 0.5,
        });
        show.live.faders[0] = 1.0;
        assert_eq!(sides(&show), "??rb");
        show.live.faders[0] = 0.0;
        assert_eq!(sides(&show), "rb??");
        // A strip has one direction only, so an angle leaves it alone.
        let mut show = solid_show("#ffffff");
        show.layers[0].effect = "picture".into();
        show.layers[0]
            .params
            .insert("picture".into(), ParamValue::Text("2x1:/wAAAAD/".into()));
        show.add_device(device(4), None);
        let (_, before) = render(&show);
        show.layers[0].angle = 90.0;
        let (_, after) = render(&show);
        assert_eq!(before, after);
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
    fn a_layer_can_target_a_group_of_lights() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(2), None);
        show.add_device(device(2), None);
        show.add_device(device(2), None);
        let group = show.add_light_group("Ceiling");
        show.fixtures[0].group = Some(group);
        show.fixtures[2].group = Some(group);
        show.layers[0].targets = vec![group];
        let (_, frame) = render(&show);
        let lit: Vec<bool> = frame.iter().map(|px| *px == Rgb::WHITE).collect();
        assert_eq!(lit, [true, true, false, false, true, true]);

        // Moving a light out of the group takes it out of the effect.
        show.fixtures[2].group = None;
        let (_, frame) = render(&show);
        assert_eq!(frame[4], Rgb::BLACK);
    }

    #[test]
    fn a_group_of_lights_can_be_dimmed_or_switched_off() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(2), None);
        show.add_device(device(2), None);
        let group = show.add_light_group("Ceiling");
        show.fixtures[1].group = Some(group);
        show.light_groups[0].brightness = 0.5;
        let (_, frame) = render(&show);
        assert_eq!(frame[0], Rgb::WHITE, "lights outside the group are untouched");
        assert!((frame[2].r - 0.5).abs() < 1e-6);
        show.light_groups[0].enabled = false;
        let (_, frame) = render(&show);
        assert_eq!((frame[1], frame[3]), (Rgb::WHITE, Rgb::BLACK));
    }

    #[test]
    fn a_folder_switches_and_scales_the_effects_inside_it() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(1), None);
        let folder = show.add_layer_group("Verse");
        show.layers[0].group = Some(folder);
        let (_, frame) = render(&show);
        assert_eq!(frame[0], Rgb::WHITE);

        show.layer_groups[0].opacity = 0.25;
        let (_, frame) = render(&show);
        assert!((frame[0].r - 0.25).abs() < 1e-6);

        show.layer_groups[0].enabled = false;
        let (_, frame) = render(&show);
        assert_eq!(frame[0], Rgb::BLACK, "the layer's own switch is still on");
        assert!(show.layers[0].enabled);
    }

    #[test]
    fn the_brightness_limit_scales_what_full_brightness_means() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(1), None);
        show.master.limit = 0.4;
        let (_, frame) = render(&show);
        assert!((frame[0].r - 0.4).abs() < 1e-6);
        show.master.brightness = 0.5;
        let (_, frame) = render(&show);
        assert!((frame[0].r - 0.2).abs() < 1e-6);
    }

    #[test]
    fn frames_can_be_mixed_for_a_fade() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(2), None);
        let (mut r, _) = render(&show);
        r.mix_in(&[Rgb::BLACK, Rgb::BLACK], 0.75);
        assert!((r.frame()[0].r - 0.25).abs() < 1e-6);
        // A frame of another size is ignored instead of panicking.
        r.mix_in(&[Rgb::BLACK], 1.0);
        assert!((r.frame()[1].r - 0.25).abs() < 1e-6);
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
        let audio = AudioFeatures {
            active: true,
            bass: 1.0,
            ..AudioFeatures::default()
        };
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
    fn a_layer_reacts_only_to_the_part_of_the_sound_it_listens_to() {
        // The level meter grows with whatever it hears.
        let mut show = Show::default();
        show.layers.clear();
        let id = show.add_layer("meter");
        show.add_device(device(20), None);
        let lit = |show: &Show, audio: &AudioFeatures| {
            let mut r = Renderer::new();
            r.sync(show);
            r.render(show, 0.0, 1.0 / 60.0, audio);
            r.frame.iter().filter(|p| p.r + p.g + p.b > 0.2).count()
        };
        // Loud treble, silent bass.
        let audio = AudioFeatures {
            active: true,
            level: 0.5,
            treble: 1.0,
            bass: 0.0,
            ..AudioFeatures::default()
        };
        show.layer_mut(id).unwrap().sound.band = crate::Band::Treble;
        let highs = lit(&show, &audio);
        show.layer_mut(id).unwrap().sound.band = crate::Band::Bass;
        let bass = lit(&show, &audio);
        show.layer_mut(id).unwrap().sound.band = crate::Band::All;
        let all = lit(&show, &audio);
        assert!(highs >= 18, "listening to highs should fill the bar, lit {highs}");
        assert!(bass <= 1, "listening to bass should leave it dark, lit {bass}");
        assert!(
            all > bass && all < highs,
            "everything sits in between: {bass} < {all} < {highs}"
        );

        // Gain makes a quiet sound count for more.
        show.layer_mut(id).unwrap().sound.gain = 2.0;
        assert!(lit(&show, &audio) > all);
    }

    #[test]
    fn brightness_can_follow_the_sound_on_any_effect() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(1), None);
        let quiet = AudioFeatures {
            active: true,
            level: 0.25,
            ..AudioFeatures::default()
        };
        let brightness = |show: &Show| {
            let mut r = Renderer::new();
            r.sync(show);
            r.render(show, 0.0, 1.0 / 60.0, &quiet)[0].r
        };
        assert_eq!(brightness(&show), 1.0, "off by default: a solid colour stays solid");
        show.layers[0].sound.pulse = 1.0;
        assert!((brightness(&show) - 0.25).abs() < 1e-5);
        show.layers[0].sound.pulse = 0.5;
        assert!((brightness(&show) - 0.625).abs() < 1e-5);
    }

    #[test]
    fn a_hit_fired_by_hand_reaches_only_its_layer() {
        let mut show = Show::default();
        show.layers.clear();
        show.add_device(device(1), None);
        show.add_device(device(1), None);
        let (left, right) = (show.fixtures[0].id, show.fixtures[1].id);
        let a = show.add_layer("beat_pulse");
        let b = show.add_layer("beat_pulse");
        show.layer_mut(a).unwrap().targets = vec![left];
        show.layer_mut(b).unwrap().targets = vec![right];
        for id in [a, b] {
            let layer = show.layer_mut(id).unwrap();
            layer.params.insert("floor".into(), ParamValue::Number(0.0));
            layer.blend = Blend::Normal;
        }
        let mut r = Renderer::new();
        r.sync(&show);
        let silence = AudioFeatures::default();
        let frame = r.render(&show, 0.0, 1.0 / 60.0, &silence).to_vec();
        assert_eq!(frame, [Rgb::BLACK, Rgb::BLACK]);

        let cues = Cues {
            hits: vec![LayerHit {
                layer: b,
                pulse: 1.0,
                count: 1,
            }],
        };
        let frame = r.render_with(&show, 0.02, 1.0 / 60.0, &silence, &cues).to_vec();
        assert_eq!(frame[0], Rgb::BLACK, "the other layer heard nothing");
        assert!(frame[1].r + frame[1].g + frame[1].b > 0.5, "{:?}", frame[1]);
    }

    #[test]
    fn a_layer_set_to_ignore_the_music_reacts_only_when_fired() {
        let mut show = Show::default();
        show.layers.clear();
        show.add_device(device(1), None);
        let id = show.add_layer("beat_pulse");
        show.layer_mut(id)
            .unwrap()
            .params
            .insert("floor".into(), ParamValue::Number(0.0));
        show.layer_mut(id).unwrap().sound.manual = true;
        let mut r = Renderer::new();
        r.sync(&show);
        // Loud music with a beat on every frame: nothing.
        for i in 1..30 {
            let music = AudioFeatures {
                level: 1.0,
                beat: 1.0,
                onset: 1.0,
                onsets: [1.0; 3],
                beat_count: i,
                bpm: 120.0,
                active: true,
                ..AudioFeatures::default()
            };
            let frame = r.render(&show, i as f64 / 60.0, 1.0 / 60.0, &music).to_vec();
            assert_eq!(frame[0], Rgb::BLACK, "frame {i}");
        }
        let cues = Cues {
            hits: vec![LayerHit {
                layer: id,
                pulse: 1.0,
                count: 1,
            }],
        };
        let frame = r
            .render_with(&show, 1.0, 1.0 / 60.0, &AudioFeatures::default(), &cues)
            .to_vec();
        assert!(frame[0].r + frame[0].g + frame[0].b > 0.5);
    }

    #[test]
    fn quiet_parts_of_a_song_are_dimmer_only_for_effects_that_listen() {
        let quiet = AudioFeatures {
            active: true,
            intensity: 0.0,
            ..AudioFeatures::default()
        };
        let big = AudioFeatures {
            active: true,
            ..AudioFeatures::default()
        };
        let lit = |show: &Show, audio: &AudioFeatures| {
            let mut r = Renderer::new();
            r.sync(show);
            r.render(show, 0.0, 1.0 / 60.0, audio)[0].r
        };
        // Spectrum listens; with every band full it is white on a strip.
        let mut show = Show::default();
        show.layers.clear();
        show.add_device(device(1), None);
        show.add_layer("energy");
        let loud = |intensity| AudioFeatures {
            bass: 1.0,
            intensity,
            ..big
        };
        show.audio.dynamics = 1.0;
        let (full, low) = (lit(&show, &loud(1.0)), lit(&show, &loud(0.0)));
        assert!(full > 0.9, "{full}");
        assert!(
            (low / full - 0.3).abs() < 0.02,
            "at full strength a lull is 30%: {low} of {full}"
        );
        show.audio.dynamics = 0.0;
        assert_eq!(lit(&show, &loud(0.0)), full, "zero treats every part alike");

        // A solid colour does not listen, and an effect played by hand is not held back.
        let mut plain = solid_show("#ffffff");
        plain.add_device(device(1), None);
        plain.audio.dynamics = 1.0;
        assert_eq!(lit(&plain, &quiet), 1.0);
    }

    #[test]
    fn an_effect_can_draw_on_each_light_by_itself() {
        // A level meter fills from the left. Across two strips side by
        // side, half full lights the first strip only; on each by itself,
        // it lights the first half of both.
        let mut show = Show::default();
        show.layers.clear();
        show.add_device(device(4), None);
        show.add_device(device(4), None);
        show.fixtures[0].x = 0.0;
        show.fixtures[1].x = 1.0;
        show.fixtures[1].y = 0.0;
        let id = show.add_layer("meter");
        show.layer_mut(id)
            .unwrap()
            .params
            .insert("hold".into(), ParamValue::Bool(false));
        let half = AudioFeatures {
            level: 0.5,
            active: true,
            ..AudioFeatures::default()
        };
        let lit = |show: &Show| {
            let mut r = Renderer::new();
            r.sync(show);
            let mut frame = Vec::new();
            for i in 0..40 {
                frame = r.render(show, i as f64 / 60.0, 1.0 / 60.0, &half).to_vec();
            }
            frame.iter().map(|px| px.r + px.g + px.b > 0.3).collect::<Vec<bool>>()
        };
        assert_eq!(lit(&show), [true, true, true, true, false, false, false, false]);
        show.layer_mut(id).unwrap().each = true;
        assert_eq!(lit(&show), [true, true, false, false, true, true, false, false]);
    }

    #[test]
    fn a_matrix_told_to_turn_its_pattern_keeps_its_own_up_and_down() {
        // Asked to turn with the light, fire burns from the matrix's own
        // bottom row, even with the matrix upside down on the canvas.
        let mut show = Show::default();
        show.layers.clear();
        show.add_device(device(64), Some((8, 8)));
        show.fixtures[0].rotation = 180.0;
        let id = show.add_layer("fire");
        show.layer_mut(id).unwrap().each = true;
        show.layer_mut(id).unwrap().turns = true;
        let mut r = Renderer::new();
        r.sync(&show);
        let mut frame = Vec::new();
        for i in 0..60 {
            frame = r
                .render(&show, i as f64 / 60.0, 1.0 / 60.0, &AudioFeatures::default())
                .to_vec();
        }
        let row = |n: usize| frame[n * 8..n * 8 + 8].iter().map(|px| px.r).sum::<f32>();
        assert!(row(7) > 4.0, "the last row wired is the bottom: {}", row(7));
        assert!(row(0) < 0.5, "and the first is the top: {}", row(0));
        // Across the canvas instead, upside down means flames from the first row.
        show.layer_mut(id).unwrap().each = false;
        r.sync(&show);
        for i in 0..60 {
            frame = r
                .render(&show, i as f64 / 60.0, 1.0 / 60.0, &AudioFeatures::default())
                .to_vec();
        }
        let row = |n: usize| frame[n * 8..n * 8 + 8].iter().map(|px| px.r).sum::<f32>();
        assert!(row(0) > row(7));
    }

    #[test]
    fn an_effect_that_needs_fast_flashes_sits_out_while_the_flash_limit_is_on() {
        let mut show = Show::default();
        show.add_device(device(4), None);
        show.layers[0].effect = "lightning".into();
        let beat = AudioFeatures {
            beat_count: 1,
            bpm: 120.0,
            active: true,
            ..AudioFeatures::default()
        };
        let lit = |show: &Show| {
            let mut r = Renderer::new();
            r.sync(show);
            (0..10).any(|frame| {
                r.render(show, frame as f64 / 60.0, 1.0 / 60.0, &beat)
                    .iter()
                    .any(|px| px.r > 0.1)
            })
        };
        assert!(show.flash.on());
        assert!(!lit(&show), "left out while the limit is on");
        show.flash = crate::show::FlashSettings::off();
        assert!(lit(&show), "and there with it off");
    }

    #[test]
    fn a_strength_linked_to_the_music_moves_with_it() {
        use crate::show::Link;
        // A white layer at a fifth of its strength, which loudness raises.
        let mut show = solid_show("#ffffff");
        show.add_device(device(4), None);
        show.layers[0].opacity = 0.2;
        show.layers[0].links.push(Link {
            param: LINK_STRENGTH.into(),
            source: Source::Level,
            amount: 0.8,
        });
        let at = |show: &Show, level: f32| {
            let mut r = Renderer::new();
            r.sync(show);
            let audio = AudioFeatures {
                level,
                active: true,
                ..AudioFeatures::default()
            };
            r.render(show, 0.0, 1.0 / 60.0, &audio)[0].r
        };
        assert!((at(&show, 0.0) - 0.2).abs() < 0.01, "silence leaves it where it is set");
        assert!((at(&show, 0.5) - 0.6).abs() < 0.01);
        assert!((at(&show, 1.0) - 1.0).abs() < 0.01);
        // A negative amount goes the other way: dimmer as it gets louder.
        show.layers[0].opacity = 1.0;
        show.layers[0].links[0].amount = -1.0;
        assert!(at(&show, 0.0) > 0.99 && at(&show, 1.0) < 0.01);
        // A strength of nothing is still drawn when a link can raise it.
        show.layers[0].opacity = 0.0;
        show.layers[0].links[0].amount = 1.0;
        assert!(at(&show, 0.0) < 0.01 && at(&show, 1.0) > 0.99);
        // The show itself is not changed by any of this.
        assert_eq!(show.layers[0].opacity, 0.0);
    }

    #[test]
    fn a_linked_slider_stays_in_its_range_and_can_follow_a_fader() {
        use crate::show::Link;
        // Beat pulse's resting brightness runs from 0 to 0.5. Here it is
        // set to nothing and follows the second fader.
        let mut show = Show::default();
        show.add_device(device(4), None);
        show.layers[0].effect = "beat_pulse".into();
        show.layers[0].params.insert("floor".into(), ParamValue::Number(0.0));
        show.layers[0].links.push(Link {
            param: "floor".into(),
            source: Source::Fader { n: 2 },
            amount: 1.0,
        });
        let brightest = |show: &Show| {
            let mut r = Renderer::new();
            r.sync(show);
            r.render(show, 0.0, 1.0 / 60.0, &AudioFeatures::default())
                .iter()
                .fold(0.0f32, |m, px| m.max(px.r).max(px.g).max(px.b))
        };
        assert!(brightest(&show) < 0.01, "fader down: dark in silence");
        show.live.faders[1] = 1.0;
        let up = brightest(&show);
        assert!(up > 0.3 && up <= 0.51, "fader up: the top of the slider's range, {up}");
        show.live.faders[1] = 0.5;
        let half = brightest(&show);
        assert!(half > 0.15 && half < up, "{half}");
        // Another fader does nothing to it.
        show.live.faders = [1.0, 0.0, 1.0, 1.0];
        assert!(brightest(&show) < 0.01);
        // The stored setting is as it was.
        assert_eq!(show.layers[0].params["floor"], ParamValue::Number(0.0));
        // A link to a setting the effect does not have is ignored.
        show.layers[0].links[0].param = "no_such_setting".into();
        show.live.faders[1] = 1.0;
        assert!(brightest(&show) < 0.01);
    }

    #[test]
    fn a_colour_pad_in_force_recolours_every_palette() {
        use crate::show::Swatch;
        let mut show = Show::default();
        show.add_device(device(8), None);
        // The default effect takes its colours from a palette.
        show.live.swatches.push(Swatch {
            id: 900,
            name: "Red".into(),
            value: "#ff0000,#ff0000".into(),
        });
        let only_red = |show: &Show| {
            let (_, frame) = render(show);
            frame.iter().all(|px| px.g == 0.0 && px.b == 0.0) && frame.iter().any(|px| px.r > 0.0)
        };
        assert!(!only_red(&show), "its own colours to begin with");
        show.live.colours = Some(900);
        assert!(only_red(&show));
        // The stored setting is untouched, and a swatch that is gone does nothing.
        assert!(!show.layers[0].params.contains_key("palette"));
        show.live.colours = Some(901);
        assert!(!only_red(&show));
        // An effect with a colour of its own, not a palette, keeps it.
        let mut plain = solid_show("#00ff00");
        plain.add_device(device(2), None);
        plain.live.swatches = show.live.swatches.clone();
        plain.live.colours = Some(900);
        let (_, frame) = render(&plain);
        assert!(frame[0].g > 0.9 && frame[0].r < 0.1);
    }

    #[test]
    fn an_effect_can_be_kept_to_strips_or_to_matrices() {
        let mut show = solid_show("#ffffff");
        show.add_device(device(6), None);
        show.add_device(Device::wled("matrix", "127.0.0.2", 4048, 4), Some((2, 2)));
        let strip_pixels = show.fixtures[0].pixel_count() as usize;
        let lit = |show: &Show| {
            let (_, frame) = render(show);
            (frame[0].r > 0.5, frame[strip_pixels].r > 0.5)
        };
        assert_eq!(lit(&show), (true, true));
        show.layers[0].only = crate::show::Only::Strips;
        assert_eq!(lit(&show), (true, false));
        show.layers[0].only = crate::show::Only::Matrices;
        assert_eq!(lit(&show), (false, true));
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
