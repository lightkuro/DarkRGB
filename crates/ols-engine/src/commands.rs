//! Every change a client can ask for, and the checks applied before it
//! touches the show. Nothing here does I/O, so all of it is unit tested.

use ols_core::color;
use ols_core::effects;
use ols_core::params::{ParamKind, ParamValue};
use ols_core::show::Blend;
use ols_core::{Rgb, Shape, Show};
use serde::Deserialize;

const MAX_NAME: usize = 80;
const MAX_LAYERS: usize = 32;
const MAX_SCENES: usize = 200;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// Looks the device up on the network, then adds it. Handled by the
    /// app because it needs the network; see `Shared::execute`.
    AddDevice {
        address: String,
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
    AddLayer {
        effect: String,
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
    /// Moves a layer to a position in the stack; 0 is the bottom.
    MoveLayer {
        id: u32,
        to: usize,
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
    },
    SetAudio {
        source: Option<String>,
        sensitivity: Option<f32>,
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
        Command::AddDevice { .. } => Err("adding a device needs the network".into()),

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
            if let Some(n) = new_name {
                device.name = n;
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

        Command::AddLayer { effect } => {
            if effects::find(effect).is_none() {
                return Err(format!("unknown effect {effect:?}"));
            }
            if show.layers.len() >= MAX_LAYERS {
                return Err(format!("a show can have at most {MAX_LAYERS} layers"));
            }
            show.add_layer(effect);
            Ok(())
        }

        Command::RemoveLayer { id } => {
            let before = show.layers.len();
            show.layers.retain(|l| l.id != *id);
            if show.layers.len() == before {
                return Err("that layer no longer exists".into());
            }
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
            if let Some(effect) = effect {
                if effects::find(effect).is_none() {
                    return Err(format!("unknown effect {effect:?}"));
                }
            }
            let opacity = opacity.map(|o| finite(o, "Opacity")).transpose()?;
            if let Some(targets) = targets {
                if let Some(missing) = targets.iter().find(|t| !show.fixtures.iter().any(|f| f.id == **t)) {
                    return Err(format!("fixture {missing} no longer exists"));
                }
            }
            let layer = show.layer_mut(*id).ok_or("that layer no longer exists")?;
            if let Some(effect) = effect {
                if *effect != layer.effect {
                    // Settings belong to the old effect.
                    layer.effect = effect.clone();
                    layer.params.clear();
                }
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

        Command::MoveLayer { id, to } => {
            let from = show
                .layers
                .iter()
                .position(|l| l.id == *id)
                .ok_or("that layer no longer exists")?;
            let layer = show.layers.remove(from);
            let to = (*to).min(show.layers.len());
            show.layers.insert(to, layer);
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

        Command::SetMaster { brightness, blackout } => {
            let brightness = brightness.map(|b| finite(b, "Brightness")).transpose()?;
            if let Some(b) = brightness {
                show.master.brightness = b.clamp(0.0, 1.0);
            }
            if let Some(b) = blackout {
                show.master.blackout = *b;
            }
            Ok(())
        }

        Command::SetAudio { source, sensitivity } => {
            let sensitivity = sensitivity.map(|s| finite(s, "Sensitivity")).transpose()?;
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
            let before = show.scenes.len();
            show.scenes.retain(|s| s.id != *id);
            if show.scenes.len() == before {
                return Err("that scene no longer exists".into());
            }
            Ok(())
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
    use ols_core::Device;

    fn show_with_device() -> Show {
        let mut show = Show::default();
        show.add_device(
            Device {
                id: 0,
                name: "Strip".into(),
                address: "127.0.0.1".into(),
                port: 4048,
                pixel_count: 30,
                enabled: true,
                gamma: 2.2,
                brightness: 1.0,
                mac: None,
                version: None,
            },
            None,
        );
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
                blackout: Some(true)
            }
        );
        assert!(serde_json::from_str::<Command>(r#"{"type":"format_disk"}"#).is_err());
    }

    #[test]
    fn params_are_checked_against_the_effect() {
        let mut show = show_with_device();
        let layer = show.add_layer("solid");
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
        assert!(set(&mut show, "pulse", ParamValue::Number(f32::NAN)).is_err());
        assert!(set(&mut show, "pulse", ParamValue::Number(50.0)).is_ok());
        assert_eq!(show.layer_mut(layer).unwrap().params["pulse"], ParamValue::Number(1.0));
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
        let layer = show.add_layer("solid");
        apply(
            &mut show,
            &Command::SetParam {
                layer,
                param: "pulse".into(),
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
        apply(&mut show, &update("solid")).unwrap();
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
        apply(&mut show, &Command::MoveLayer { id: c, to: 0 }).unwrap();
        assert_eq!(show.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [c, a, b]);
        apply(&mut show, &Command::MoveLayer { id: c, to: 99 }).unwrap();
        assert_eq!(show.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, b, c]);
        apply(&mut show, &Command::RemoveLayer { id: b }).unwrap();
        assert!(apply(&mut show, &Command::RemoveLayer { id: b }).is_err());
        assert_eq!(show.layers.len(), 2);
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

    #[test]
    fn layer_count_is_capped() {
        let mut show = show_with_device();
        while show.layers.len() < MAX_LAYERS {
            apply(&mut show, &Command::AddLayer { effect: "solid".into() }).unwrap();
        }
        assert!(apply(&mut show, &Command::AddLayer { effect: "solid".into() }).is_err());
    }
}
