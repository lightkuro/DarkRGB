//! Every change a client can ask for, and the checks applied before it
//! touches the show. Nothing here does I/O, so all of it is unit tested.

use darkrgb_core::color;
use darkrgb_core::effects;
use darkrgb_core::layout::MAX_PIXELS;
use darkrgb_core::params::{ParamKind, ParamValue};
use darkrgb_core::show::{Blend, FRAME_RATES};
use darkrgb_core::{Action, Band, KeyMode, Launch, Rgb, Shape, Show};
use serde::Deserialize;

const MAX_NAME: usize = 80;
const MAX_LAYERS: usize = 32;
const MAX_SCENES: usize = 200;
const MAX_DEVICES: usize = 256;
const MAX_GROUPS: usize = 64;
const MAX_BINDINGS: usize = 128;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Looks the device up on the network, then adds it. Handled by the
    /// app because it needs the network; see `Shared::execute`.
    AddDevice {
        address: String,
    },
    /// Asks every address on a local network whether it is a WLED device.
    /// Handled by the app, like `AddDevice`.
    ScanNetwork {
        prefix: String,
    },
    /// Adds or removes the built-in demo lights.
    SetDemo {
        enabled: bool,
    },
    /// Adds a light that exists only in the preview.
    AddVirtual {
        name: String,
        shape: VirtualShape,
    },
    RemoveDevice {
        id: u32,
    },
    UpdateDevice {
        id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        gamma: Option<f32>,
        brightness: Option<f32>,
    },
    UpdateFixture {
        id: u32,
        name: Option<String>,
        x: Option<f32>,
        y: Option<f32>,
        width: Option<f32>,
        height: Option<f32>,
        rotation: Option<f32>,
        reverse: Option<bool>,
    },
    /// Adds an effect on top of the stack, or on top of a folder's effects.
    AddLayer {
        effect: String,
        #[serde(default)]
        group: Option<u32>,
    },
    RemoveLayer {
        id: u32,
    },
    UpdateLayer {
        id: u32,
        effect: Option<String>,
        opacity: Option<f32>,
        blend: Option<Blend>,
        enabled: Option<bool>,
        targets: Option<Vec<u32>>,
    },
    /// Gives a layer a label. An empty name goes back to the effect's own.
    RenameLayer {
        id: u32,
        name: String,
    },
    /// Copies a layer, settings and all, and puts the copy just above it.
    DuplicateLayer {
        id: u32,
    },
    /// Changes what a layer hears and how strongly it responds.
    SetSound {
        layer: u32,
        band: Option<Band>,
        gain: Option<f32>,
        pulse: Option<f32>,
    },
    /// Puts the stack in a new order and files each layer in a folder or
    /// in none. Lists every layer once, bottom first.
    ArrangeLayers {
        order: Vec<Placement>,
    },
    /// Adds a folder for effects, optionally with one layer already in it.
    AddLayerGroup {
        name: String,
        #[serde(default)]
        layer: Option<u32>,
    },
    UpdateLayerGroup {
        id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        opacity: Option<f32>,
    },
    /// Removes a folder. Its effects stay unless `keep_layers` is false.
    RemoveLayerGroup {
        id: u32,
        #[serde(default = "yes")]
        keep_layers: bool,
    },
    AddLightGroup {
        name: String,
    },
    UpdateLightGroup {
        id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        brightness: Option<f32>,
    },
    /// Removes a group of lights. The lights stay.
    RemoveLightGroup {
        id: u32,
    },
    /// Files fixtures in a group of lights, or in none.
    GroupLights {
        fixtures: Vec<u32>,
        group: Option<u32>,
    },
    /// Ties a key to something. The key, and the thing, lose any earlier tie.
    Bind {
        key: String,
        #[serde(default)]
        label: String,
        action: Action,
        #[serde(default)]
        mode: KeyMode,
    },
    UpdateBinding {
        id: u32,
        mode: KeyMode,
    },
    Unbind {
        id: u32,
    },
    /// A key went down or came up. Handled by the app, which keeps track
    /// of what is being held.
    Trigger {
        binding: u32,
        down: bool,
    },
    /// Starts again with an empty show, keeping the lights if asked. The
    /// show as it was is kept in a file beside it. Handled by the app.
    NewShow {
        #[serde(default = "yes")]
        keep_lights: bool,
    },
    /// Writes a copy of the show to the Documents folder. Handled by the app.
    ExportShow,
    /// One tap of the tempo. Handled by the app.
    Tap,
    /// Fires a beat by hand. Handled by the app.
    Hit,
    /// Goes back to following the music's own tempo. Handled by the app.
    AutoTempo,
    SetLive {
        launch: Option<Launch>,
        fade: Option<f32>,
    },
    SetOutput {
        frame_rate: Option<u32>,
        /// Most flashes a second; 0 switches the limit off.
        flash_limit: Option<f32>,
    },
    RenameScene {
        id: u32,
        name: String,
    },
    SetParam {
        layer: u32,
        param: String,
        value: ParamValue,
    },
    ResetParams {
        layer: u32,
    },
    SetMaster {
        brightness: Option<f32>,
        blackout: Option<bool>,
        limit: Option<f32>,
    },
    SetAudio {
        source: Option<String>,
        sensitivity: Option<f32>,
        delay_ms: Option<f32>,
    },
    SaveScene {
        name: String,
    },
    RecallScene {
        id: u32,
    },
    DeleteScene {
        id: u32,
    },
    RenameShow {
        name: String,
    },
}

/// Where one layer goes in the stack: see `Command::ArrangeLayers`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Placement {
    pub id: u32,
    #[serde(default)]
    pub group: Option<u32>,
}

fn yes() -> bool {
    true
}

/// Keys that can be tied to something: letters, digits, the number pad,
/// space and punctuation, named by where they sit on the keyboard. Keys
/// the browser or the app itself needs (Escape, Tab, Enter, arrows,
/// function keys) are left out.
pub fn valid_key(key: &str) -> bool {
    const NAMED: [&str; 12] = [
        "Space",
        "Minus",
        "Equal",
        "BracketLeft",
        "BracketRight",
        "Semicolon",
        "Quote",
        "Comma",
        "Period",
        "Slash",
        "Backslash",
        "Backquote",
    ];
    let one = |prefix: &str, ok: fn(&char) -> bool| {
        key.strip_prefix(prefix)
            .is_some_and(|rest| rest.len() == 1 && rest.chars().all(|c| ok(&c)))
    };
    NAMED.contains(&key)
        || one("Key", char::is_ascii_uppercase)
        || one("Digit", char::is_ascii_digit)
        || one("Numpad", char::is_ascii_digit)
}

/// The shape of a virtual light, as the UI's form sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VirtualShape {
    Strip { count: u32 },
    Matrix { cols: u32, rows: u32 },
}

fn finite(value: f32, what: &str) -> Result<f32, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{what} must be a number"))
    }
}

fn name(value: &str, what: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{what} cannot be empty"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(format!("{what} contains characters that cannot be used"));
    }
    Ok(trimmed.chars().take(MAX_NAME).collect())
}

/// Checks a parameter value against the effect's declaration and returns
/// the value to store.
fn checked_param(effect: &str, param: &str, value: &ParamValue) -> Result<ParamValue, String> {
    let def = effects::find(effect).ok_or_else(|| format!("unknown effect {effect:?}"))?;
    let spec = def
        .info
        .params
        .iter()
        .find(|p| p.id == param)
        .ok_or_else(|| format!("{} has no setting called {param:?}", def.info.name))?;
    match (&spec.kind, value) {
        (ParamKind::Float { min, max, .. }, ParamValue::Number(v)) => {
            Ok(ParamValue::Number(finite(*v, spec.label)?.clamp(*min, *max)))
        }
        (ParamKind::Bool, ParamValue::Bool(v)) => Ok(ParamValue::Bool(*v)),
        (ParamKind::Color, ParamValue::Text(v)) => Rgb::from_hex(v)
            .map(|c| ParamValue::Text(c.to_hex()))
            .ok_or_else(|| format!("{v:?} is not a colour like #ff8800")),
        (ParamKind::Choice { options }, ParamValue::Text(v)) if options.contains(&v.as_str()) => {
            Ok(ParamValue::Text(v.clone()))
        }
        (ParamKind::Palette, ParamValue::Text(v)) if color::palettes().iter().any(|p| p.id == v) => {
            Ok(ParamValue::Text(v.clone()))
        }
        _ => Err(format!("{value:?} is not a valid value for {}", spec.label)),
    }
}

