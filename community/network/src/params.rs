//! Shared parameter plumbing: readers with clamping defaults and schema helpers.

use super::{
    BoolParam, Color, Diagnostic, EnumParam, NumberRange, ParamEntry, ParamKind, ParamSpec,
    ParamValue,
};

pub fn error(message: &str) -> Diagnostic {
    Diagnostic {
        code: "invalid-input".into(),
        message: message.into(),
        metadata: vec![],
    }
}

pub fn number(
    params: &[ParamEntry],
    id: &str,
    default: f64,
    min: f64,
    max: f64,
) -> Result<f32, Diagnostic> {
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
    Ok(n.clamp(min, max) as f32)
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

pub fn color_of(params: &[ParamEntry], id: &str, default: Color) -> Color {
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
        kind: ParamKind::Color(crate::animathio::plugin::types::ColorParam { default }),
    }
}

/// One of a fixed option set, or an error naming the allowed values. A missing
/// parameter falls back to the default; an unsupported value *or a value of the
/// wrong type* fails with the same clear message.
pub fn one_of<'a>(
    params: &'a [ParamEntry],
    id: &str,
    default: &'a str,
    options: &[&str],
) -> Result<&'a str, Diagnostic> {
    let value = match params.iter().find(|p| p.id == id).map(|p| &p.value) {
        None => Some(default),
        Some(ParamValue::String(s) | ParamValue::EnumChoice(s)) => Some(s.as_str()),
        Some(_) => None,
    };
    value
        .filter(|value| options.contains(value))
        .ok_or_else(|| {
            error(&format!(
                "Unsupported {id}. Expected one of {}",
                options.join(", ")
            ))
        })
}
