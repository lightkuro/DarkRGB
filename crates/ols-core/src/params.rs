//! Effect parameters: what an effect exposes and the values a layer stores.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::color::{self, Palette, Rgb};

/// A stored parameter value. Colours, choices and palettes are text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Bool(bool),
    Number(f32),
    Text(String),
}

pub type Params = BTreeMap<String, ParamValue>;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamKind {
    Float { min: f32, max: f32, step: f32 },
    Color,
    Bool,
    Choice { options: Vec<&'static str> },
    Palette,
}

/// Describes one parameter so the UI can build a control for it.
#[derive(Clone, Debug, Serialize)]
pub struct ParamSpec {
    pub id: &'static str,
    pub label: &'static str,
    #[serde(flatten)]
    pub kind: ParamKind,
    pub default: ParamValue,
}

impl ParamSpec {
    pub fn float(id: &'static str, label: &'static str, min: f32, max: f32, default: f32) -> Self {
        let step = if max - min > 20.0 { 1.0 } else { 0.01 };
        Self {
            id,
            label,
            kind: ParamKind::Float { min, max, step },
            default: ParamValue::Number(default),
        }
    }

    pub fn color(id: &'static str, label: &'static str, default: &'static str) -> Self {
        Self {
            id,
            label,
            kind: ParamKind::Color,
            default: ParamValue::Text(default.to_string()),
        }
    }

    pub fn bool(id: &'static str, label: &'static str, default: bool) -> Self {
        Self {
            id,
            label,
            kind: ParamKind::Bool,
            default: ParamValue::Bool(default),
        }
    }

    pub fn choice(id: &'static str, label: &'static str, options: &[&'static str]) -> Self {
        Self {
            id,
            label,
            kind: ParamKind::Choice {
                options: options.to_vec(),
            },
            default: ParamValue::Text(options[0].to_string()),
        }
    }

    pub fn palette(id: &'static str, label: &'static str, default: &'static str) -> Self {
        Self {
            id,
            label,
            kind: ParamKind::Palette,
            default: ParamValue::Text(default.to_string()),
        }
    }
}

/// Reads a layer's parameters, falling back to each spec's default and
/// clamping numbers to the declared range. Effects never see bad values.
pub struct ParamReader<'a> {
    specs: &'a [ParamSpec],
    values: &'a Params,
}

impl<'a> ParamReader<'a> {
    pub fn new(specs: &'a [ParamSpec], values: &'a Params) -> Self {
        Self { specs, values }
    }

    fn spec(&self, id: &str) -> &'a ParamSpec {
        self.specs
            .iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("effect asked for undeclared parameter {id:?}"))
    }

    fn value(&self, spec: &'a ParamSpec) -> &'a ParamValue {
        self.values.get(spec.id).unwrap_or(&spec.default)
    }

    pub fn float(&self, id: &str) -> f32 {
        let spec = self.spec(id);
        let (min, max) = match spec.kind {
            ParamKind::Float { min, max, .. } => (min, max),
            _ => (f32::MIN, f32::MAX),
        };
        let v = match (self.value(spec), &spec.default) {
            (ParamValue::Number(v), _) if v.is_finite() => *v,
            (_, ParamValue::Number(d)) => *d,
            _ => 0.0,
        };
        v.clamp(min, max)
    }

    pub fn bool(&self, id: &str) -> bool {
        let spec = self.spec(id);
        match (self.value(spec), &spec.default) {
            (ParamValue::Bool(v), _) => *v,
            (_, ParamValue::Bool(d)) => *d,
            _ => false,
        }
    }

    fn text(&self, id: &str) -> &'a str {
        let spec = self.spec(id);
        match (self.value(spec), &spec.default) {
            (ParamValue::Text(v), _) => v,
            (_, ParamValue::Text(d)) => d,
            _ => "",
        }
    }

    pub fn color(&self, id: &str) -> Rgb {
        Rgb::from_hex(self.text(id))
            .or_else(|| match &self.spec(id).default {
                ParamValue::Text(d) => Rgb::from_hex(d),
                _ => None,
            })
            .unwrap_or(Rgb::WHITE)
    }

    /// Index of the chosen option; 0 when the stored text is not an option.
    pub fn choice(&self, id: &str) -> usize {
        let chosen = self.text(id);
        match &self.spec(id).kind {
            ParamKind::Choice { options } => options.iter().position(|o| *o == chosen).unwrap_or(0),
            _ => 0,
        }
    }

    pub fn palette(&self, id: &str) -> &'static Palette {
        color::palette(self.text(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_and_clamps() {
        let specs = vec![
            ParamSpec::float("speed", "Speed", 0.0, 2.0, 1.0),
            ParamSpec::color("tint", "Tint", "#ff0000"),
            ParamSpec::choice("mode", "Mode", &["a", "b"]),
            ParamSpec::bool("mirror", "Mirror", true),
        ];
        let mut values = Params::new();
        let r = ParamReader::new(&specs, &values);
        assert_eq!(r.float("speed"), 1.0);
        assert_eq!(r.color("tint").to_hex(), "#ff0000");
        assert_eq!(r.choice("mode"), 0);
        assert!(r.bool("mirror"));

        values.insert("speed".into(), ParamValue::Number(99.0));
        values.insert("tint".into(), ParamValue::Text("not a colour".into()));
        values.insert("mode".into(), ParamValue::Text("b".into()));
        values.insert("mirror".into(), ParamValue::Text("wrong type".into()));
        let r = ParamReader::new(&specs, &values);
        assert_eq!(r.float("speed"), 2.0);
        assert_eq!(r.color("tint").to_hex(), "#ff0000");
        assert_eq!(r.choice("mode"), 1);
        assert!(r.bool("mirror"));
    }

    #[test]
    fn values_serialise_plainly() {
        let v: Vec<ParamValue> = serde_json::from_str(r##"[true, 0.5, "#00ff00"]"##).unwrap();
        assert_eq!(
            v,
            vec![
                ParamValue::Bool(true),
                ParamValue::Number(0.5),
                ParamValue::Text("#00ff00".into())
            ]
        );
    }
}
