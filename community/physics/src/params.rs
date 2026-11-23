//! Parameter readers and schema builders. Every numeric read is finite-checked and
//! clamped to its declared range, so rendering can trust the values (pattern shared
//! with `community/code-snippets`).

use super::{
    BoolParam, Color, ColorParam, Diagnostic, EnumParam, NumberRange, ParamEntry, ParamKind,
    ParamSpec, ParamValue,
};

pub fn error(message: &str) -> Diagnostic {
    Diagnostic {
        code: "invalid-input".into(),
        message: message.into(),
        metadata: vec![],
    }
}

pub fn string<'a>(params: &'a [ParamEntry], id: &str, default: &'a str) -> &'a str {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match &p.value {
            ParamValue::String(s) | ParamValue::EnumChoice(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or(default)
}

pub fn number(
    params: &[ParamEntry],
    id: &str,
    default: f64,
    min: f64,
    max: f64,
) -> Result<f64, Diagnostic> {
    let n = params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.value {
            ParamValue::Number(n) => Some(n),
            _ => None,
        })
        .unwrap_or(default);
    if !n.is_finite() {
        return Err(error("Numeric parameters must be finite"));
    }
    Ok(n.clamp(min, max))
}

pub fn boolean(params: &[ParamEntry], id: &str, default: bool) -> bool {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.value {
            ParamValue::Bool(b) => Some(b),
            _ => None,
        })
        .unwrap_or(default)
}

pub fn color(params: &[ParamEntry], id: &str, default: Color) -> Color {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.value {
            ParamValue::Color(c) => Some(c),
            _ => None,
        })
        .unwrap_or(default)
}

pub fn enum_param(id: &str, label: &str, options: &[&str], default: &str) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::EnumChoice(EnumParam {
            options: options.iter().map(|s| (*s).into()).collect(),
            default: default.into(),
        }),
    }
}

pub fn num_param(id: &str, label: &str, min: f64, max: f64, default: f64, step: f64) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Number(NumberRange {
            min,
            max,
            default,
            step,
        }),
    }
}

pub fn bool_param(id: &str, label: &str, default: bool) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Bool(BoolParam { default }),
    }
}

pub fn color_param(id: &str, label: &str, default: Color) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Color(ColorParam { default }),
    }
}
