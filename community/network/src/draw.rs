//! Scene drawing: topology glyphs, packets in flight, panels. All geometry is
//! computed from the canvas size, so output is resolution-independent and a pure
//! function of (params, timeline position).

use super::scenario::{Event, Proto, Scenario, TableKind};
use super::tables::TableState;
use super::timeline;
use super::topology::{DeviceKind, Topology, CLIENT};
use crate::animathio::plugin::types::{
    Color, CoordinateSpace, DrawCommand, PathVerb, Point, Scene, Size, StrokeCap, StrokeJoin,
    StrokeStyle, TextAlignment, TextElement,
};

// Palette (dark teaching board).
const TEXT_BRIGHT: Color = Color {
    r: 235,
    g: 240,
    b: 245,
    a: 255,
};
const TEXT_DIM: Color = Color {
    r: 158,
    g: 176,
    b: 189,
    a: 255,
};
const LINK_COLOR: Color = Color {
    r: 91,
    g: 112,
    b: 128,
    a: 255,
};
const DEVICE_FILL: Color = Color {
    r: 30,
    g: 41,
    b: 54,
    a: 255,
};
const DEVICE_STROKE: Color = Color {
    r: 168,
    g: 190,
    b: 205,
    a: 255,
};
const PANEL_BG: Color = Color {
    r: 17,
    g: 24,
    b: 33,
    a: 240,
};
const PANEL_STROKE: Color = Color {
    r: 52,
    g: 67,
    b: 82,
    a: 255,
};
const PACKET_EDGE: Color = Color {
    r: 12,
    g: 18,
    b: 24,
    a: 255,
};
const PORT_TEXT: Color = Color {
    r: 130,
    g: 148,
    b: 162,
    a: 255,
};
const PACKET_TEXT_DARK: Color = Color {
    r: 20,
    g: 26,
    b: 32,
    a: 255,
};
const PACKET_TEXT_LIGHT: Color = Color {
    r: 246,
    g: 250,
    b: 252,
    a: 255,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Labels {
    Full,
    Brief,
    None,
}

pub struct DrawInput<'a> {
    pub topo: &'static Topology,
    pub events: &'a [Event],
    pub scenario: Scenario,
    /// Timeline position (event slots) from `timeline::position`.
    pub u: f32,
    pub labels: Labels,
    pub table_kind: TableKind,
    pub show_tables: bool,
    pub show_step_list: bool,
    pub speed_lines: bool,
    /// Packet glyph size as a percentage of the short canvas edge (0.5..6).
    pub packet_percent: f32,
    pub accent: Color,
}

struct Painter {
    elements: Vec<DrawCommand>,
    width: f32,
}

impl Painter {
    fn new(width: f32) -> Self {
        Self {
            elements: Vec::new(),
            width,
        }
    }

    fn with_alpha(mut color: Color, alpha: f32) -> Color {
        color.a = ((color.a as f32) * alpha.clamp(0.0, 1.0)).round() as u8;
        color
    }

    fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<PathVerb> {
        let r = r.max(0.0).min(w * 0.5).min(h * 0.5);
        let k = 0.552_284_8 * r;
        let p = |px: f32, py: f32| Point { x: px, y: py };
        vec![
            PathVerb::MoveTo(p(x + r, y)),
            PathVerb::LineTo(p(x + w - r, y)),
            PathVerb::CubicTo((p(x + w - r + k, y), p(x + w, y + r - k), p(x + w, y + r))),
            PathVerb::LineTo(p(x + w, y + h - r)),
            PathVerb::CubicTo((
                p(x + w, y + h - r + k),
                p(x + w - r + k, y + h),
                p(x + w - r, y + h),
            )),
            PathVerb::LineTo(p(x + r, y + h)),
            PathVerb::CubicTo((p(x + r - k, y + h), p(x, y + h - r + k), p(x, y + h - r))),
            PathVerb::LineTo(p(x, y + r)),
            PathVerb::CubicTo((p(x, y + r - k), p(x + r - k, y), p(x + r, y))),
            PathVerb::Close,
        ]
    }

    fn fill(&mut self, path: Vec<PathVerb>, color: Color) {
        if color.a == 0 {
            return;
        }
        self.elements.push(DrawCommand::FillPath((path, color)));
    }

    fn fill_round(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
        if w <= 0.0 || h <= 0.0 || color.a == 0 {
            return;
        }
        self.fill(Self::rounded_rect(x, y, w, h, r), color);
    }

