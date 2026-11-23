//! Vector painting: bonds (single/double/triple, wedge, dash, aromatic
//! circles), host-shaped atom labels with bond trimming, highlights,
//! lone-pair dots, plus signs and the reaction arrow. All emission is a pure
//! function of the geometry and style arguments.
use crate::{Color, DrawCommand, PathVerb, Point, StrokeCap, StrokeJoin, StrokeStyle};

use crate::layout::Pt;

pub fn path_from_points(points: &[Pt]) -> Vec<PathVerb> {
    let mut out = Vec::with_capacity(points.len() + 1);
    for (i, p) in points.iter().enumerate() {
        let p = Point { x: p.0, y: p.1 };
        out.push(if i == 0 {
            PathVerb::MoveTo(p)
        } else {
            PathVerb::LineTo(p)
        });
    }
    out
}

pub fn lerp(a: Pt, b: Pt, t: f32) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn unit(a: Pt, b: Pt) -> (Pt, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        ((1.0, 0.0), 0.0)
    } else {
        ((dx / len, dy / len), len)
    }
}

pub fn with_alpha(mut c: Color, alpha: f32) -> Color {
    c.a = ((c.a as f32) * alpha.clamp(0.0, 1.0)).round() as u8;
    c
}

fn verb_point(p: Point, scale: f32, dx: f32, dy: f32) -> Point {
    Point {
        x: p.x * scale + dx,
        y: p.y * scale + dy,
    }
}

fn transform_path(path: &[PathVerb], scale: f32, dx: f32, dy: f32) -> Vec<PathVerb> {
    path.iter()
        .map(|v| match *v {
            PathVerb::MoveTo(p) => PathVerb::MoveTo(verb_point(p, scale, dx, dy)),
            PathVerb::LineTo(p) => PathVerb::LineTo(verb_point(p, scale, dx, dy)),
            PathVerb::QuadTo((a, b)) => {
                PathVerb::QuadTo((verb_point(a, scale, dx, dy), verb_point(b, scale, dx, dy)))
            }
            PathVerb::CubicTo((a, b, c)) => PathVerb::CubicTo((
                verb_point(a, scale, dx, dy),
                verb_point(b, scale, dx, dy),
                verb_point(c, scale, dx, dy),
            )),
            PathVerb::Close => PathVerb::Close,
        })
        .collect()
}

/// Ink bounding box of a set of contours.
pub fn contours_bbox(paths: &[Vec<PathVerb>]) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for path in paths {
        for v in path {
            let mut include = |p: Point| {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            };
            match *v {
                PathVerb::MoveTo(p) | PathVerb::LineTo(p) => include(p),
                PathVerb::QuadTo((a, b)) => {
                    include(a);
                    include(b);
                }
                PathVerb::CubicTo((a, b, c)) => {
                    include(a);
                    include(b);
                    include(c);
                }
                PathVerb::Close => {}
            }
        }
    }
    if min_x.is_finite() {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// A prepared atom label: merged glyph contours relative to (0,0), ready to
/// translate onto the atom position, plus its half extents for bond trimming.
pub struct Label {
    pub contours: Vec<PathVerb>,
    pub half_w: f32,
    pub half_h: f32,
}

/// Shape `text` through the host engine and center its ink box on the origin.
/// Returns None for empty output (host limit); callers skip the label.
pub fn prepare_label(text: &str, em: f32) -> Option<Label> {
    let shaped = host::shape_text(text, em).ok()?;
    if shaped.is_empty() {
        return None;
    }
    let (min_x, min_y, max_x, max_y) = contours_bbox(&shaped)?;
    let dx = -(min_x + max_x) * 0.5;
    let dy = -(min_y + max_y) * 0.5;
    let mut contours = Vec::new();
    for path in &shaped {
        contours.extend(transform_path(path, 1.0, dx, dy));
    }
    Some(Label {
        half_w: (max_x - min_x) * 0.5,
        half_h: (max_y - min_y) * 0.5,
        contours,
    })
}

use crate::animathio::plugin::host;

#[derive(Clone, Copy)]
pub struct BondStroke<'a> {
    pub points: [Pt; 2],
    pub labels: [Option<&'a Label>; 2],
    pub scale: f32,
    pub width: f32,
    pub progress: f32,
    pub color: Color,
}

impl BondStroke<'_> {
    fn geometry(self, offset: f32, shrink: f32) -> Option<(Pt, Pt)> {
        let [a, b] = self.points;
        let (dir, len) = unit(a, b);
        if len < 1e-3 {
            return None;
        }
        let (nx, ny) = (-dir.1, dir.0);
        let trim = |label: Option<&Label>| {
            let Some(label) = label else {
                return 0.0;
            };
            let padding = self.width * 0.78;
            let x = label.half_w * self.scale + padding;
            let y = label.half_h * self.scale + padding;
            let fit = |extent: f32, direction: f32| {
                if direction.abs() <= 1e-6 {
                    f32::INFINITY
                } else {
                    extent / direction.abs()
                }
            };
            fit(x, dir.0).min(fit(y, dir.1))
        };
        let start = trim(self.labels[0]) + shrink * len;
        let end = len - trim(self.labels[1]) - shrink * len;
        if end <= start {
            return None;
        }
        let at = |dist| {
            (
                a.0 + dir.0 * dist + nx * offset,
                a.1 + dir.1 * dist + ny * offset,
            )
        };
        Some((at(start), at(end)))
    }
}

