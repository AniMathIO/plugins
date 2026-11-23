//! Ray Optics — animated ray-optics diagrams for AniMathIO.
//! Behavioral/structural reference: ricktu288/ray-optics (Apache-2.0); the
//! tracing ideas were studied and reimplemented in Rust, no upstream source
//! is copied. See NOTICE.
wit_bindgen::generate!({ path: "wit", world: "source-plugin" });
use animathio::plugin::types::*;

mod diagram;
mod draw;
mod geom;
mod spectrum;
mod trace;

use draw::{Painter, Palette};

const VERSION: &str = "0.1.0";
const DESCRIPTION: &str = "Animated ray-optics diagrams: thin lens and mirror principal rays, Snell refraction, total internal reflection and prism dispersion";
const SETUPS: [&str; 5] = ["lens-diagram", "refraction", "tir", "mirror", "dispersion"];
const MAX_WAVELENGTH_SAMPLES: usize = spectrum::MAX_SPECTRAL_RAYS;
const MAX_ELEMENTS: usize = draw::MAX_ELEMENTS;

struct RayOptics;

fn error(message: &str) -> Diagnostic {
    Diagnostic {
        code: "invalid-input".into(),
        message: message.into(),
        metadata: vec![],
    }
}

fn string<'a>(params: &'a [ParamEntry], id: &str, default: &'a str) -> &'a str {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match &p.value {
            ParamValue::String(s) | ParamValue::EnumChoice(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or(default)
}

fn number(
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

fn boolean(params: &[ParamEntry], id: &str, default: bool) -> bool {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.value {
            ParamValue::Bool(b) => Some(b),
            _ => None,
        })
        .unwrap_or(default)
}

fn color(params: &[ParamEntry], id: &str, default: Color) -> Color {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.value {
            ParamValue::Color(c) => Some(c),
            _ => None,
        })
        .unwrap_or(default)
}

fn num_param(id: &str, label: &str, min: f64, max: f64, default: f64, step: f64) -> ParamSpec {
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
fn enum_param(id: &str, label: &str, options: &[&str], default: &str) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::EnumChoice(EnumParam {
            options: options.iter().map(|s| (*s).into()).collect(),
            default: default.into(),
        }),
    }
}
fn bool_param(id: &str, label: &str, default: bool) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Bool(BoolParam { default }),
    }
}
fn color_param(id: &str, label: &str, default: Color) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Color(ColorParam { default }),
    }
}

fn validate_setup(setup: &str) -> Result<&'static str, Diagnostic> {
    SETUPS
        .iter()
        .find(|s| **s == setup)
        .copied()
        .ok_or_else(|| error("Unsupported setup"))
}

/// Signed paraxial focal length (f = R/2) of the mirror setup from the
/// `curvature` parameter, in percent of the canvas short edge. Negative
/// curvature is a convex mirror and gets a negative focal length; magnitudes
/// below the 10% radius floor use the floored radius, so 0 is concave.
fn mirror_focal(curvature: f64, unit: f64) -> f64 {
    let radius = curvature.abs().max(10.0) / 100.0 * unit;
    if curvature < 0.0 {
        -radius / 2.0
    } else {
        radius / 2.0
    }
}

impl Guest for RayOptics {
    fn describe() -> Manifest {
        Manifest {
            name: "Ray Optics".into(),
            version: VERSION.into(),
            description: DESCRIPTION.into(),
            kinds: vec!["clip-source".into()],
            metadata: vec![
                ("param.focal_length.unit".into(), "% of short edge".into()),
                (
                    "param.object_distance.unit".into(),
                    "% of short edge".into(),
                ),
                ("param.object_height.unit".into(), "% of short edge".into()),
                (
                    "param.curvature.unit".into(),
                    "% of short edge (radius)".into(),
                ),
                ("param.angle_deg.unit".into(), "degrees".into()),
                (
                    "param.progress.description".into(),
                    "drives ray growth and the TIR angle sweep".into(),
                ),
                (
                    "param.ray_count.description".into(),
                    "spectral sample count in dispersion".into(),
                ),
            ],
            params: vec![
                enum_param("setup", "Setup", &SETUPS, "lens-diagram"),
                num_param("progress", "Progress", 0.0, 1.0, 1.0, 0.01),
                enum_param(
                    "lens_type",
                    "Lens type",
                    &["converging", "diverging"],
                    "converging",
                ),
                num_param("focal_length", "Focal length (%)", 5.0, 50.0, 20.0, 0.5),
                num_param(
                    "object_distance",
                    "Object distance (%)",
                    5.0,
                    90.0,
                    35.0,
                    0.5,
                ),
                num_param("object_height", "Object height (%)", 2.0, 30.0, 15.0, 0.5),
                num_param(
                    "index_1",
                    "Index n₁ (surrounding medium)",
                    1.0,
                    2.5,
                    1.0,
                    0.01,
                ),
                num_param("index", "Index n₂ (element)", 1.0, 2.5, 1.5, 0.01),
                num_param(
                    "curvature",
                    "Signed mirror radius (%)",
                    -50.0,
                    50.0,
                    30.0,
                    0.5,
                ),
                num_param("angle_deg", "Incidence angle (°)", 0.0, 89.0, 45.0, 0.5),
                num_param(
                    "ray_count",
                    "Spectral rays",
                    1.0,
                    MAX_WAVELENGTH_SAMPLES as f64,
                    12.0,
                    1.0,
                ),
                bool_param("show_normal", "Show normal", true),
                bool_param("show_labels", "Show labels", true),
                bool_param("show_focal", "Show focal markers", true),
                bool_param("spectrum", "Disperse into spectrum", true),
                color_param(
                    "accent",
                    "Ray colour",
                    Color {
                        r: 255,
                        g: 205,
                        b: 66,
                        a: 255,
                    },
                ),
            ],
        }
    }