    fn stroke(&mut self, path: Vec<PathVerb>, width: f32, color: Color) {
        if color.a == 0 || width <= 0.0 {
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

    #[allow(clippy::too_many_arguments)] // (x, y, w, h, r) mirror `fill_round`.
    fn stroke_round(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, width: f32, color: Color) {
        if w <= 0.0 || h <= 0.0 || color.a == 0 || width <= 0.0 {
            return;
        }
        self.stroke(Self::rounded_rect(x, y, w, h, r), width, color);
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color) {
        if color.a == 0 || width <= 0.0 {
            return;
        }
        self.stroke(
            vec![
                PathVerb::MoveTo(Point { x: x1, y: y1 }),
                PathVerb::LineTo(Point { x: x2, y: y2 }),
            ],
            width,
            color,
        );
    }

    fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        if color.a == 0 || r <= 0.0 {
            return;
        }
        let k = 0.552_284_8 * r;
        let p = |dx: f32, dy: f32| Point {
            x: cx + dx,
            y: cy + dy,
        };
        let path = vec![
            PathVerb::MoveTo(p(r, 0.0)),
            PathVerb::CubicTo((p(r, k), p(k, r), p(0.0, r))),
            PathVerb::CubicTo((p(-k, r), p(-r, k), p(-r, 0.0))),
            PathVerb::CubicTo((p(-r, -k), p(-k, -r), p(0.0, -r))),
            PathVerb::CubicTo((p(k, -r), p(r, -k), p(r, 0.0))),
            PathVerb::Close,
        ];
        self.fill(path, color);
    }

    fn dashed_circle(&mut self, cx: f32, cy: f32, r: f32, width: f32, color: Color) {
        if color.a == 0 || r <= 0.0 {
            return;
        }
        let segments = 24;
        let step = std::f32::consts::TAU / (segments as f32);
        for i in 0..segments {
            if i % 2 == 1 {
                continue;
            }
            let a0 = i as f32 * step;
            let a1 = a0 + step;
            self.line(
                cx + r * a0.cos(),
                cy + r * a0.sin(),
                cx + r * a1.cos(),
                cy + r * a1.sin(),
                width,
                color,
            );
        }
    }

    /// Horizontal arrow with a solid triangular head.
    fn arrow(&mut self, x1: f32, y: f32, x2: f32, head: f32, width: f32, color: Color) {
        self.line(x1, y, x2, y, width, color);
        let dir = (x2 - x1).signum();
        let path = vec![
            PathVerb::MoveTo(Point {
                x: x2 - dir * head * 0.6,
                y: y - head,
            }),
            PathVerb::LineTo(Point {
                x: x2 + dir * head,
                y,
            }),
            PathVerb::LineTo(Point {
                x: x2 - dir * head * 0.6,
                y: y + head,
            }),
            PathVerb::Close,
        ];
        self.fill(path, color);
    }

    /// The released host aligns a canvas text run within the canvas width.
    fn text_center(&mut self, text: &str, cx: f32, baseline: f32, font: f32, color: Color) {
        if text.is_empty() || color.a == 0 || font <= 0.0 {
            return;
        }
        self.elements.push(DrawCommand::TextRun(TextElement {
            align: TextAlignment::Center,
            text: text.to_string(),
            font_size: font,
            x: cx - self.width * 0.5,
            y: baseline - font * 0.93,
            color,
        }));
    }

    fn text_left(&mut self, text: &str, x: f32, baseline: f32, font: f32, color: Color) {
        if text.is_empty() || color.a == 0 || font <= 0.0 {
            return;
        }
        self.elements.push(DrawCommand::TextRun(TextElement {
            align: TextAlignment::Left,
            text: text.to_string(),
            font_size: font,
            x,
            y: baseline - font * 0.93,
            color,
        }));
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Truncate to `max_chars` characters with an ellipsis (deterministic).
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn proto_color(proto: Proto) -> Color {
    let (r, g, b) = proto.rgba();
    Color { r, g, b, a: 255 }
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn centered(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x: x - w * 0.5,
            y: y - h * 0.5,
            w,
            h,
        }
    }

    fn padded(self, pad: f32) -> Self {
        Self {
            x: self.x - pad,
            y: self.y - pad,
            w: self.w + 2.0 * pad,
            h: self.h + 2.0 * pad,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }

    fn right(self) -> f32 {
        self.x + self.w
    }

    fn bottom(self) -> f32 {
        self.y + self.h
    }
}

/// One content row of the table panel: two columns plus the event index that
/// inserted it (drives the "just learned" highlight).
struct TableRow {
    left: String,
    right: String,
    at: Option<usize>,
}

fn empty_row(label: &str) -> TableRow {
    TableRow {
        left: label.to_string(),
        right: String::new(),
        at: None,
    }
}

/// Table panel content for the effective table kind.
fn table_rows(input: &DrawInput, state: &TableState) -> (&'static str, Vec<TableRow>) {
    let kind = match input.table_kind {
        TableKind::Auto => input.scenario.auto_table(input.topo),
        other => other,
    };
    match kind {
        TableKind::Arp => {
            let rows = if state.arp.is_empty() {
                vec![empty_row("Waiting for a frame")]
            } else {
                state
                    .arp
                    .iter()
                    .map(|r| TableRow {
                        left: r.ip.to_string(),
                        right: r.mac.to_string(),
                        at: Some(r.at),
                    })
                    .collect()
            };
            ("PC1 ARP cache", rows)
        }
        TableKind::Route => {
            let rows = if input.topo.routes.is_empty() {
                vec![empty_row("No router on this LAN")]
            } else {
                input
                    .topo
                    .routes
                    .iter()
                    .map(|r| TableRow {
                        left: r.destination.to_string(),
                        right: format!("{} · {}", r.kind, r.interface),
                        at: None,
                    })
                    .collect()
            };
            ("Router routing table", rows)
        }
        _ => {
            let rows = if state.mac.is_empty() {
                vec![empty_row("Waiting for a frame")]
            } else {
                state
                    .mac
                    .iter()
                    .map(|r| TableRow {
                        left: r.port.to_string(),
                        right: r.mac.to_string(),
                        at: Some(r.at),
                    })
                    .collect()
            };
            ("Switch MAC table", rows)
        }
    }
}

fn panel(p: &mut Painter, rect: Rect, short: f32) {
    let Rect { x, y, w, h } = rect;
    p.fill_round(x, y, w, h, 0.012 * short, PANEL_BG);
    p.stroke_round(
        x,
        y,
        w,
        h,
        0.012 * short,
        (0.0018 * short).max(1.0),
        PANEL_STROKE,
    );
}

fn glyph_body(cx: f32, cy: f32, kind: DeviceKind, icon: f32) -> (Rect, f32) {
    let (width, height, center_y, corners) = match kind {
        DeviceKind::Host => (0.92, 0.64, -0.14, 0.07),
        DeviceKind::Switch => (1.20, 0.44, 0.0, 0.09),
        DeviceKind::Router => (1.10, 0.84, 0.0, 0.20),
        DeviceKind::Server => (0.68, 1.08, 0.0, 0.08),
    };
    (
        Rect::centered(cx, cy + center_y * icon, width * icon, height * icon),
        corners * icon,
    )
}

fn glyph_stroke(short: f32) -> f32 {
    (0.004 * short).max(1.0)
}

fn glyph_bounds(cx: f32, cy: f32, kind: DeviceKind, icon: f32, short: f32) -> Rect {
    let (body, _) = glyph_body(cx, cy, kind, icon);
    let mut bounds = body.padded(glyph_stroke(short) * 0.5);
    if kind == DeviceKind::Host {
        // Include the monitor stand and the round cap of its base.
        bounds.h = bounds.h.max(cy + (0.34 + 0.035 / 2.0) * icon - bounds.y);
    }
    bounds
}

/// Vector device glyph, drawn from scratch (original artwork; see NOTICE).
fn glyph(p: &mut Painter, cx: f32, cy: f32, kind: DeviceKind, s: f32, short: f32, accent: Color) {
    let (body, corners) = glyph_body(cx, cy, kind, s);
    let Rect { x, y, w, h } = body;
    p.fill_round(x, y, w, h, corners, DEVICE_FILL);
    p.stroke_round(x, y, w, h, corners, glyph_stroke(short), DEVICE_STROKE);
    match kind {
        DeviceKind::Host => {
            p.fill_round(
                cx - 0.11 * s,
                cy + 0.18 * s,
                0.22 * s,
                0.16 * s,
                0.02 * s,
                DEVICE_FILL,
            );
            p.line(
                cx - 0.30 * s,
                cy + 0.34 * s,
                cx + 0.30 * s,
                cy + 0.34 * s,
                0.035 * s,
                DEVICE_STROKE,
            );
        }
        DeviceKind::Switch => {
            p.arrow(
                cx - 0.40 * s,
                cy - 0.10 * s,
                cx + 0.34 * s,
                0.07 * s,
                0.020 * s,
                Painter::with_alpha(accent, 0.95),
            );
            p.arrow(
                cx + 0.40 * s,
                cy + 0.12 * s,
                cx - 0.34 * s,
                0.07 * s,
                0.020 * s,
                Painter::with_alpha(accent, 0.70),
            );
        }
        DeviceKind::Router => {
            p.arrow(
                cx - 0.36 * s,
                cy - 0.16 * s,
                cx + 0.30 * s,
                0.08 * s,
                0.022 * s,
                Painter::with_alpha(accent, 0.95),
            );
            p.arrow(
                cx + 0.36 * s,
                cy + 0.16 * s,
                cx - 0.30 * s,
                0.08 * s,
                0.022 * s,
                Painter::with_alpha(accent, 0.70),
            );
        }
        DeviceKind::Server => {
            p.line(
                cx - 0.20 * s,
                cy - 0.30 * s,
                cx + 0.20 * s,
                cy - 0.30 * s,
                0.012 * s,
                Painter::with_alpha(DEVICE_STROKE, 0.55),
            );
            p.line(
                cx - 0.20 * s,
                cy - 0.12 * s,
                cx + 0.20 * s,
                cy - 0.12 * s,
                0.012 * s,
                Painter::with_alpha(DEVICE_STROKE, 0.55),
            );
            p.fill_circle(
                cx + 0.16 * s,
                cy + 0.28 * s,
                0.04 * s,
                Painter::with_alpha(accent, 0.95),
            );
        }
    }
}

/// Visual knobs of a packet glyph, bundled to keep `packet` readable.
struct PacketStyle {
    labels: Labels,
    speed_lines: bool,
    /// Glyph size in canvas pixels.
    base: f32,
    short: f32,
}

fn packet_style(input: &DrawInput, rect: Rect, short: f32) -> PacketStyle {
    PacketStyle {
        labels: input.labels,
        speed_lines: input.speed_lines,
        base: (short * input.packet_percent / 100.0).min(rect.w * 0.06),
        short,
    }
}

struct Packet {
    px: f32,
    py: f32,
    radius: f32,
    plate: Option<Rect>,
    label: String,
    font: f32,
}

fn packet_geometry(
    (ax, ay): (f32, f32),
    (bx, by): (f32, f32),
    t: f32,
    event: &Event,
    style: &PacketStyle,
) -> Packet {
    let px = lerp(ax, bx, t);
    let py = lerp(ay, by, t);
    if style.labels == Labels::None {
        return Packet {
            px,
            py,
            radius: style.base * 0.62,
            plate: None,
            label: String::new(),
            font: 0.0,
        };
    }

    let radius = style.base * 0.34;
    let base = if style.labels == Labels::Full {
        event.proto.full()
    } else {
        event.proto.short()
    };
    let label = match event.ttl {
        Some(ttl) => format!("{base} · TTL {ttl}"),
        None => base.to_string(),
    };
    let font = style.base.min(style.short * 0.034) * 0.68;
    let width = (label.chars().count() as f32 * 0.62 + 1.5) * font;
    let height = font * 2.04;
    let bottom = py - radius - style.short * 0.007;
    Packet {
        px,
        py,
        radius,
        plate: Some(Rect::centered(px, bottom - height * 0.5, width, height)),
        label,
        font,
    }
}

fn packet_rim(short: f32) -> f32 {
    short * 0.0016
}

/// One packet glyph (possibly with speed lines) travelling `from → leg`.
fn packet(
    p: &mut Painter,
    (ax, ay): (f32, f32),
    (bx, by): (f32, f32),
    t: f32,
    proto: Proto,
    style: &PacketStyle,
    shape: &Packet,
) {
    let fill = proto_color(proto);
    if style.speed_lines && t > 0.14 {
        let (t0, t1, t2) = (
            (t - 0.16).max(0.0),
            (t - 0.05).max(0.0),
            (t - 0.028).max(0.0),
        );
        p.line(
            lerp(ax, bx, t0),
            lerp(ay, by, t0),
            lerp(ax, bx, t1),
            lerp(ay, by, t1),
            style.short * 0.003,
            Painter::with_alpha(fill, 0.35),
        );
        p.line(
            lerp(ax, bx, t1),
            lerp(ay, by, t1),
            lerp(ax, bx, t2),
            lerp(ay, by, t2),
            style.short * 0.002,
            Painter::with_alpha(fill, 0.22),
        );
    }
    let (px, py) = (shape.px, shape.py);
    let edge = packet_rim(style.short);
    p.fill_circle(px, py, shape.radius + edge, PACKET_EDGE);
    p.fill_circle(px, py, shape.radius, fill);
    let Some(plate) = shape.plate else {
        return;
    };
    p.fill_round(
        plate.x,
        plate.y,
        plate.w,
        plate.h,
        plate.h * 0.30,
        PACKET_EDGE,
    );
    p.fill_round(
        plate.x + edge,
        plate.y + edge,
        plate.w - edge * 2.0,
        plate.h - edge * 2.0,
        plate.h * 0.26,
        Painter::with_alpha(fill, 0.94),
    );
    let (r, g, b) = proto.rgba();
    let luminance = (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0;
    let text = if luminance > 0.62 {
        PACKET_TEXT_DARK
    } else {
        PACKET_TEXT_LIGHT
    };
    p.text_center(
        &shape.label,
        px,
        plate.y + plate.h * 0.5 + shape.font * 0.36,
        shape.font,
        text,
    );
}

fn text_fit(
    p: &mut Painter,
    text: &str,
    x: f32,
    baseline: f32,
    font: f32,
    width: f32,
    color: Color,
) {
    let chars = (width / (font * 0.59)).floor().max(1.0) as usize;
    p.text_left(&truncate(text, chars), x, baseline, font, color);
}

struct DiagramMetrics {
    icon: f32,
    em: f32,
    tick: f32,
    port_font: f32,
}

fn device_centers(topo: &Topology, rect: Rect) -> Vec<(f32, f32)> {
    topo.anchors
        .iter()
        .map(|&(x, y)| (rect.x + x * rect.w, rect.y + y * rect.h))
        .collect()
}

fn diagram_metrics(rect: Rect, short: f32) -> DiagramMetrics {
    let icon = (short * 0.112).min(rect.w * 0.14).min(rect.h * 0.19);
    let em = (short * 0.025).min(rect.w * 0.032);
    DiagramMetrics {
        icon,
        em,
        tick: icon * 0.11,
        port_font: em * 0.67,
    }
}

struct LinkTicks {
    a: (f32, f32),
    b: (f32, f32),
    along: (f32, f32),
    normal: (f32, f32),
    label_travel: f32,
}

fn link_ticks((ax, ay): (f32, f32), (bx, by): (f32, f32), icon: f32) -> LinkTicks {
    let (dx, dy) = (bx - ax, by - ay);
    let len = dx.hypot(dy);
    let (ux, uy) = (dx / len.max(f32::EPSILON), dy / len.max(f32::EPSILON));
    let inset = (icon * 0.68).min(len * 0.35);
    // The pill sits above the wire. Put labels below it, or to the right
    // when the wire runs vertically.
    let normal = if dx == 0.0 {
        (1.0, 0.0)
    } else {
        (-uy * dx.signum(), ux.abs())
    };
    LinkTicks {
        a: (ax + ux * inset, ay + uy * inset),
        b: (bx - ux * inset, by - uy * inset),
        along: (ux, uy),
        normal,
        label_travel: len * 0.49 - inset,
    }
}

fn port_label_bounds(label: &str, x: f32, baseline: f32, font: f32) -> Rect {
    // Allow for the host's text metrics and antialiasing.
    let width = label.chars().count() as f32 * 0.62 * font;
    Rect::centered(x, baseline - font * 0.325, width, font * 1.35).padded(font * 0.2)
}

fn place_port_label(
    tick: (f32, f32),
    toward: f32,
    ticks: &LinkTicks,
    text: &str,
    font: f32,
    diagram: Rect,
    obstacles: &[Rect],
) -> Option<(f32, f32, Rect)> {
    let (tx, ty) = tick;
    let (nx, ny) = ticks.normal;
    let (ux, uy) = ticks.along;
    let bounds = port_label_bounds(text, 0.0, 0.0, font);
    let clearance = (nx.abs() * bounds.w + ny.abs() * bounds.h) * 0.5 + font * 0.8;

    // Stay near the tick, moving along the wire before moving farther away.
    // Either side of a vertical link can clear the pill above the packet.
    let sides: &[f32] = if ux == 0.0 { &[1.0, -1.0] } else { &[1.0] };
    let nudges = [0, 1, 2, 3, 4, 5, 6, -1, -2, -3, -4];
    let step = ticks.label_travel.min(font * 6.0) / 6.0;
    for offset in 0..=6 {
        for &side in sides {
            let away = side * (clearance + offset as f32 * font * 0.75);
            for nudge in nudges {
                let along = nudge as f32 * step * toward;
                let x = tx + nx * away + ux * along;
                let baseline = ty + ny * away + uy * along + font * 0.325;
                let bounds = port_label_bounds(text, x, baseline, font);
                if diagram.contains(bounds)
                    && !obstacles.iter().any(|obstacle| bounds.overlaps(*obstacle))
                {
                    return Some((x, baseline, bounds));
                }
            }
        }
    }
    None
}

fn packet_obstacles(packet: &Packet, short: f32) -> impl Iterator<Item = Rect> {
    let diameter = (packet.radius + packet_rim(short)) * 2.0;
    let dot = Rect::centered(packet.px, packet.py, diameter, diameter);
    std::iter::once(dot).chain(packet.plate)
}

fn draw_diagram(
    p: &mut Painter,
    input: &DrawInput,
    rect: Rect,
    active: Option<(&Event, f32)>,
    leased: bool,
    short: f32,
) {
    panel(p, rect, short);
    let centers = device_centers(input.topo, rect);
    let metrics = diagram_metrics(rect, short);
    let style = packet_style(input, rect, short);
    let mut obstacles = Vec::new();
    if input.labels == Labels::Full {
        obstacles.extend(
            input
                .topo
                .devices
                .iter()
                .zip(&centers)
                .map(|(device, &(cx, cy))| glyph_bounds(cx, cy, device.kind, metrics.icon, short)),
        );
    }
    let mut packets = Vec::new();
    if let Some((event, phase)) = active {
        let from = centers[usize::from(event.from)];
        for &leg in &event.legs {
            let to = centers[usize::from(leg)];
            let shape = packet_geometry(from, to, phase, event, &style);
            if input.labels == Labels::Full {
                obstacles.extend(packet_obstacles(&shape, short));
            }
            packets.push((from, to, shape));
        }
    }
    for link in input.topo.links {
        let (ax, ay) = centers[usize::from(link.a)];
        let (bx, by) = centers[usize::from(link.b)];
        let active_link = active.filter(|(event, _)| {
            (event.from == link.a && event.legs.contains(&link.b))
                || (event.from == link.b && event.legs.contains(&link.a))
        });
        let (width, color) = if let Some((event, _)) = active_link {
            (0.0035, Painter::with_alpha(proto_color(event.proto), 0.68))
        } else {
            (0.0021, LINK_COLOR)
        };
        p.line(ax, ay, bx, by, width * short, color);
        let ticks = link_ticks((ax, ay), (bx, by), metrics.icon);
        for (tick, text, toward) in [(ticks.a, link.a_port, 1.0), (ticks.b, link.b_port, -1.0)] {
            let (tx, ty) = tick;
            p.fill_round(
                tx - metrics.tick / 2.0,
                ty - metrics.tick / 2.0,
                metrics.tick,
                metrics.tick,
                metrics.tick * 0.25,
                color,
            );
            if input.labels == Labels::Full {
                let font = metrics.port_font;
                if let Some((x, baseline, bounds)) =
                    place_port_label(tick, toward, &ticks, text, font, rect, &obstacles)
                {
                    p.text_center(text, x, baseline, font, PORT_TEXT);
                    obstacles.push(bounds);
                }
            }
        }
    }

    if let Some((event, phase)) = active {
        if event.broadcast {
            let (cx, cy) = centers[usize::from(event.from)];
            let radius = lerp(0.58, 1.45, phase) * metrics.icon;
            p.dashed_circle(
                cx,
                cy,
                radius,
                short * 0.0026,
                Painter::with_alpha(proto_color(event.proto), (1.0 - phase) * 0.70),
            );
        }
    }

    for (i, device) in input.topo.devices.iter().enumerate() {
        let (cx, cy) = centers[i];
        let facing = input.topo.label_above[i];
        glyph(p, cx, cy, device.kind, metrics.icon, short, input.accent);
        let (name_y, ip_y) = if facing {
            let name_y = cy - metrics.icon * 0.58 - metrics.em * 0.7;
            (name_y, name_y - metrics.em * 1.13)
        } else {
            let name_y = cy + metrics.icon * 0.61 + metrics.em * 0.85;
            (name_y, name_y + metrics.em * 1.06)
        };
        p.text_center(device.name, cx, name_y, metrics.em, TEXT_BRIGHT);
        let address = if i == usize::from(CLIENT) && input.scenario == Scenario::Dhcp && !leased {
            "Awaiting lease"
        } else {
            device.ip
        };
        if input.labels != Labels::None {
            p.text_center(address, cx, ip_y, metrics.em * 0.78, TEXT_DIM);
        }
    }

    if let Some((event, phase)) = active {
        for (from, to, shape) in &packets {
            packet(p, *from, *to, phase, event.proto, &style, shape);
        }
    }
}

fn draw_table(
    p: &mut Painter,
    rect: Rect,
    title: &str,
    rows: &[TableRow],
    done: usize,
    em: f32,
    short: f32,
) {
    panel(p, rect, short);
    let pad = em * 0.9;
    let x = rect.x + pad;
    p.text_left(title, x, rect.y + pad + em * 0.78, em, TEXT_BRIGHT);
    let divider = rect.y + pad + em * 1.4;
    p.line(
        x,
        divider,
        rect.right() - pad,
        divider,
        short * 0.0014,
        PANEL_STROKE,
    );
    for (i, row) in rows.iter().enumerate() {
        let y = divider + em * (1.28 + i as f32 * 1.25);
        let highlighted = row.at.is_some_and(|at| at + 1 == done);
        let left_x = x + em * 0.55;
        if highlighted {
            p.fill_circle(x + em * 0.10, y - em * 0.35, em * 0.13, TEXT_BRIGHT);
        }
        let font = em * 0.90;
        let split = rect.x + rect.w * 0.43;
        let max_left = if row.right.is_empty() {
            rect.right() - pad
        } else {
            split - pad
        };
        text_fit(
            p,
            &row.left,
            left_x,
            y,
            font,
            max_left - left_x,
            TEXT_BRIGHT,
        );
        text_fit(
            p,
            &row.right,
            split,
            y,
            font,
            rect.right() - pad - split,
            TEXT_DIM,
        );
    }
}

fn draw_steps(
    p: &mut Painter,
    input: &DrawInput,
    rect: Rect,
    done: usize,
    active: Option<usize>,
    em: f32,
    short: f32,
) {
    panel(p, rect, short);
    let pad = em * 0.88;
    let x = rect.x + pad;
    let divider = rect.y + pad + em * 1.65;
    let row_h = em * 2.48;
    let count = input.events.len();
    let selected = active.unwrap_or_else(|| done.saturating_sub(1));
    let visible = (((rect.bottom() - divider - pad) / row_h) as usize).clamp(1, count);
    let start = selected.saturating_sub(visible / 2).min(count - visible);
    let heading = if done == count {
        format!("{count}/{count} steps shown")
    } else if active.is_none() {
        format!("{count} steps · Ready")
    } else {
        format!("Step {} of {count}", selected + 1)
    };
    p.text_left(&heading, x, rect.y + pad + em * 0.83, em, TEXT_BRIGHT);
    p.line(
        x,
        divider,
        rect.right() - pad,
        divider,
        short * 0.0014,
        PANEL_STROKE,
    );

    let col = x + em * 1.85;
    let max_width = rect.right() - pad - col;
    for (i, event) in input.events.iter().enumerate().skip(start).take(visible) {
        let y = divider + em * 0.28 + (i - start) as f32 * row_h;
        let current = active == Some(i);
        if current {
            p.fill_round(
                rect.x + pad * 0.4,
                y,
                rect.w - pad * 0.8,
                row_h,
                em * 0.34,
                Painter::with_alpha(input.accent, 0.15),
            );
            p.fill_round(
                rect.x + pad * 0.4,
                y + em * 0.36,
                em * 0.14,
                row_h - em * 0.72,
                em * 0.07,
                input.accent,
            );
        }
        let label = if input.labels == Labels::Brief {
            event.proto.short()
        } else {
            event.proto.full()
        };
        let title = if let Some(ttl) = event.ttl {
            format!("{label} · TTL {ttl}")
        } else {
            label.into()
        };
        let baseline = y + em * 1.05;
        let event_color = if i < done { TEXT_DIM } else { TEXT_BRIGHT };
        p.text_left(&(i + 1).to_string(), x, baseline, em * 0.80, event_color);
        text_fit(p, &title, col, baseline, em * 0.95, max_width, event_color);
        let devices = input.topo.devices;
        let peer_names = event
            .legs
            .iter()
            .map(|&leg| devices[usize::from(leg)].name)
            .collect::<Vec<_>>()
            .join(", ");
        let path = format!("{} → {peer_names}", devices[usize::from(event.from)].name);
        let note = if input.labels == Labels::Full && !event.note.is_empty() {
            &event.note
        } else {
            &path
        };
        text_fit(p, note, col, y + em * 2.04, em * 0.76, max_width, TEXT_DIM);
    }
}

struct Layout {
    area: Rect,
    diagram: Rect,
    step_w: f32,
    step_h: f32,
    table_h: f32,
    em: f32,
    margin: f32,
    gap: f32,
}

fn layout(input: &DrawInput, w: f32, h: f32) -> Layout {
    let short = w.min(h);
    let em = short * 0.026;
    let margin = short * 0.035;
    let gap = short * 0.021;
    let title_h = em * 3.2;
    let area = Rect {
        x: margin,
        y: margin + title_h,
        w: w - 2.0 * margin,
        h: h - 2.0 * margin - title_h,
    };
    let beside = w / h >= 1.24;
    let step_w = if beside && input.show_step_list {
        area.w * 0.34
    } else {
        0.0
    };
    let step_h = if !beside && input.show_step_list {
        area.h * 0.44
    } else {
        0.0
    };
    let table_h = if input.show_tables { em * 7.65 } else { 0.0 };
    let left_w = area.w - if step_w > 0.0 { step_w + gap } else { 0.0 };
    let diagram_h = area.h
        - if step_h > 0.0 { step_h + gap } else { 0.0 }
        - if table_h > 0.0 { table_h + gap } else { 0.0 };
    Layout {
        area,
        diagram: Rect {
            w: left_w,
            h: diagram_h,
            ..area
        },
        step_w,
        step_h,
        table_h,
        em,
        margin,
        gap,
    }
}

pub fn draw(input: &DrawInput, size: &Size) -> Scene {
    let (w, h) = (size.width as f32, size.height as f32);
    let short = w.min(h);
    let layout = layout(input, w, h);
    let em = layout.em;
    let margin = layout.margin;
    let gap = layout.gap;
    let done = timeline::done(input.u);
    let playing = timeline::playing(input.u, input.events.len());
    let active = playing
        .zip(timeline::phase(input.u, input.events.len()))
        .map(|(i, phase)| (&input.events[i], phase));
    let state = super::tables::replay(input.events, done);
    let (table_title, rows) = table_rows(input, &state);

    let mut p = Painter::new(w);

    p.fill_round(
        0.0,
        0.0,
        w,
        h,
        0.0,
        Color {
            r: 12,
            g: 18,
            b: 27,
            a: 255,
        },
    );
    p.text_left(
        input.scenario.title(),
        margin,
        margin + em * 1.47,
        em * 1.7,
        TEXT_BRIGHT,
    );
    text_fit(
        &mut p,
        input.topo.label,
        margin,
        margin + em * 2.72,
        em * 0.90,
        layout.area.w,
        TEXT_DIM,
    );
    let leased = input
        .events
        .get(done.saturating_sub(1))
        .is_some_and(|event| {
            done > 0 && event.proto == Proto::DhcpAck && event.legs.contains(&CLIENT)
        });
    draw_diagram(&mut p, input, layout.diagram, active, leased, short);
    if input.show_tables {
        let rect = Rect {
            y: layout.diagram.bottom() + gap,
            h: layout.table_h,
            ..layout.diagram
        };
        draw_table(&mut p, rect, table_title, &rows, done, em, short);
    }
    if input.show_step_list {
        let rect = if layout.step_w > 0.0 {
            Rect {
                x: layout.diagram.right() + gap,
                w: layout.step_w,
                ..layout.area
            }
        } else {
            Rect {
                y: layout.area.bottom() - layout.step_h,
                h: layout.step_h,
                ..layout.area
            }
        };
        draw_steps(&mut p, input, rect, done, playing, em, short);
    }
    Scene {
        coordinate_space: CoordinateSpace::Canvas,
        elements: p.elements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{build, SCENARIOS};
    use crate::topology::topology;

    #[test]
    fn port_labels_clear_packets_and_devices() {
        for routed in [false, true] {
            for name in SCENARIOS {
                let scenario = Scenario::parse(name).unwrap();
                let topo = topology(routed, scenario == Scenario::Dns);
                let events = build(scenario, topo);
                let mut input = DrawInput {
                    topo,
                    events: &events,
                    scenario,
                    u: 0.0,
                    labels: Labels::Full,
                    table_kind: TableKind::Auto,
                    show_tables: true,
                    show_step_list: true,
                    speed_lines: true,
                    packet_percent: 2.4,
                    accent: crate::ACCENT_DEFAULT,
                };
                for (width, height) in [(1280, 720), (320, 180), (480, 720), (900, 900)] {
                    let size = Size { width, height };
                    for (tables, steps) in
                        [(true, true), (true, false), (false, true), (false, false)]
                    {
                        input.show_tables = tables;
                        input.show_step_list = steps;
                        for packet_percent in [2.4, 6.0] {
                            input.packet_percent = packet_percent;
                            check_port_labels(&mut input, &size);
                        }
                    }
                }
            }
        }
    }

    fn check_port_labels(input: &mut DrawInput, size: &Size) {
        let topo = input.topo;
        let (w, h) = (size.width as f32, size.height as f32);
        let short = w.min(h);
        let diagram = layout(input, w, h).diagram;
        let centers = device_centers(topo, diagram);
        let icon = diagram_metrics(diagram, short).icon;
        let devices: Vec<_> = topo
            .devices
            .iter()
            .zip(&centers)
            .map(|(device, &(x, y))| glyph_bounds(x, y, device.kind, icon, short))
            .collect();
        let style = packet_style(input, diagram, short);

        // Include both holds and sample every hop, including each flood.
        for sample in -1..=input.events.len() as i32 * 8 {
            input.u = sample as f32 / 8.0;
            let context = || {
                format!(
                    "{:?} on {} at {w}x{h}, tables {}, steps {}, packet size {}, timeline position {}",
                    input.scenario, topo.label, input.show_tables, input.show_step_list,
                    input.packet_percent, input.u,
                )
            };
            let mut obstacles = devices.clone();
            if let Some(index) = timeline::playing(input.u, input.events.len()) {
                let event = &input.events[index];
                let from = centers[usize::from(event.from)];
                let phase = timeline::phase(input.u, input.events.len()).unwrap();
                for &leg in &event.legs {
                    let to = centers[usize::from(leg)];
                    let shape = packet_geometry(from, to, phase, event, &style);
                    obstacles.extend(packet_obstacles(&shape, short));
                }
            }

            let mut expected = topo.links.len() * 2;
            if obstacles.len() > devices.len() {
                // A moving pill may leave no clear spot close to the port.
                expected -= 2;
            }
            let mut count = 0;
            for element in draw(input, size).elements {
                let DrawCommand::TextRun(run) = element else {
                    continue;
                };
                if (run.color.r, run.color.g, run.color.b)
                    != (PORT_TEXT.r, PORT_TEXT.g, PORT_TEXT.b)
                {
                    continue;
                }
                count += 1;
                let bounds = port_label_bounds(
                    &run.text,
                    run.x + w * 0.5,
                    run.y + run.font_size * 0.93,
                    run.font_size,
                );
                for obstacle in &obstacles {
                    // Recovering the anchor from the text run can round at the edge.
                    assert!(
                        !bounds.overlaps(obstacle.padded(-0.0001)),
                        "{}: {} {bounds:?} overlaps {obstacle:?}",
                        context(),
                        run.text,
                    );
                }
                obstacles.push(bounds);
            }
            assert!(count >= expected, "{}: only {count} port labels", context());
        }
    }
}