pub struct Painter {
    pub elements: Vec<DrawCommand>,
}

impl Painter {
    pub fn new() -> Self {
        Painter {
            elements: Vec::new(),
        }
    }

    pub fn fill(&mut self, path: Vec<PathVerb>, color: Color) {
        if path.is_empty() || color.a == 0 {
            return;
        }
        self.elements.push(DrawCommand::FillPath((path, color)));
    }

    pub fn stroke(&mut self, path: Vec<PathVerb>, width: f32, color: Color) {
        if path.len() < 2 || width <= 0.0 || color.a == 0 {
            return;
        }
        self.elements.push(DrawCommand::StrokePath((
            path,
            StrokeStyle {
                width,
                color,
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
            },
        )));
    }

    pub fn line(&mut self, start: Pt, end: Pt, t: f32, width: f32, color: Color) {
        if t > 0.0 {
            self.stroke(
                path_from_points(&[start, lerp(start, end, t)]),
                width,
                color,
            );
        }
    }

    pub fn bond_line(&mut self, bond: BondStroke<'_>, offset: f32, shrink: f32) {
        if let Some((start, end)) = bond.geometry(offset, shrink) {
            self.line(start, end, bond.progress, bond.width, bond.color);
        }
    }

    /// The narrow tip points toward the annotated stereocenter.
    pub fn stereo_bond(&mut self, mut bond: BondStroke<'_>, reverse: bool, solid: bool) {
        if reverse {
            bond.points.swap(0, 1);
            bond.labels.swap(0, 1);
        }
        let Some((start, end)) = bond.geometry(0.0, 0.0) else {
            return;
        };
        let ((dx, dy), _) = unit(start, end);
        let normal = (-dy, dx);
        let half_width = bond.width * 1.55;
        let cross = |s: f32| {
            let at = lerp(start, end, s);
            let spread = half_width * s;
            [
                (at.0 + normal.0 * spread, at.1 + normal.1 * spread),
                (at.0 - normal.0 * spread, at.1 - normal.1 * spread),
            ]
        };
        if solid {
            let [a, b] = cross(bond.progress);
            let mut path = path_from_points(&[start, a, b]);
            path.push(PathVerb::Close);
            self.fill(path, bond.color);
        } else {
            for step in 1..=6 {
                let t = step as f32 / 7.0;
                if t > bond.progress {
                    break;
                }
                self.stroke(path_from_points(&cross(t)), bond.width * 0.58, bond.color);
            }
        }
    }

    /// An aromatic circle sweeps clockwise as its ring finishes drawing.
    pub fn circle(&mut self, center: Pt, radius: f32, t: f32, width: f32, color: Color) {
        let sweep = t * std::f32::consts::TAU;
        let steps = (64.0 * t).ceil() as usize;
        let points: Vec<_> = (0..=steps)
            .map(|i| {
                let angle = i as f32 / steps as f32 * sweep;
                (
                    center.0 + radius * angle.cos(),
                    center.1 + radius * angle.sin(),
                )
            })
            .collect();
        let mut path = path_from_points(&points);
        if t >= 1.0 {
            path.push(PathVerb::Close);
        }
        self.stroke(path, width, color);
    }

