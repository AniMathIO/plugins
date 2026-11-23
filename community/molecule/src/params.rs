//! Bounded parameter readers and schema builders (same pattern as
//! `community/code-snippets`: every value is clamped/validated before use).
use crate::{Color, Diagnostic, ParamEntry, ParamKind, ParamSpec, ParamValue};

pub fn error(code: &str, message: &str) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: message.into(),
        metadata: vec![],
    }
}

pub fn invalid(message: &str) -> Diagnostic {
    error("invalid-input", message)
}

pub fn limit(message: &str) -> Diagnostic {
    error("limit-exceeded", message)
}

/// Read a string or enum-choice parameter, falling back to `default`.
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

/// Read a finite number parameter, clamped to `[min, max]`.
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
        return Err(invalid("Numeric parameters must be finite"));
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

/// Read an enum-choice parameter; unknown values are rejected so saved
/// projects never render with a silently different meaning.
pub fn choice<'a>(
    params: &'a [ParamEntry],
    id: &str,
    options: &[&str],
    default: &'a str,
) -> Result<&'a str, Diagnostic> {
    let value = string(params, id, default);
    if options.contains(&value) {
        Ok(value)
    } else {
        Err(invalid(&format!(
            "{id} must be one of: {}",
            options.join(", ")
        )))
    }
}

/// Read a color parameter with a fallback default.
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

pub fn text_param(id: &str, label: &str, default: &str) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::String(crate::StringParam {
            default: default.into(),
        }),
    }
}

pub fn enum_param(id: &str, label: &str, options: &[&str], default: &str) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::EnumChoice(crate::EnumParam {
            options: options.iter().map(|s| (*s).into()).collect(),
            default: default.into(),
        }),
    }
}

pub fn num_param(id: &str, label: &str, min: f64, max: f64, default: f64) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Number(crate::NumberRange {
            min,
            max,
            default,
            step: 0.01,
        }),
    }
}

pub fn bool_param(id: &str, label: &str, default: bool) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Bool(crate::BoolParam { default }),
    }
}

pub fn color_param(id: &str, label: &str, default: Color) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Color(crate::ColorParam { default }),
    }
}

/// Atom selections are one-based and bond selections have a b prefix and
/// are zero-based. Both can appear in the same comma-separated list.
#[derive(Debug, Default)]
pub struct Highlights {
    pub atoms: Vec<usize>,
    pub bonds: Vec<usize>,
}

pub fn highlights(spec: &str) -> Result<Highlights, Diagnostic> {
    use crate::molecule::{MAX_ATOMS, MAX_BONDS};

    if spec.len() > 512 {
        return Err(limit("Highlight selection exceeds 512 bytes"));
    }
    let mut result = Highlights::default();
    if spec.trim().is_empty() {
        return Ok(result);
    }
    let mut atoms = [false; MAX_ATOMS];
    let mut bonds = [false; MAX_BONDS];
    for term in spec.split(',') {
        let term = term.trim();
        let (indices, selected, base) = match term.strip_prefix('b') {
            Some(rest) => (rest, bonds.as_mut_slice(), 0),
            None => (term, atoms.as_mut_slice(), 1),
        };
        let (first, last) = indices.split_once('-').unwrap_or((indices, indices));
        let parse = |index: &str| {
            index
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|i| i.checked_sub(base))
                .filter(|&i| i < selected.len())
        };
        let (Some(first), Some(last)) = (parse(first), parse(last)) else {
            return Err(invalid(
                "Use atom indices 1–128 or bond indices b0–b255, for example 1,4-6,b0,b2-3",
            ));
        };
        if first > last {
            return Err(invalid("Highlight ranges must be in ascending order"));
        }
        selected[first..=last].fill(true);
    }
    result.atoms = atoms
        .into_iter()
        .enumerate()
        .filter_map(|(i, on)| on.then_some(i))
        .collect();
    result.bonds = bonds
        .into_iter()
        .enumerate()
        .filter_map(|(i, on)| on.then_some(i))
        .collect();
    Ok(result)
}
