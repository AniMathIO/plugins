//! Scene emission: world-to-canvas fitting and the drawing primitives every scene
//! shares (bodies, arrows, dashed trails, tick-marked ground, readout text).

use super::{
    Color, DrawCommand, PathVerb, Point, Size, StrokeCap, StrokeJoin, StrokeStyle, TextAlignment,
    TextElement,
};

/// Axis-aligned bounding box in world coordinates (metres, y up).
#[derive(Clone, Copy, Debug)]
pub struct Extent {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Extent {
    pub fn new() -> Self {
        Extent {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }
    pub fn add(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }
    pub fn pad(&mut self, m: f64) {
        self.min_x -= m;
        self.min_y -= m;
        self.max_x += m;
        self.max_y += m;
    }
    /// Degenerate boxes become small finite squares so fitting stays finite.
    pub fn sanitized(mut self) -> Self {
        let min_side = 0.5;
        if self.max_x - self.min_x < min_side {
            let c = (self.min_x + self.max_x) * 0.5;
            self.min_x = c - min_side * 0.5;
            self.max_x = c + min_side * 0.5;
        }
        if self.max_y - self.min_y < min_side {
            let c = (self.min_y + self.max_y) * 0.5;
            self.min_y = c - min_side * 0.5;
            self.max_y = c + min_side * 0.5;
        }
        self
    }
}

impl Default for Extent {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps world metres (y up) to canvas pixels (y down) with a uniform scale, so the
/// scene is resolution-independent: the same metres-per-canvas-height at any size.
#[derive(Clone, Copy, Debug)]
pub struct View {
    scale: f64,
    tx: f64,
    ty: f64,
}

impl View {
    /// Fit `extent` into the canvas leaving `margin` of each edge free (fraction of
    /// the corresponding canvas dimension).
    pub fn fit(extent: &Extent, size: Size, margin: f64) -> Self {
        let w = (extent.max_x - extent.min_x).max(1e-6);
        let h = (extent.max_y - extent.min_y).max(1e-6);
        let scale = ((size.width as f64 * (1.0 - 2.0 * margin)) / w)
            .min(size.height as f64 * (1.0 - 2.0 * margin) / h)
            .max(1e-6);
        let center_x = (extent.min_x + extent.max_x) * 0.5;
        let center_y = (extent.min_y + extent.max_y) * 0.5;
        View {
            scale,
            tx: size.width as f64 * 0.5 - center_x * scale,
            ty: size.height as f64 * 0.5 + center_y * scale,
        }
    }
    pub fn point(&self, x: f64, y: f64) -> Point {
        Point {
            x: (self.tx + x * self.scale) as f32,
            y: (self.ty - y * self.scale) as f32,
        }
    }
    /// Canvas length of a world distance.
    pub fn dist(&self, meters: f64) -> f32 {
        (meters * self.scale) as f32
    }
}

const KAPPA: f32 = 0.552_284_8;

/// A circle as four cubic Béziers, canvas pixels.
pub fn circle_path(cx: f32, cy: f32, r: f32) -> Vec<PathVerb> {
    let k = KAPPA * r;
    let p = |x: f32, y: f32| Point { x, y };
    vec![
        PathVerb::MoveTo(p(cx + r, cy)),
        PathVerb::CubicTo((p(cx + r, cy + k), p(cx + k, cy + r), p(cx, cy + r))),
        PathVerb::CubicTo((p(cx - k, cy + r), p(cx - r, cy + k), p(cx - r, cy))),
        PathVerb::CubicTo((p(cx - r, cy - k), p(cx - k, cy - r), p(cx, cy - r))),
        PathVerb::CubicTo((p(cx + k, cy - r), p(cx + r, cy - k), p(cx + r, cy))),
        PathVerb::Close,
    ]
}

/// An open polyline (or closed polygon) through canvas points.
pub fn polyline(points: &[Point], closed: bool) -> Vec<PathVerb> {
    let mut path = Vec::with_capacity(points.len() + 1);
    for (index, point) in points.iter().enumerate() {
        if index == 0 {
            path.push(PathVerb::MoveTo(*point));
        } else {
            path.push(PathVerb::LineTo(*point));
        }
    }
    if closed && points.len() > 2 {
        path.push(PathVerb::Close);
    }
    path
}

/// The four corners of a rotated box, canvas pixels.
pub fn box_corners(cx: f64, cy: f64, hx: f64, hy: f64, angle: f64, view: &View) -> Vec<Point> {
    let (sin, cos) = angle.sin_cos();
    let local = [(hx, hy), (-hx, hy), (-hx, -hy), (hx, -hy)];
    local
        .iter()
        .map(|(dx, dy)| view.point(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos))
        .collect()
}

#[cfg(test)]
pub fn rgb(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

pub fn with_alpha(mut c: Color, a: u8) -> Color {
    c.a = a;
    c
}

pub fn arrow_head(width: f32) -> (f32, f32) {
    let length = width * 3.0 + 7.0;
    (length, length * 0.42)
}

pub const FOREGROUND: Color = Color {
    r: 226,
    g: 229,
    b: 235,
    a: 255,
};
pub const MUTED: Color = Color {
    r: 124,
    g: 132,
    b: 143,
    a: 255,
};
pub const TRAIL: Color = Color {
    r: 126,
    g: 158,
    b: 196,
    a: 255,
};

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
        if path.len() > 1 {
            self.elements.push(DrawCommand::FillPath((path, color)));
        }
    }
    pub fn stroke(&mut self, path: Vec<PathVerb>, width: f32, color: Color) {
        if path.len() > 1 && width > 0.0 {
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
    }
    pub fn line(&mut self, from: Point, to: Point, width: f32, color: Color) {
        self.stroke(polyline(&[from, to], false), width, color);
    }
    /// An arrow from `from` to `to` (canvas pixels): round-cap shaft plus a filled
    /// triangular head sized to the shaft width.
    pub fn arrow(&mut self, from: Point, to: Point, width: f32, color: Color) {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let len = (dx * dx + dy * dy).sqrt();
        let (head, half) = arrow_head(width);
        if len < head * 1.4 {
            return; // too short to read as an arrow
        }
        let (ux, uy) = (dx / len, dy / len);
        let shaft_end = Point {
            x: to.x - ux * head * 0.9,
            y: to.y - uy * head * 0.9,
        };
        self.line(from, shaft_end, width, color);
        let tip_a = Point {
            x: to.x - ux * head - uy * half,
            y: to.y - uy * head + ux * half,
        };
        let tip_b = Point {
            x: to.x - ux * head + uy * half,
            y: to.y - uy * head - ux * half,
        };
        self.fill(polyline(&[to, tip_a, tip_b], true), color);
    }
    /// A dashed polyline in canvas pixels, fixed pixel dash lengths so the rhythm
    /// does not change with zoom.
    pub fn dashed(&mut self, points: &[Point], width: f32, color: Color, on: f32, off: f32) {
        if points.len() < 2 {
            return;
        }
        // Scale the dashes on long paths to bound the number of emitted paths.
        let length: f32 = points
            .windows(2)
            .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
            .sum();
        let factor = (length / ((on + off) * 96.0)).max(1.0);
        let (on, off) = (on * factor, off * factor);
        let mut segment: Vec<Point> = Vec::new();
        let mut drawing = true;
        let mut remaining = on;
        for pair in points.windows(2) {
            let (ax, ay) = (pair[0].x, pair[0].y);
            let (bx, by) = (pair[1].x, pair[1].y);
            let mut seg_len = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            let (ux, uy) = ((bx - ax) / seg_len.max(1e-6), (by - ay) / seg_len.max(1e-6));
            let mut cursor = Point { x: ax, y: ay };
            while seg_len > 1e-3 {
                let piece = seg_len.min(remaining);
                let next = Point {
                    x: cursor.x + ux * piece,
                    y: cursor.y + uy * piece,
                };
                if drawing {
                    if segment.is_empty() {
                        segment.push(cursor);
                    }
                    segment.push(next);
                }
                cursor = next;
                seg_len -= piece;
                remaining -= piece;
                if remaining <= 1e-3 {
                    if drawing && segment.len() > 1 {
                        self.stroke(polyline(&segment, false), width, color);
                    }
                    segment.clear();
                    drawing = !drawing;
                    remaining = if drawing { on } else { off };
                }
            }
        }
        if drawing && segment.len() > 1 {
            self.stroke(polyline(&segment, false), width, color);
        }
    }
    /// Host-shaped text at canvas pixels; (`x`, `y`) is the top-left of the line box
    /// after the alignment layout across the canvas width.
    pub fn text(
        &mut self,
        text: &str,
        align: TextAlignment,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
    ) {
        if text.is_empty() {
            return;
        }
        self.elements.push(DrawCommand::TextRun(TextElement {
            align,
            text: text.into(),
            font_size: size.max(4.0),
            x,
            y,
            color,
        }));
    }
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

/// A ground ruler with graduations in metres. Large ranges use coarser ticks.
pub fn draw_ground(
    painter: &mut Painter,
    view: &View,
    x0: f64,
    x1: f64,
    label_every: u32,
    canvas: Size,
    tick_font: f32,
) {
    let a = view.point(x0, 0.0);
    let b = view.point(x1, 0.0);
    painter.line(a, b, 3.0, MUTED);
    let raw_step = ((x1 - x0) / 64.0).max(1.0);
    let order = 10_f64.powf(raw_step.log10().floor());
    let ratio = raw_step / order;
    let step = order
        * if ratio <= 1.0 {
            1.0
        } else if ratio <= 2.0 {
            2.0
        } else if ratio <= 5.0 {
            5.0
        } else {
            10.0
        };
    let first = (x0 / step).ceil() as i64;
    let last = (x1 / step).floor() as i64;
    for index in first..=last {
        let m = index * step as i64;
        let tick = view.point(m as f64, 0.0);
        let labeled = index % i64::from(label_every) == 0;
        let height = if labeled { 12.0 } else { 6.0 };
        painter.line(
            tick,
            Point {
                x: tick.x,
                y: tick.y + height,
            },
            1.5,
            with_alpha(MUTED, 200),
        );
        if labeled {
            // Center the label on the tick: offset from the canvas-centred layout.
            let offset = tick.x - canvas.width as f32 * 0.5;
            painter.text(
                &m.to_string(),
                TextAlignment::Center,
                offset,
                tick.y + 18.0,
                tick_font,
                with_alpha(MUTED, 220),
            );
        }
    }
}