    /// Filled disk (highlight halo under an atom).
    pub fn disk(&mut self, center: Pt, radius: f32, color: Color) {
        if radius < 0.5 || color.a == 0 {
            return;
        }
        let k = 0.552_284_7;
        let p = |x: f32, y: f32| Point { x, y };
        let (r, cx, cy) = (radius, center.0, center.1);
        let path = vec![
            PathVerb::MoveTo(p(cx + r, cy)),
            PathVerb::CubicTo((p(cx + r, cy + k * r), p(cx + k * r, cy + r), p(cx, cy + r))),
            PathVerb::CubicTo((p(cx - k * r, cy + r), p(cx - r, cy + k * r), p(cx - r, cy))),
            PathVerb::CubicTo((p(cx - r, cy - k * r), p(cx - k * r, cy - r), p(cx, cy - r))),
            PathVerb::CubicTo((p(cx + k * r, cy - r), p(cx + r, cy - k * r), p(cx + r, cy))),
            PathVerb::Close,
        ];
        self.fill(path, color);
    }

    /// Plus sign between reaction fragments.
    pub fn plus(&mut self, center: Pt, size: f32, width: f32, color: Color) {
        self.stroke(
            path_from_points(&[(center.0 - size, center.1), (center.0 + size, center.1)]),
            width,
            color,
        );
        self.stroke(
            path_from_points(&[(center.0, center.1 - size), (center.0, center.1 + size)]),
            width,
            color,
        );
    }

    /// Reaction arrow: shaft grows with `t` until it reaches `b` minus the
    /// head, then the triangular head lands. Drawn when `t` > 0.
    pub fn arrow(&mut self, a: Pt, b: Pt, t: f32, width: f32, color: Color) {
        if t <= 0.0 {
            return;
        }
        let (dir, len) = unit(a, b);
        if len < 1e-3 {
            return;
        }
        let normal = (-dir.1, dir.0);
        let head_len = (len * 0.19).max(width * 3.0);
        let tip = lerp(a, b, t);
        self.line(a, tip, 1.0, width, color);
        let head = head_len * ((t - 0.76) / 0.24).clamp(0.0, 1.0);
        if head > 0.0 {
            let base = (tip.0 - dir.0 * head, tip.1 - dir.1 * head);
            let a = (
                base.0 + normal.0 * head * 0.47,
                base.1 + normal.1 * head * 0.47,
            );
            let b = (
                base.0 - normal.0 * head * 0.47,
                base.1 - normal.1 * head * 0.47,
            );
            let mut path = path_from_points(&[tip, a, b]);
            path.push(PathVerb::Close);
            self.fill(path, color);
        }
    }

    pub fn draw_label(&mut self, label: &Label, center: Pt, scale: f32, color: Color) {
        self.fill(
            transform_path(&label.contours, scale, center.0, center.1),
            color,
        );
    }

    /// Lone-pair dots: `pairs` groups of two dots fanned away from bonds.
    pub fn lone_pairs(
        &mut self,
        center: Pt,
        away: Pt,
        distance: f32,
        pairs: u8,
        dot: f32,
        color: Color,
    ) {
        let (ax, ay) = away;
        let norm = ax.hypot(ay);
        let (ux, uy) = if norm < 1e-6 {
            (0.0, -1.0)
        } else {
            (ax / norm, ay / norm)
        };
        let (px, py) = (-uy, ux);
        let spacing = dot * 8.5;
        for g in 0..pairs as i32 {
            let along = (g as f32 - (pairs as f32 - 1.0) * 0.5) * spacing;
            let gx = center.0 + ux * distance - px * along;
            let gy = center.1 + uy * distance - py * along;
            for s in [-1.0, 1.0] {
                self.disk(
                    (gx + px * s * dot * 1.4, gy + py * s * dot * 1.4),
                    dot,
                    color,
                );
            }
        }
    }
}

/// +1 when `point` lies on the +normal side of the a→b line, −1 otherwise.
/// The normal is (−dy, dx), so offsetting by `sign * distance` moves the
/// companion line toward `point` (used for ring-enclosing inner lines).
pub fn toward_sign(a: Pt, b: Pt, point: Pt) -> f32 {
    let cross = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
    if cross >= 0.0 {
        1.0
    } else {
        -1.0
    }
}
