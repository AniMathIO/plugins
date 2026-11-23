//! Scene emission: converts math-frame geometry into AniMathIO vector draw
//! commands in canvas coordinates (y down). Text uses the host-shaped
//! `TextRun` (embedded DejaVu Sans), so labels render identically everywhere.
//! Element counts are bounded by `MAX_ELEMENTS`.

use super::{
    Color, DrawCommand, PathVerb, Point, Size, StrokeCap, StrokeJoin, StrokeStyle, TextAlignment,
    TextElement,
};
use crate::geom::V;

pub const MAX_ELEMENTS: usize = 4_000;
const MAX_DASHES_PER_LINE: usize = 160;
const ARC_POINTS: usize = 48;

/// Straight-alpha RGBA colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub fn with_alpha(self, alpha: f64) -> Self {
        let a = (self.a as f64 * alpha.clamp(0.0, 1.0)).round() as i64;
        Self {
            a: a.clamp(0, 255) as u8,
            ..self
        }
    }
    fn wit(self) -> Color {
        Color {
            r: self.r,
            g: self.g,
            b: self.b,
            a: self.a,
        }
    }
}

/// Palette: construction lines are muted, rays take the accent colour, the
/// object/image arrows and glass fills are fixed teaching-diagram colours.
pub struct Palette {
    pub ray: Rgba,
    pub construction: Rgba,
    pub label: Rgba,
    pub object: Rgba,
    pub image: Rgba,
    pub glass: Rgba,
    pub marker: Rgba,
    pub white_beam: Rgba,
}

impl Palette {
    pub fn new(accent: Color) -> Self {
        Self {
            ray: Rgba::new(accent.r, accent.g, accent.b, accent.a),
            construction: Rgba::new(150, 158, 172, 150),
            label: Rgba::new(205, 210, 220, 255),
            object: Rgba::new(235, 238, 245, 255),
            image: Rgba::new(126, 226, 142, 255),
            glass: Rgba::new(96, 165, 250, 38),
            marker: Rgba::new(103, 200, 235, 255),
            white_beam: Rgba::new(250, 248, 240, 255),
        }
    }
}

/// Emits ordered draw commands. All geometry comes in through the y-up math
/// frame; `pt` converts to canvas pixels.
pub struct Painter {
    pub elements: Vec<DrawCommand>,
    width: f32,
    height: f32,
    full: bool,
}

impl Painter {
    pub fn new(size: Size) -> Self {
        Self {
            elements: Vec::new(),
            width: size.width as f32,
            height: size.height as f32,
            full: false,
        }
    }

    /// Math frame (origin canvas centre, y up) to canvas pixels.
    pub fn pt(&self, v: V) -> Point {
        Point {
            x: self.width * 0.5 + v.x as f32,
            y: self.height * 0.5 - v.y as f32,
        }
    }

    pub fn push(&mut self, cmd: DrawCommand) -> bool {
        if self.elements.len() >= MAX_ELEMENTS {
            self.full = true;
            return false;
        }
        self.elements.push(cmd);
        true
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    /// Solid polyline stroke through math-frame points.
    pub fn stroke(&mut self, points: &[V], width: f64, color: Rgba) {
        if points.len() < 2 {
            return;
        }
        let path: Vec<PathVerb> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let pt = self.pt(*p);
                if i == 0 {
                    PathVerb::MoveTo(pt)
                } else {
                    PathVerb::LineTo(pt)
                }
            })
            .collect();
        self.push(DrawCommand::StrokePath((
            path,
            StrokeStyle {
                width: width as f32,
                color: color.wit(),
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
            },
        )));
    }

    /// Dashed line between two math-frame points (the scene format has no dash
    /// attribute, so this emits short strokes; bounded per line).
    pub fn dashed(&mut self, a: V, b: V, width: f64, dash: f64, gap: f64, color: Rgba) {
        let total = (b - a).length();
        if total < 1e-9 {
            return;
        }
        let dir = (b - a).unit();
        let period = (dash + gap).max(total / MAX_DASHES_PER_LINE as f64);
        let count = ((total / period).ceil() as usize).min(MAX_DASHES_PER_LINE);
        let mut s = 0.0;
        for _ in 0..count {
            if s >= total {
                break;
            }
            let e = s + dash.min(total - s);
            self.stroke(&[a + dir * s, a + dir * e], width, color);
            s += period;
        }
    }

    /// Filled arrow head at `tip`, pointing along `dir`, size in pixels.
    pub fn arrow_head(&mut self, tip: V, dir: V, size: f64, color: Rgba) {
        let d = dir.unit();
        let side = d.perp() * (size * 0.5);
        let base = tip - d * size;
        let path = vec![
            PathVerb::MoveTo(self.pt(tip)),
            PathVerb::LineTo(self.pt(base + side)),
            PathVerb::LineTo(self.pt(base - side)),
            PathVerb::Close,
        ];
        self.push(DrawCommand::FillPath((path, color.wit())));
    }

    /// Line with an arrow head at the end (object/image arrows).
    pub fn arrow_line(&mut self, from: V, to: V, width: f64, head: f64, color: Rgba) {
        self.stroke(&[from, to], width, color);
        self.arrow_head(to, to - from, head, color);
    }

    /// Filled convex polygon (glass bodies).
    pub fn fill_polygon(&mut self, points: &[V], color: Rgba) {
        if points.len() < 3 {
            return;
        }
        let path: Vec<PathVerb> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let pt = self.pt(*p);
                if i == 0 {
                    PathVerb::MoveTo(pt)
                } else {
                    PathVerb::LineTo(pt)
                }
            })
            .collect();
        let mut path = path;
        path.push(PathVerb::Close);
        self.push(DrawCommand::FillPath((path, color.wit())));
    }

    /// Circle arc centred at `c` from `start` to `end` radians (math frame).
    pub fn arc(&mut self, c: V, radius: f64, start: f64, end: f64, width: f64, color: Rgba) {
        let steps = ARC_POINTS;
        let mut points = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let a = start + (end - start) * i as f64 / steps as f64;
            points.push(V::new(c.x + radius * a.cos(), c.y + radius * a.sin()));
        }
        self.stroke(&points, width, color);
    }

    /// Small filled dot (focal markers, hit points).
    pub fn dot(&mut self, at: V, radius: f64, color: Rgba) {
        let steps = 20;
        let mut path = Vec::with_capacity(steps + 2);
        for i in 0..=steps {
            let a = std::f64::consts::TAU * i as f64 / steps as f64;
            let p = V::new(at.x + radius * a.cos(), at.y + radius * a.sin());
            let verb = if i == 0 {
                PathVerb::MoveTo(self.pt(p))
            } else {
                PathVerb::LineTo(self.pt(p))
            };
            path.push(verb);
        }
        path.push(PathVerb::Close);
        self.push(DrawCommand::FillPath((path, color.wit())));
    }

    /// Host-shaped, horizontally centred text. Canvas text starts at the top
    /// of its line box; offset that box to put the glyphs near `at`.
    pub fn label(&mut self, text: &str, at: V, font_size: f64, color: Rgba) {
        if color.a == 0 || text.is_empty() {
            return;
        }
        let p = self.pt(at);
        self.push(DrawCommand::TextRun(TextElement {
            align: TextAlignment::Center,
            text: text.to_string(),
            font_size: font_size as f32,
            x: p.x - self.width * 0.5,
            y: p.y - font_size as f32 * 0.63,
            color: color.wit(),
        }));
    }
}