/// Applies a command to the show. On error the show is unchanged.
pub fn apply(show: &mut Show, command: &Command) -> Result<(), String> {
    match command {
        Command::AddDevice { .. } | Command::ScanNetwork { .. } => Err("that command needs the network".into()),
        Command::Trigger { .. }
        | Command::Tap
        | Command::Hit
        | Command::AutoTempo
        | Command::NewShow { .. }
        | Command::ExportShow => Err("that command needs the running app".into()),

        Command::SetDemo { enabled } => {
            if *enabled && show.devices.len() + 3 > MAX_DEVICES {
                return Err(format!("a show can have at most {MAX_DEVICES} devices"));
            }
            show.set_demo(*enabled);
            Ok(())
        }

        Command::AddVirtual {
            name: light_name,
            shape,
        } => {
            let light_name = name(light_name, "The name")?;
            if show.devices.len() >= MAX_DEVICES {
                return Err(format!("a show can have at most {MAX_DEVICES} devices"));
            }
            let shape = match *shape {
                VirtualShape::Strip { count } if (1..=MAX_PIXELS).contains(&count) => {
                    Shape::Strip { count, reverse: false }
                }
                VirtualShape::Strip { .. } => return Err(format!("a strip needs between 1 and {MAX_PIXELS} LEDs")),
                VirtualShape::Matrix { cols, rows }
                    if cols >= 2 && rows >= 2 && cols.checked_mul(rows).is_some_and(|n| n <= MAX_PIXELS) =>
                {
                    Shape::Matrix {
                        cols,
                        rows,
                        serpentine: false,
                    }
                }
                VirtualShape::Matrix { .. } => {
                    return Err(format!(
                        "a matrix needs at least 2 by 2 LEDs and at most {MAX_PIXELS} in total"
                    ));
                }
            };
            show.add_virtual(&light_name, &shape, false);
            Ok(())
        }

        Command::RemoveDevice { id } => {
            if show.device(*id).is_none() {
                return Err("that device no longer exists".into());
            }
            show.remove_device(*id);
            Ok(())
        }

        Command::UpdateDevice {
            id,
            name: new_name,
            enabled,
            gamma,
            brightness,
        } => {
            // Validate everything before changing anything.
            let new_name = new_name.as_deref().map(|n| name(n, "The name")).transpose()?;
            let gamma = gamma.map(|g| finite(g, "Gamma")).transpose()?;
            let brightness = brightness.map(|b| finite(b, "Brightness")).transpose()?;
            let device = show
                .devices
                .iter_mut()
                .find(|d| d.id == *id)
                .ok_or("that device no longer exists")?;
            let mut renamed = None;
            if let Some(n) = new_name {
                renamed = Some((std::mem::replace(&mut device.name, n.clone()), n));
            }
            if let Some(e) = enabled {
                device.enabled = *e;
            }
            if let Some(g) = gamma {
                device.gamma = g.clamp(1.0, 4.0);
            }
            if let Some(b) = brightness {
                device.brightness = b.clamp(0.0, 1.0);
            }
            // A fixture that still carries its device's old name follows
            // the rename, so the lists that name fixtures stay in step.
            if let Some((old, new)) = renamed {
                for fixture in show.fixtures.iter_mut().filter(|f| f.device == *id && f.name == old) {
                    fixture.name = new.clone();
                }
            }
            Ok(())
        }

        Command::UpdateFixture {
            id,
            name: new_name,
            x,
            y,
            width,
            height,
            rotation,
            reverse,
        } => {
            let new_name = new_name.as_deref().map(|n| name(n, "The name")).transpose()?;
            let x = x.map(|v| finite(v, "Position")).transpose()?;
            let y = y.map(|v| finite(v, "Position")).transpose()?;
            let width = width.map(|v| finite(v, "Width")).transpose()?;
            let height = height.map(|v| finite(v, "Height")).transpose()?;
            let rotation = rotation.map(|v| finite(v, "Rotation")).transpose()?;
            let fixture = show.fixture_mut(*id).ok_or("that fixture no longer exists")?;
            if let Some(n) = new_name {
                fixture.name = n;
            }
            if let Some(v) = x {
                fixture.x = v.clamp(-1000.0, 1000.0);
            }
            if let Some(v) = y {
                fixture.y = v.clamp(-1000.0, 1000.0);
            }
            if let Some(v) = width {
                fixture.width = v.clamp(0.01, 100.0);
            }
            if let Some(v) = height {
                // A strip has no height; keep its box flat.
                fixture.height = match fixture.shape {
                    Shape::Strip { .. } => 0.0,
                    Shape::Matrix { .. } => v.clamp(0.01, 100.0),
                };
            }
            if let Some(v) = rotation {
                fixture.rotation = v.rem_euclid(360.0);
            }
            if let (Some(r), Shape::Strip { reverse, .. }) = (reverse, &mut fixture.shape) {
                *reverse = *r;
            }
            Ok(())
        }

        Command::AddLayer { effect, group } => {
            if effects::find(effect).is_none() {
                return Err(format!("unknown effect {effect:?}"));
            }
            if show.layers.len() >= MAX_LAYERS {
                return Err(format!("a show can have at most {MAX_LAYERS} layers"));
            }
            if let Some(group) = group
                && !show.layer_groups.iter().any(|g| g.id == *group)
            {
                return Err("that folder no longer exists".into());
            }
            show.add_layer_in(effect, *group);
            Ok(())
        }

        Command::RemoveLayer { id } => {
            let before = show.layers.len();
            show.layers.retain(|l| l.id != *id);
            if show.layers.len() == before {
                return Err("that layer no longer exists".into());
            }
            show.prune_bindings();
            Ok(())
        }

        Command::UpdateLayer {
            id,
            effect,
            opacity,
            blend,
            enabled,
            targets,
        } => {
            if let Some(effect) = effect
                && effects::find(effect).is_none()
            {
                return Err(format!("unknown effect {effect:?}"));
            }
            let opacity = opacity.map(|o| finite(o, "Opacity")).transpose()?;
            // A target is a light or a group of lights.
            if let Some(targets) = targets
                && targets
                    .iter()
                    .any(|t| !show.fixtures.iter().any(|f| f.id == *t) && !show.light_groups.iter().any(|g| g.id == *t))
            {
                return Err("one of those lights no longer exists".into());
            }
            let layer = show.layer_mut(*id).ok_or("that layer no longer exists")?;
            if let Some(effect) = effect
                && *effect != layer.effect
            {
                // Settings belong to the old effect.
                layer.effect = effect.clone();
                layer.params.clear();
            }
            if let Some(o) = opacity {
                layer.opacity = o.clamp(0.0, 1.0);
            }
            if let Some(b) = blend {
                layer.blend = *b;
            }
            if let Some(e) = enabled {
                layer.enabled = *e;
            }
            if let Some(t) = targets {
                let mut t = t.clone();
                t.sort_unstable();
                t.dedup();
                layer.targets = t;
            }
            Ok(())
        }

        Command::RenameLayer { id, name: label } => {
            // Empty means "use the effect's name again".
            let label = if label.trim().is_empty() {
                None
            } else {
                Some(name(label, "The name")?)
            };
            show.layer_mut(*id).ok_or("that layer no longer exists")?.name = label;
            Ok(())
        }

        Command::DuplicateLayer { id } => {
            let at = show
                .layers
                .iter()
                .position(|l| l.id == *id)
                .ok_or("that layer no longer exists")?;
            if show.layers.len() >= MAX_LAYERS {
                return Err(format!("a show can have at most {MAX_LAYERS} layers"));
            }
            let mut copy = show.layers[at].clone();
            copy.id = show.alloc_id();
            show.layers.insert(at + 1, copy);
            Ok(())
        }

        Command::SetSound {
            layer,
            band,
            gain,
            pulse,
        } => {
            let gain = gain.map(|g| finite(g, "Sensitivity")).transpose()?;
            let pulse = pulse.map(|p| finite(p, "Brightness follows sound")).transpose()?;
            let band = match band {
                Some(Band::Range { low, high }) => {
                    let (low, high) = (finite(*low, "The range")?, finite(*high, "The range")?);
                    let (low, high) = (low.clamp(0.0, 1.0), high.clamp(0.0, 1.0));
                    Some(Band::Range {
                        low: low.min(high),
                        high: low.max(high),
                    })
                }
                other => *other,
            };
            let sound = &mut show.layer_mut(*layer).ok_or("that layer no longer exists")?.sound;
            if let Some(b) = band {
                sound.band = b;
            }
            if let Some(g) = gain {
                sound.gain = g.clamp(0.25, 4.0);
            }
            if let Some(p) = pulse {
                sound.pulse = p.clamp(0.0, 1.0);
            }
            Ok(())
        }

        Command::ArrangeLayers { order } => {
            // The list must name exactly the layers there are; otherwise it
            // was made from a stack that has since changed.
            let mut wanted: Vec<u32> = order.iter().map(|p| p.id).collect();
            let mut have: Vec<u32> = show.layers.iter().map(|l| l.id).collect();
            wanted.sort_unstable();
            have.sort_unstable();
            if wanted != have {
                return Err("the effects changed in the meantime; try again".into());
            }
            if order
                .iter()
                .filter_map(|p| p.group)
                .any(|g| !show.layer_groups.iter().any(|x| x.id == g))
            {
                return Err("that folder no longer exists".into());
            }
            let mut layers = std::mem::take(&mut show.layers);
            for place in order {
                let at = layers.iter().position(|l| l.id == place.id).expect("checked above");
                let mut layer = layers.remove(at);
                layer.group = place.group;
                show.layers.push(layer);
            }
            show.tidy_layers();
            Ok(())
        }

        Command::AddLayerGroup { name: folder, layer } => {
            let folder = name(folder, "The name")?;
            if show.layer_groups.len() >= MAX_GROUPS {
                return Err(format!("a show can have at most {MAX_GROUPS} folders"));
            }
            if let Some(layer) = layer
                && !show.layers.iter().any(|l| l.id == *layer)
            {
                return Err("that layer no longer exists".into());
            }
            let id = show.add_layer_group(&folder);
            if let Some(layer) = layer {
                show.layer_mut(*layer).expect("checked above").group = Some(id);
                show.tidy_layers();
            }
            Ok(())
        }

        Command::UpdateLayerGroup {
            id,
            name: new_name,
            enabled,
            opacity,
        } => {
            let new_name = new_name.as_deref().map(|n| name(n, "The name")).transpose()?;
            let opacity = opacity.map(|o| finite(o, "Strength")).transpose()?;
            let folder = show
                .layer_groups
                .iter_mut()
                .find(|g| g.id == *id)
                .ok_or("that folder no longer exists")?;
            if let Some(n) = new_name {
                folder.name = n;
            }
            if let Some(e) = enabled {
                folder.enabled = *e;
            }
            if let Some(o) = opacity {
                folder.opacity = o.clamp(0.0, 1.0);
            }
            Ok(())
        }

        Command::RemoveLayerGroup { id, keep_layers } => {
            if !show.layer_groups.iter().any(|g| g.id == *id) {
                return Err("that folder no longer exists".into());
            }
            show.remove_layer_group(*id, *keep_layers);
            Ok(())
        }

        Command::AddLightGroup { name: group } => {
            let group = name(group, "The name")?;
            if show.light_groups.len() >= MAX_GROUPS {
                return Err(format!("a show can have at most {MAX_GROUPS} groups of lights"));
            }
            show.add_light_group(&group);
            Ok(())
        }

        Command::UpdateLightGroup {
            id,
            name: new_name,
            enabled,
            brightness,
        } => {
            let new_name = new_name.as_deref().map(|n| name(n, "The name")).transpose()?;
            let brightness = brightness.map(|b| finite(b, "Brightness")).transpose()?;
            let group = show
                .light_groups
                .iter_mut()
                .find(|g| g.id == *id)
                .ok_or("that group no longer exists")?;
            if let Some(n) = new_name {
                group.name = n;
            }
            if let Some(e) = enabled {
                group.enabled = *e;
            }
            if let Some(b) = brightness {
                group.brightness = b.clamp(0.0, 1.0);
            }
            Ok(())
        }

        Command::RemoveLightGroup { id } => {
            if !show.light_groups.iter().any(|g| g.id == *id) {
                return Err("that group no longer exists".into());
            }
            show.remove_light_group(*id);
            Ok(())
        }

        Command::GroupLights { fixtures, group } => {
            if let Some(group) = group
                && !show.light_groups.iter().any(|g| g.id == *group)
            {
                return Err("that group no longer exists".into());
            }
            if fixtures.iter().any(|id| !show.fixtures.iter().any(|f| f.id == *id)) {
                return Err("one of those lights no longer exists".into());
            }
            for fixture in show.fixtures.iter_mut().filter(|f| fixtures.contains(&f.id)) {
                fixture.group = *group;
            }
            Ok(())
        }

        Command::Bind {
            key,
            label,
            action,
            mode,
        } => {
            if !valid_key(key) {
                return Err("that key cannot be used; pick a letter, a number, space or punctuation".into());
            }
            let exists = match *action {
                Action::Scene { id } => show.scenes.iter().any(|s| s.id == id),
                Action::Layer { id } => show.layers.iter().any(|l| l.id == id),
                Action::LayerGroup { id } => show.layer_groups.iter().any(|g| g.id == id),
                Action::LightGroup { id } => show.light_groups.iter().any(|g| g.id == id),
                Action::Blackout | Action::TapTempo | Action::Hit => true,
            };
            if !exists {
                return Err("the thing that key was for no longer exists".into());
            }
            // Replacing an existing key does not add to the count.
            let replaces = show.bindings.iter().any(|b| b.key == *key || b.action == *action);
            if !replaces && show.bindings.len() >= MAX_BINDINGS {
                return Err(format!("a show can have at most {MAX_BINDINGS} keys"));
            }
            // What is printed on the key: a character or two, never markup.
            let label: String = label.trim().chars().filter(|c| !c.is_control()).take(8).collect();
            // Only things with a state to flip can be held.
            let mode = if action.is_switch() { *mode } else { KeyMode::Toggle };
            show.bind(key, &label, *action, mode);
            Ok(())
        }

        Command::UpdateBinding { id, mode } => {
            let binding = show
                .bindings
                .iter_mut()
                .find(|b| b.id == *id)
                .ok_or("that key is no longer set up")?;
            if binding.action.is_switch() {
                binding.mode = *mode;
            }
            Ok(())
        }

        Command::Unbind { id } => {
            let before = show.bindings.len();
            show.bindings.retain(|b| b.id != *id);
            if show.bindings.len() == before {
                return Err("that key is no longer set up".into());
            }
            Ok(())
        }

        Command::SetLive { launch, fade } => {
            let fade = fade.map(|f| finite(f, "The fade time")).transpose()?;
            if let Some(l) = launch {
                show.live.launch = *l;
            }
            if let Some(f) = fade {
                show.live.fade = f.clamp(0.0, 10.0);
            }
            Ok(())
        }

        Command::SetOutput {
            frame_rate,
            flash_limit,
        } => {
            let flash_limit = flash_limit.map(|l| finite(l, "The flash limit")).transpose()?;
            if let Some(rate) = frame_rate {
                if !FRAME_RATES.contains(rate) {
                    return Err(format!("the frame rate can be one of {FRAME_RATES:?}"));
                }
                show.output.frame_rate = *rate;
            }
            if let Some(limit) = flash_limit {
                // Zero is off. Anything else is a whole number of flashes.
                show.output.flash_limit = if limit < 1.0 { 0.0 } else { limit.round().min(30.0) };
            }
            Ok(())
        }

        Command::RenameScene { id, name: scene_name } => {
            let scene_name = name(scene_name, "The scene name")?;
            if show.scenes.iter().any(|s| s.name == scene_name && s.id != *id) {
                return Err(format!("there is already a scene called {scene_name}"));
            }
            show.scenes
                .iter_mut()
                .find(|s| s.id == *id)
                .ok_or("that scene no longer exists")?
                .name = scene_name;
            Ok(())
        }

        Command::SetParam { layer, param, value } => {
            let effect = show
                .layers
                .iter()
                .find(|l| l.id == *layer)
                .ok_or("that layer no longer exists")?
                .effect
                .clone();
            let value = checked_param(&effect, param, value)?;
            show.layer_mut(*layer)
                .expect("checked above")
                .params
                .insert(param.clone(), value);
            Ok(())
        }

        Command::ResetParams { layer } => {
            show.layer_mut(*layer)
                .ok_or("that layer no longer exists")?
                .params
                .clear();
            Ok(())
        }

        Command::SetMaster {
            brightness,
            blackout,
            limit,
        } => {
            let brightness = brightness.map(|b| finite(b, "Brightness")).transpose()?;
            let limit = limit.map(|l| finite(l, "The brightness limit")).transpose()?;
            if let Some(b) = brightness {
                show.master.brightness = b.clamp(0.0, 1.0);
            }
            if let Some(b) = blackout {
                show.master.blackout = *b;
            }
            if let Some(l) = limit {
                // Never zero, or the lights could not be turned back up.
                show.master.limit = l.clamp(0.05, 1.0);
            }
            Ok(())
        }

        Command::SetAudio {
            source,
            sensitivity,
            delay_ms,
        } => {
            let sensitivity = sensitivity.map(|s| finite(s, "Sensitivity")).transpose()?;
            let delay_ms = delay_ms.map(|d| finite(d, "The delay")).transpose()?;
            if let Some(d) = delay_ms {
                show.audio.delay_ms = d.clamp(0.0, 1000.0);
            }
            if let Some(source) = source {
                if source.is_empty() || source.len() > 512 || source.chars().any(char::is_control) {
                    return Err("that audio source is not valid".into());
                }
                show.audio.source = source.clone();
            }
            if let Some(s) = sensitivity {
                show.audio.sensitivity = s.clamp(0.25, 4.0);
            }
            Ok(())
        }

        Command::SaveScene { name: scene_name } => {
            let scene_name = name(scene_name, "The scene name")?;
            if show.scenes.len() >= MAX_SCENES {
                return Err(format!("a show can have at most {MAX_SCENES} scenes"));
            }
            // Saving under an existing name replaces that scene.
            if let Some(existing) = show.scenes.iter_mut().find(|s| s.name == scene_name) {
                existing.layers = show.layers.clone();
                existing.groups = show.layer_groups.clone();
                show.prune_bindings();
            } else {
                show.save_scene(&scene_name);
            }
            Ok(())
        }

        Command::RecallScene { id } => {
            if show.recall_scene(*id) {
                Ok(())
            } else {
                Err("that scene no longer exists".into())
            }
        }

        Command::DeleteScene { id } => {
            if show.delete_scene(*id) {
                Ok(())
            } else {
                Err("that scene no longer exists".into())
            }
        }

        Command::RenameShow { name: show_name } => {
            show.name = name(show_name, "The show name")?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkrgb_core::Device;

    fn show_with_device() -> Show {
        let mut show = Show::default();
        show.add_device(Device::wled("Strip", "127.0.0.1", 4048, 30), None);
        show
    }

    fn parse(json: &str) -> Command {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn commands_parse_from_what_the_ui_sends() {
        assert_eq!(
            parse(r#"{"type":"add_device","address":"192.168.1.50"}"#),
            Command::AddDevice {
                address: "192.168.1.50".into()
            }
        );
        assert_eq!(
            parse(r##"{"type":"set_param","layer":3,"param":"colour","value":"#ff0000"}"##),
            Command::SetParam {
                layer: 3,
                param: "colour".into(),
                value: ParamValue::Text("#ff0000".into())
            }
        );
        // Extra fields such as a request id are ignored.
        assert_eq!(
            parse(r#"{"id":7,"type":"set_master","blackout":true}"#),
            Command::SetMaster {
                brightness: None,
                blackout: Some(true),
                limit: None
            }
        );
        assert!(serde_json::from_str::<Command>(r#"{"type":"format_disk"}"#).is_err());
    }

    #[test]
    fn params_are_checked_against_the_effect() {
        let mut show = show_with_device();
        let layer = show.add_layer("sparkle");
        let set = |show: &mut Show, param: &str, value: ParamValue| {
            apply(
                show,
                &Command::SetParam {
                    layer,
                    param: param.into(),
                    value,
                },
            )
        };
        assert!(set(&mut show, "colour", ParamValue::Text("#00FF00".into())).is_ok());
        assert_eq!(
            show.layer_mut(layer).unwrap().params["colour"],
            ParamValue::Text("#00ff00".into())
        );
        assert!(set(&mut show, "colour", ParamValue::Text("green".into())).is_err());
        assert!(set(&mut show, "colour", ParamValue::Number(1.0)).is_err());
        assert!(set(&mut show, "no_such", ParamValue::Number(1.0)).is_err());
        assert!(set(&mut show, "density", ParamValue::Number(f32::NAN)).is_err());
        assert!(set(&mut show, "density", ParamValue::Number(50.0)).is_ok());
        assert_eq!(
            show.layer_mut(layer).unwrap().params["density"],
            ParamValue::Number(1.0)
        );
    }

    #[test]
    fn choices_and_palettes_must_be_known() {
        let mut show = show_with_device();
        let layer = show.add_layer("scroll");
        let set = |show: &mut Show, param: &str, value: &str| {
            apply(
                show,
                &Command::SetParam {
                    layer,
                    param: param.into(),
                    value: ParamValue::Text(value.into()),
                },
            )
        };
        assert!(set(&mut show, "origin", "Left").is_ok());
        assert!(set(&mut show, "origin", "Sideways").is_err());
        let plasma = show.layers[0].id;
        let palette = |show: &mut Show, value: &str| {
            apply(
                show,
                &Command::SetParam {
                    layer: plasma,
                    param: "palette".into(),
                    value: ParamValue::Text(value.into()),
                },
            )
        };
        assert!(palette(&mut show, "fire").is_ok());
        assert!(palette(&mut show, "nope").is_err());
    }

    #[test]
    fn changing_a_layers_effect_clears_its_settings() {
        let mut show = show_with_device();
        let layer = show.add_layer("flow");
        apply(
            &mut show,
            &Command::SetParam {
                layer,
                param: "speed".into(),
                value: ParamValue::Number(0.5),
            },
        )
        .unwrap();
        let update = |effect: &str| Command::UpdateLayer {
            id: layer,
            effect: Some(effect.into()),
            opacity: None,
            blend: None,
            enabled: None,
            targets: None,
        };
        // Same effect: settings stay.
        apply(&mut show, &update("flow")).unwrap();
        assert_eq!(show.layer_mut(layer).unwrap().params.len(), 1);
        apply(&mut show, &update("spectrum")).unwrap();
        assert!(show.layer_mut(layer).unwrap().params.is_empty());
        assert!(apply(&mut show, &update("nope")).is_err());
    }

    #[test]
    fn a_failed_update_changes_nothing() {
        let mut show = show_with_device();
        let before = show.clone();
        let id = show.devices[0].id;
        let result = apply(
            &mut show,
            &Command::UpdateDevice {
                id,
                name: Some("New name".into()),
                enabled: Some(false),
                gamma: Some(f32::INFINITY),
                brightness: None,
            },
        );
        assert!(result.is_err());
        assert_eq!(show, before);
    }

    #[test]
    fn values_are_clamped_to_safe_ranges() {
        let mut show = show_with_device();
        let device = show.devices[0].id;
        let fixture = show.fixtures[0].id;
        apply(
            &mut show,
            &Command::UpdateDevice {
                id: device,
                name: Some("  Desk  ".into()),
                enabled: None,
                gamma: Some(99.0),
                brightness: Some(-3.0),
            },
        )
        .unwrap();
        assert_eq!(show.devices[0].name, "Desk");
        assert_eq!(show.devices[0].gamma, 4.0);
        assert_eq!(show.devices[0].brightness, 0.0);

        apply(
            &mut show,
            &Command::UpdateFixture {
                id: fixture,
                name: None,
                x: Some(1e9),
                y: None,
                width: Some(0.0),
                height: Some(5.0),
                rotation: Some(-90.0),
                reverse: Some(true),
            },
        )
        .unwrap();
        let f = &show.fixtures[0];
        assert_eq!(f.x, 1000.0);
        assert_eq!(f.width, 0.01);
        assert_eq!(f.height, 0.0, "a strip stays flat");
        assert_eq!(f.rotation, 270.0);
        assert!(matches!(f.shape, Shape::Strip { reverse: true, .. }));
    }

    #[test]
    fn renaming_a_device_renames_the_fixture_named_after_it() {
        let mut show = show_with_device();
        let id = show.devices[0].id;
        let rename = |name: &str| Command::UpdateDevice {
            id,
            name: Some(name.into()),
            enabled: None,
            gamma: None,
            brightness: None,
        };
        apply(&mut show, &rename("Desk")).unwrap();
        assert_eq!(show.fixtures[0].name, "Desk");
        // A fixture the user named separately keeps its own name.
        show.fixtures[0].name = "Left half".into();
        apply(&mut show, &rename("Desk 2")).unwrap();
        assert_eq!(show.devices[0].name, "Desk 2");
        assert_eq!(show.fixtures[0].name, "Left half");
    }

    #[test]
    fn new_commands_parse_from_what_the_ui_sends() {
        assert_eq!(
            parse(r#"{"type":"set_demo","enabled":true}"#),
            Command::SetDemo { enabled: true }
        );
        assert_eq!(
            parse(r#"{"type":"add_virtual","name":"Ceiling","shape":{"type":"matrix","cols":30,"rows":10}}"#),
            Command::AddVirtual {
                name: "Ceiling".into(),
                shape: VirtualShape::Matrix { cols: 30, rows: 10 }
            }
        );
        assert_eq!(
            parse(r#"{"type":"set_sound","layer":4,"band":{"type":"treble"}}"#),
            Command::SetSound {
                layer: 4,
                band: Some(Band::Treble),
                gain: None,
                pulse: None
            }
        );
        assert_eq!(
            parse(r#"{"type":"set_sound","layer":4,"band":{"type":"range","low":0.2,"high":0.6},"gain":2}"#),
            Command::SetSound {
                layer: 4,
                band: Some(Band::Range { low: 0.2, high: 0.6 }),
                gain: Some(2.0),
                pulse: None
            }
        );
        assert_eq!(
            parse(r#"{"type":"scan_network","prefix":"192.168.1."}"#),
            Command::ScanNetwork {
                prefix: "192.168.1.".into()
            }
        );
    }

    #[test]
    fn virtual_lights_are_checked_for_size() {
        let mut show = Show::default();
        let add = |show: &mut Show, shape| {
            apply(
                show,
                &Command::AddVirtual {
                    name: "Plan".into(),
                    shape,
                },
            )
        };
        assert!(add(&mut show, VirtualShape::Strip { count: 60 }).is_ok());
        assert!(add(&mut show, VirtualShape::Matrix { cols: 32, rows: 8 }).is_ok());
        assert!(add(&mut show, VirtualShape::Strip { count: 0 }).is_err());
        assert!(add(&mut show, VirtualShape::Strip { count: MAX_PIXELS + 1 }).is_err());
        assert!(add(&mut show, VirtualShape::Matrix { cols: 1, rows: 50 }).is_err());
        assert!(
            add(
                &mut show,
                VirtualShape::Matrix {
                    cols: 70_000,
                    rows: 70_000
                }
            )
            .is_err()
        );
        assert_eq!(show.devices.len(), 2);
        assert!(show.devices.iter().all(|d| !d.sends() && !d.demo));
        assert!(show.validate().is_ok());
    }

    #[test]
    fn demo_lights_switch_on_and_off() {
        let mut show = show_with_device();
        apply(&mut show, &Command::SetDemo { enabled: true }).unwrap();
        assert_eq!(show.devices.iter().filter(|d| d.demo).count(), 3);
        apply(&mut show, &Command::SetDemo { enabled: false }).unwrap();
        assert_eq!(show.devices.len(), 1);
    }

    #[test]
    fn sound_settings_are_clamped_and_ranges_put_in_order() {
        let mut show = show_with_device();
        let layer = show.layers[0].id;
        apply(
            &mut show,
            &Command::SetSound {
                layer,
                band: Some(Band::Range { low: 0.9, high: -3.0 }),
                gain: Some(100.0),
                pulse: Some(7.0),
            },
        )
        .unwrap();
        let sound = show.layers[0].sound;
        assert_eq!(sound.band, Band::Range { low: 0.0, high: 0.9 });
        assert_eq!((sound.gain, sound.pulse), (4.0, 1.0));
        let before = show.clone();
        let bad = Command::SetSound {
            layer,
            band: Some(Band::Range {
                low: f32::NAN,
                high: 1.0,
            }),
            gain: Some(1.0),
            pulse: None,
        };
        assert!(apply(&mut show.clone(), &bad).is_err());
        assert_eq!(show, before);
    }

    #[test]
    fn layers_can_be_named_and_duplicated() {
        let mut show = show_with_device();
        let first = show.layers[0].id;
        let top = show.add_layer("solid");
        apply(
            &mut show,
            &Command::RenameLayer {
                id: first,
                name: "  Wash  ".into(),
            },
        )
        .unwrap();
        assert_eq!(show.layers[0].name.as_deref(), Some("Wash"));
        apply(&mut show, &Command::DuplicateLayer { id: first }).unwrap();
        // The copy sits just above the original and has its own id.
        assert_eq!(show.layers.len(), 3);
        assert_eq!(show.layers[1].name.as_deref(), Some("Wash"));
        assert_ne!(show.layers[1].id, first);
        assert_eq!(show.layers[2].id, top);
        apply(
            &mut show,
            &Command::RenameLayer {
                id: first,
                name: " ".into(),
            },
        )
        .unwrap();
        assert_eq!(show.layers[0].name, None);
    }

    #[test]
    fn names_cannot_be_empty_or_huge() {
        let mut show = show_with_device();
        assert!(apply(&mut show, &Command::RenameShow { name: "   ".into() }).is_err());
        assert!(apply(&mut show, &Command::RenameShow { name: "a\nb".into() }).is_err());
        apply(&mut show, &Command::RenameShow { name: "x".repeat(500) }).unwrap();
        assert_eq!(show.name.len(), MAX_NAME);
    }

    #[test]
    fn layers_can_be_reordered_and_removed() {
        let mut show = show_with_device();
        let a = show.layers[0].id;
        let b = show.add_layer("solid");
        let c = show.add_layer("flow");
        let arrange = |show: &mut Show, order: &[(u32, Option<u32>)]| {
            let order = order.iter().map(|&(id, group)| Placement { id, group }).collect();
            apply(show, &Command::ArrangeLayers { order })
        };
        let ids = |show: &Show| show.layers.iter().map(|l| l.id).collect::<Vec<_>>();
        arrange(&mut show, &[(c, None), (a, None), (b, None)]).unwrap();
        assert_eq!(ids(&show), [c, a, b]);
        // A list that leaves a layer out, repeats one or invents one was
        // made from an older stack and is refused.
        assert!(arrange(&mut show, &[(c, None), (a, None)]).is_err());
        assert!(arrange(&mut show, &[(c, None), (a, None), (a, None)]).is_err());
        assert!(arrange(&mut show, &[(c, None), (a, None), (9999, None)]).is_err());
        assert!(arrange(&mut show, &[(c, Some(9999)), (a, None), (b, None)]).is_err());
        assert_eq!(ids(&show), [c, a, b]);

        apply(&mut show, &Command::RemoveLayer { id: b }).unwrap();
        assert!(apply(&mut show, &Command::RemoveLayer { id: b }).is_err());
        assert_eq!(show.layers.len(), 2);
    }

    #[test]
    fn effects_can_be_filed_in_folders() {
        let mut show = show_with_device();
        let a = show.layers[0].id;
        let b = show.add_layer("solid");
        let c = show.add_layer("flow");
        apply(
            &mut show,
            &Command::AddLayerGroup {
                name: " Chorus ".into(),
                layer: Some(c),
            },
        )
        .unwrap();
        let folder = show.layer_groups[0].id;
        assert_eq!(show.layer_groups[0].name, "Chorus");
        assert_eq!(show.layers.iter().find(|l| l.id == c).unwrap().group, Some(folder));

        // Filing a and c together pulls them next to each other.
        let order = vec![
            Placement {
                id: a,
                group: Some(folder),
            },
            Placement { id: b, group: None },
            Placement {
                id: c,
                group: Some(folder),
            },
        ];
        apply(&mut show, &Command::ArrangeLayers { order }).unwrap();
        assert_eq!(show.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, c, b]);

        // A new effect can go straight into the folder.
        apply(
            &mut show,
            &Command::AddLayer {
                effect: "sparkle".into(),
                group: Some(folder),
            },
        )
        .unwrap();
        assert_eq!(show.layers[2].effect, "sparkle");
        assert!(
            apply(
                &mut show,
                &Command::AddLayer {
                    effect: "sparkle".into(),
                    group: Some(9999)
                }
            )
            .is_err()
        );

        apply(
            &mut show,
            &Command::UpdateLayerGroup {
                id: folder,
                name: None,
                enabled: Some(false),
                opacity: Some(7.0),
            },
        )
        .unwrap();
        assert_eq!(
            (show.layer_groups[0].enabled, show.layer_groups[0].opacity),
            (false, 1.0)
        );

        // Removing the folder keeps its effects unless told otherwise.
        let mut kept = show.clone();
        apply(
            &mut kept,
            &parse(&format!(r#"{{"type":"remove_layer_group","id":{folder}}}"#)),
        )
        .unwrap();
        assert_eq!(kept.layers.len(), 4);
        apply(
            &mut show,
            &Command::RemoveLayerGroup {
                id: folder,
                keep_layers: false,
            },
        )
        .unwrap();
        assert_eq!(show.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [b]);
    }

    #[test]
    fn lights_can_be_grouped_and_effects_aimed_at_the_group() {
        let mut show = show_with_device();
        show.add_device(Device::wled("Second", "127.0.0.2", 4048, 30), None);
        let (first, second) = (show.fixtures[0].id, show.fixtures[1].id);
        apply(&mut show, &Command::AddLightGroup { name: "Ceiling".into() }).unwrap();
        let group = show.light_groups[0].id;
        let file = |show: &mut Show, fixtures: Vec<u32>, group| apply(show, &Command::GroupLights { fixtures, group });
        file(&mut show, vec![first, second], Some(group)).unwrap();
        assert!(show.fixtures.iter().all(|f| f.group == Some(group)));
        assert!(file(&mut show, vec![first], Some(9999)).is_err());
        assert!(file(&mut show, vec![9999], Some(group)).is_err());
        file(&mut show, vec![second], None).unwrap();
        assert_eq!(show.fixtures[1].group, None);

        let layer = show.layers[0].id;
        let aim = |show: &mut Show, targets: Vec<u32>| {
            apply(
                show,
                &Command::UpdateLayer {
                    id: layer,
                    effect: None,
                    opacity: None,
                    blend: None,
                    enabled: None,
                    targets: Some(targets),
                },
            )
        };
        aim(&mut show, vec![group, second]).unwrap();
        assert!(aim(&mut show, vec![layer]).is_err(), "a layer is not a light");

        apply(
            &mut show,
            &Command::UpdateLightGroup {
                id: group,
                name: Some("Roof".into()),
                enabled: None,
                brightness: Some(-3.0),
            },
        )
        .unwrap();
        assert_eq!(
            (show.light_groups[0].name.as_str(), show.light_groups[0].brightness),
            ("Roof", 0.0)
        );

        apply(&mut show, &Command::RemoveLightGroup { id: group }).unwrap();
        assert_eq!(show.layers[0].targets, [first, second]);
        assert!(apply(&mut show, &Command::RemoveLightGroup { id: group }).is_err());
    }

    #[test]
    fn keys_are_checked_and_follow_what_they_are_tied_to() {
        for key in ["KeyA", "KeyZ", "Digit0", "Numpad7", "Space", "Slash"] {
            assert!(valid_key(key), "{key}");
        }
        for key in [
            "",
            "Key",
            "Keya",
            "KeyAB",
            "Escape",
            "Tab",
            "Enter",
            "F5",
            "ArrowUp",
            "Digit12",
            "a",
            "Key\u{e9}",
        ] {
            assert!(!valid_key(key), "{key}");
        }

        let mut show = show_with_device();
        let layer = show.layers[0].id;
        let bind = |show: &mut Show, key: &str, action, mode| {
            apply(
                show,
                &Command::Bind {
                    key: key.into(),
                    label: "  \u{7}Q  and far too long ".into(),
                    action,
                    mode,
                },
            )
        };
        assert!(bind(&mut show, "Escape", Action::Blackout, KeyMode::Toggle).is_err());
        assert!(bind(&mut show, "KeyQ", Action::Layer { id: 9999 }, KeyMode::Toggle).is_err());
        assert!(bind(&mut show, "KeyQ", Action::Scene { id: layer }, KeyMode::Toggle).is_err());
        bind(&mut show, "KeyQ", Action::Layer { id: layer }, KeyMode::Hold).unwrap();
        assert_eq!(
            show.bindings[0].label, "Q  and f",
            "trimmed, no control characters, short"
        );
        assert_eq!(show.bindings[0].mode, KeyMode::Hold);

        // Things with nothing to flip cannot be held.
        bind(&mut show, "KeyT", Action::TapTempo, KeyMode::Hold).unwrap();
        let tap = show.binding_for(Action::TapTempo).unwrap().id;
        assert_eq!(show.binding_for(Action::TapTempo).unwrap().mode, KeyMode::Toggle);
        apply(
            &mut show,
            &Command::UpdateBinding {
                id: tap,
                mode: KeyMode::Hold,
            },
        )
        .unwrap();
        assert_eq!(show.binding_for(Action::TapTempo).unwrap().mode, KeyMode::Toggle);

        // Removing the effect removes its key; unbinding removes the other.
        apply(&mut show, &Command::RemoveLayer { id: layer }).unwrap();
        assert_eq!(show.bindings.len(), 1);
        apply(&mut show, &Command::Unbind { id: tap }).unwrap();
        assert!(apply(&mut show, &Command::Unbind { id: tap }).is_err());
        assert!(show.bindings.is_empty());
    }

    #[test]
    fn settings_are_kept_within_what_makes_sense() {
        let mut show = show_with_device();
        apply(
            &mut show,
            &Command::SetMaster {
                brightness: None,
                blackout: None,
                limit: Some(0.0),
            },
        )
        .unwrap();
        assert_eq!(show.master.limit, 0.05, "never so low the lights cannot come back");
        apply(
            &mut show,
            &Command::SetAudio {
                source: None,
                sensitivity: None,
                delay_ms: Some(5000.0),
            },
        )
        .unwrap();
        assert_eq!(show.audio.delay_ms, 1000.0);
        apply(
            &mut show,
            &Command::SetLive {
                launch: Some(Launch::Bar),
                fade: Some(-1.0),
            },
        )
        .unwrap();
        assert_eq!((show.live.launch, show.live.fade), (Launch::Bar, 0.0));
        let output = |show: &mut Show, frame_rate, flash_limit| {
            apply(
                show,
                &Command::SetOutput {
                    frame_rate,
                    flash_limit,
                },
            )
        };
        assert!(output(&mut show, Some(1000), None).is_err());
        assert!(output(&mut show, Some(0), None).is_err());
        output(&mut show, Some(40), None).unwrap();
        output(&mut show, Some(120), None).unwrap();
        assert_eq!(show.output.frame_rate, 120);
        // The flash limit: three by default, zero is off, never a fraction or absurd.
        assert_eq!(show.output.flash_limit, 3.0);
        output(&mut show, None, Some(0.4)).unwrap();
        assert_eq!(show.output.flash_limit, 0.0);
        output(&mut show, None, Some(4.4)).unwrap();
        assert_eq!(show.output.flash_limit, 4.0);
        output(&mut show, None, Some(1e9)).unwrap();
        assert_eq!(show.output.flash_limit, 30.0);
        assert!(output(&mut show, None, Some(f32::NAN)).is_err());
        assert_eq!((show.output.frame_rate, show.output.flash_limit), (120, 30.0));
        // Commands that need the running app are refused here.
        assert!(apply(&mut show, &Command::Tap).is_err());
        assert!(apply(&mut show, &Command::Trigger { binding: 1, down: true }).is_err());
    }

    #[test]
    fn scenes_can_be_renamed_but_not_to_a_name_in_use() {
        let mut show = show_with_device();
        apply(&mut show, &Command::SaveScene { name: "Intro".into() }).unwrap();
        apply(&mut show, &Command::SaveScene { name: "Drop".into() }).unwrap();
        let (intro, drop) = (show.scenes[0].id, show.scenes[1].id);
        let rename = |show: &mut Show, id, name: &str| apply(show, &Command::RenameScene { id, name: name.into() });
        assert!(rename(&mut show, drop, "Intro").is_err());
        rename(&mut show, drop, "Drop").unwrap();
        rename(&mut show, intro, "Opening").unwrap();
        assert_eq!(show.scenes[0].name, "Opening");
        assert!(rename(&mut show, 9999, "x").is_err());
    }

    #[test]
    fn layer_targets_must_exist_and_are_deduplicated() {
        let mut show = show_with_device();
        let layer = show.layers[0].id;
        let fixture = show.fixtures[0].id;
        let target = |targets: Vec<u32>| Command::UpdateLayer {
            id: layer,
            effect: None,
            opacity: None,
            blend: None,
            enabled: None,
            targets: Some(targets),
        };
        assert!(apply(&mut show, &target(vec![9999])).is_err());
        apply(&mut show, &target(vec![fixture, fixture])).unwrap();
        assert_eq!(show.layers[0].targets, [fixture]);
    }

    #[test]
    fn saving_a_scene_under_the_same_name_replaces_it() {
        let mut show = show_with_device();
        apply(&mut show, &Command::SaveScene { name: "Chill".into() }).unwrap();
        show.add_layer("solid");
        apply(&mut show, &Command::SaveScene { name: " Chill ".into() }).unwrap();
        assert_eq!(show.scenes.len(), 1);
        assert_eq!(show.scenes[0].layers.len(), 2);
        let id = show.scenes[0].id;
        apply(&mut show, &Command::DeleteScene { id }).unwrap();
        assert!(apply(&mut show, &Command::RecallScene { id }).is_err());
    }

    /// A small repeatable source of awkward values for the test below.
    struct Awkward(u32);

    impl Awkward {
        const NUMBERS: [f32; 10] = [
            0.0,
            1.0,
            -1.0,
            0.5,
            1e30,
            -1e30,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN_POSITIVE,
        ];
        const TEXTS: [&'static str; 13] = [
            "", " ", "x", "#ff0000", "#GGGGGG", "plasma", "solid", "scroll", "nope", "Left", "fire", "system", "a\nb",
        ];

        fn next(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.0 >> 8
        }
        fn flag(&mut self) -> bool {
            self.next().is_multiple_of(2)
        }
        fn number(&mut self) -> f32 {
            Self::NUMBERS[self.next() as usize % Self::NUMBERS.len()]
        }
        fn text(&mut self) -> String {
            Self::TEXTS[self.next() as usize % Self::TEXTS.len()].to_string()
        }
        fn maybe<T>(&mut self, make: impl FnOnce(&mut Self) -> T) -> Option<T> {
            if self.flag() { Some(make(self)) } else { None }
        }
        /// Mostly an id that exists, sometimes one that does not.
        fn id(&mut self, ids: Vec<u32>) -> u32 {
            let n = self.next();
            if n.is_multiple_of(5) || ids.is_empty() {
                n % 50
            } else {
                ids[n as usize % ids.len()]
            }
        }
    }

    /// Whatever commands arrive, in whatever order and with whatever
    /// values, the show stays one that could be saved and loaded again.
    #[test]
    fn no_sequence_of_commands_can_make_the_show_invalid() {
        let mut r = Awkward(0x2545_F491);
        let blends = [Blend::Normal, Blend::Add, Blend::Lighten, Blend::Multiply];
        let params = ["speed", "colour", "palette", "origin", "mirror", "nope"];

        let mut show = show_with_device();
        show.add_layer("scroll");
        let mut accepted = 0;
        let (mut seen_folders, mut seen_groups, mut seen_keys) = (false, false, false);
        for _ in 0..30_000 {
            let layer = r.id(show.layers.iter().map(|l| l.id).collect());
            let device = r.id(show.devices.iter().map(|d| d.id).collect());
            let fixture = r.id(show.fixtures.iter().map(|f| f.id).collect());
            let scene = r.id(show.scenes.iter().map(|s| s.id).collect());
            let folder = r.id(show.layer_groups.iter().map(|g| g.id).collect());
            let group = r.id(show.light_groups.iter().map(|g| g.id).collect());
            let binding = r.id(show.bindings.iter().map(|b| b.id).collect());
            let command = match r.next() % 36 {
                20 => Command::ArrangeLayers {
                    // Usually every layer once, shuffled and refiled; sometimes not.
                    order: {
                        let mut order: Vec<Placement> = show
                            .layers
                            .iter()
                            .map(|l| Placement {
                                id: l.id,
                                group: r.flag().then_some(folder),
                            })
                            .collect();
                        if !order.is_empty() {
                            let (a, b) = (r.next() as usize % order.len(), r.next() as usize % order.len());
                            order.swap(a, b);
                        }
                        if r.next().is_multiple_of(7) {
                            order.pop();
                        }
                        order
                    },
                },
                21 => Command::AddLayerGroup {
                    name: r.text(),
                    layer: r.maybe(|_| layer),
                },
                22 => Command::UpdateLayerGroup {
                    id: folder,
                    name: r.maybe(Awkward::text),
                    enabled: r.maybe(Awkward::flag),
                    opacity: r.maybe(Awkward::number),
                },
                23 => Command::RemoveLayerGroup {
                    id: folder,
                    keep_layers: r.flag(),
                },
                24 => Command::AddLightGroup { name: r.text() },
                25 => Command::UpdateLightGroup {
                    id: group,
                    name: r.maybe(Awkward::text),
                    enabled: r.maybe(Awkward::flag),
                    brightness: r.maybe(Awkward::number),
                },
                26 => Command::RemoveLightGroup { id: group },
                27 => Command::GroupLights {
                    fixtures: vec![fixture],
                    group: r.maybe(|_| group),
                },
                28 | 29 => Command::Bind {
                    key: ["KeyA", "KeyB", "Digit1", "Space", "Escape", "", "Keyzz"][r.next() as usize % 7].into(),
                    label: r.text(),
                    action: match r.next() % 7 {
                        0 => Action::Scene { id: scene },
                        1 => Action::Layer { id: layer },
                        2 => Action::LayerGroup { id: folder },
                        3 => Action::LightGroup { id: group },
                        4 => Action::Blackout,
                        5 => Action::TapTempo,
                        _ => Action::Hit,
                    },
                    mode: if r.flag() { KeyMode::Hold } else { KeyMode::Toggle },
                },
                30 => Command::UpdateBinding {
                    id: binding,
                    mode: if r.flag() { KeyMode::Hold } else { KeyMode::Toggle },
                },
                31 => Command::Unbind { id: binding },
                32 => Command::SetLive {
                    launch: r.maybe(|r| [Launch::Now, Launch::Beat, Launch::Bar][r.next() as usize % 3]),
                    fade: r.maybe(Awkward::number),
                },
                33 => Command::SetOutput {
                    frame_rate: r.maybe(|r| [0, 30, 40, 60, 61, 120, u32::MAX][r.next() as usize % 7]),
                    flash_limit: r.maybe(Awkward::number),
                },
                34 => Command::RenameScene {
                    id: scene,
                    name: r.text(),
                },
                35 => Command::Trigger {
                    binding,
                    down: r.flag(),
                },
                15 => Command::SetDemo { enabled: r.flag() },
                16 => Command::AddVirtual {
                    name: r.text(),
                    shape: if r.flag() {
                        VirtualShape::Strip {
                            count: [0, 1, 30, 100_000, 100_001, u32::MAX][r.next() as usize % 6],
                        }
                    } else {
                        VirtualShape::Matrix {
                            cols: [0, 1, 2, 16, 70_000, u32::MAX][r.next() as usize % 6],
                            rows: [0, 1, 2, 16, 70_000, u32::MAX][r.next() as usize % 6],
                        }
                    },
                },
                17 => Command::RenameLayer {
                    id: layer,
                    name: r.text(),
                },
                18 => Command::DuplicateLayer { id: layer },
                19 => Command::SetSound {
                    layer,
                    band: r.maybe(|r| match r.next() % 5 {
                        0 => Band::All,
                        1 => Band::Bass,
                        2 => Band::Mid,
                        3 => Band::Treble,
                        _ => Band::Range {
                            low: r.number(),
                            high: r.number(),
                        },
                    }),
                    gain: r.maybe(Awkward::number),
                    pulse: r.maybe(Awkward::number),
                },
                0 => Command::UpdateDevice {
                    id: device,
                    name: r.maybe(Awkward::text),
                    enabled: r.maybe(Awkward::flag),
                    gamma: r.maybe(Awkward::number),
                    brightness: r.maybe(Awkward::number),
                },
                1 => Command::UpdateFixture {
                    id: fixture,
                    name: r.maybe(Awkward::text),
                    x: r.maybe(Awkward::number),
                    y: r.maybe(Awkward::number),
                    width: r.maybe(Awkward::number),
                    height: r.maybe(Awkward::number),
                    rotation: r.maybe(Awkward::number),
                    reverse: r.maybe(Awkward::flag),
                },
                2 => Command::AddLayer {
                    effect: r.text(),
                    group: r.maybe(|_| folder),
                },
                3 => Command::RemoveLayer { id: layer },
                4 => Command::UpdateLayer {
                    id: layer,
                    effect: r.maybe(Awkward::text),
                    opacity: r.maybe(Awkward::number),
                    blend: r.maybe(|r| blends[r.next() as usize % 4]),
                    enabled: r.maybe(Awkward::flag),
                    targets: r.maybe(|r| vec![fixture, if r.flag() { group } else { r.next() % 50 }]),
                },
                5 => Command::RemoveDevice { id: device },
                6 | 7 => Command::SetParam {
                    layer,
                    param: params[r.next() as usize % params.len()].into(),
                    value: match r.next() % 3 {
                        0 => ParamValue::Number(r.number()),
                        1 => ParamValue::Text(r.text()),
                        _ => ParamValue::Bool(r.flag()),
                    },
                },
                8 => Command::ResetParams { layer },
                9 => Command::SetMaster {
                    brightness: r.maybe(Awkward::number),
                    blackout: r.maybe(Awkward::flag),
                    limit: r.maybe(Awkward::number),
                },
                10 => Command::SetAudio {
                    source: r.maybe(Awkward::text),
                    sensitivity: r.maybe(Awkward::number),
                    delay_ms: r.maybe(Awkward::number),
                },
                11 => Command::SaveScene { name: r.text() },
                12 => Command::RecallScene { id: scene },
                13 => Command::DeleteScene { id: scene },
                _ => Command::RenameShow { name: r.text() },
            };
            // Like the server, apply to a copy and keep it only on success.
            let mut next = show.clone();
            if apply(&mut next, &command).is_ok() {
                accepted += 1;
                show = next;
            }
            if let Err(problem) = show.validate() {
                panic!("{command:?} left the show invalid: {problem}");
            }
            assert!(show.layers.len() <= MAX_LAYERS && show.scenes.len() <= MAX_SCENES);
            assert!(show.layer_groups.len() <= MAX_GROUPS && show.light_groups.len() <= MAX_GROUPS);
            assert!(show.bindings.len() <= MAX_BINDINGS);
            for layer in &show.layers {
                assert!((0.0..=1.0).contains(&layer.opacity), "{command:?}");
                assert!(
                    layer.targets.iter().all(|t| {
                        show.fixtures.iter().any(|f| f.id == *t) || show.light_groups.iter().any(|g| g.id == *t)
                    }),
                    "{command:?} left a target that does not exist"
                );
            }
            assert!((0.05..=1.0).contains(&show.master.limit), "{command:?}");
            assert!(show.bindings.iter().all(|b| valid_key(&b.key)), "{command:?}");
            for (i, b) in show.bindings.iter().enumerate() {
                let clash = show.bindings[..i]
                    .iter()
                    .any(|x| x.key == b.key || x.action == b.action);
                assert!(!clash, "{command:?} left two keys for one thing, or one key for two");
            }
            // Nothing dangles: every folder, group and key points at
            // something, and a folder's effects sit together. Mending a
            // show that needs no mending changes nothing.
            let mut mended = show.clone();
            mended.repair();
            assert_eq!(mended, show, "{command:?} left something for repair to fix");
            seen_folders |= !show.layer_groups.is_empty();
            seen_groups |= !show.light_groups.is_empty();
            seen_keys |= !show.bindings.is_empty();
        }
        assert!(
            seen_folders && seen_groups && seen_keys,
            "the new things were never exercised"
        );
        assert!(
            accepted > 2_000,
            "only {accepted} commands were accepted; the test is not exercising much"
        );
    }

    #[test]
    fn layer_count_is_capped() {
        let mut show = show_with_device();
        while show.layers.len() < MAX_LAYERS {
            apply(&mut show, &parse(r#"{"type":"add_layer","effect":"solid"}"#)).unwrap();
        }
        assert!(apply(&mut show, &parse(r#"{"type":"add_layer","effect":"solid"}"#)).is_err());
    }
}