    fn render_clip(
        params: Vec<ParamEntry>,
        _time_ticks: i64,
        size: Size,
    ) -> Result<Scene, Diagnostic> {
        render(&params, size)
    }
}

fn render(params: &[ParamEntry], size: Size) -> Result<Scene, Diagnostic> {
    if size.width == 0 || size.height == 0 {
        return Err(error("Canvas dimensions must be positive"));
    }
    let setup = validate_setup(string(params, "setup", "lens-diagram"))?;
    let progress = number(params, "progress", 1.0, 0.0, 1.0)?;
    let unit = (size.width.min(size.height)) as f64;
    let half_width = size.width as f64 / 2.0;
    let half_height = size.height as f64 / 2.0;

    // Shared geometry parameters (percent-of-short-edge scaling keeps the
    // diagrams resolution independent).
    let focal = number(params, "focal_length", 20.0, 5.0, 50.0)? / 100.0 * unit;
    let lens_sign = match string(params, "lens_type", "converging") {
        "converging" => 1.0,
        "diverging" => -1.0,
        _ => return Err(error("Unsupported lens type")),
    };
    let object_distance = number(params, "object_distance", 35.0, 5.0, 90.0)? / 100.0 * unit;
    let object_height = number(params, "object_height", 15.0, 2.0, 30.0)? / 100.0 * unit;
    let index_1 = number(params, "index_1", 1.0, 1.0, 2.5)?;
    let index = number(params, "index", 1.5, 1.0, 2.5)?;
    let curvature = number(params, "curvature", 30.0, -50.0, 50.0)?;
    let mirror_focal = mirror_focal(curvature, unit);
    let angle = number(params, "angle_deg", 45.0, 0.0, 89.0)?.to_radians();
    let ray_count = number(
        params,
        "ray_count",
        12.0,
        1.0,
        MAX_WAVELENGTH_SAMPLES as f64,
    )?
    .round() as usize;
    let show_normal = boolean(params, "show_normal", true);
    let show_labels = boolean(params, "show_labels", true);
    let show_focal = boolean(params, "show_focal", true);
    let spectrum_on = boolean(params, "spectrum", true);
    let accent = color(
        params,
        "accent",
        Color {
            r: 255,
            g: 205,
            b: 66,
            a: 255,
        },
    );

    let palette = Palette::new(accent);
    let mut painter = Painter::new(size);
    let mut diagram = diagram::Diagram {
        painter: &mut painter,
        palette: &palette,
        progress,
        unit,
        half_width,
        half_height,
        labels: show_labels,
        normal: show_normal,
        focal: show_focal,
    };
    match setup {
        "lens-diagram" => {
            diagram.principal(lens_sign * focal, object_distance, object_height, false)
        }
        "mirror" => diagram.principal(mirror_focal, object_distance, object_height, true),
        "refraction" => diagram.refraction(index_1, index, angle),
        "tir" => diagram.total_internal_reflection(index_1, index, angle),
        "dispersion" => diagram.dispersion(index, angle, ray_count, spectrum_on),
        _ => return Err(error("Unsupported setup")),
    }
    if painter.is_full() {
        return Err(Diagnostic {
            code: "limit-exceeded".into(),
            message: format!("scene exceeds {MAX_ELEMENTS} elements"),
            metadata: vec![],
        });
    }
    Ok(Scene {
        coordinate_space: CoordinateSpace::Canvas,
        elements: painter.elements,
    })
}

export!(RayOptics);

#[cfg(test)]
mod tests;
