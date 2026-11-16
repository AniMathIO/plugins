use super::{Color, DrawCommand, PathVerb, Point, Size};
use ttf_parser::{Face, OutlineBuilder};

pub fn color(c: syntect::highlighting::Color) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}
pub fn blend(a: Color, b: Color, t: f32) -> Color {
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: channel(a.a, b.a),
    }
}
pub struct Painter<'a> {
    pub elements: Vec<DrawCommand>,
    font: Face<'a>,
    scale: f32,
    advance: f32,
    line_height: f32,
    x: f32,
    y: f32,
    panel: (f32, f32, f32, f32),
}
impl Painter<'static> {
    pub fn new(
        size: Size,
        font_size: f32,
        padding: f32,
        columns: usize,
        lines: usize,
        gutter: usize,
    ) -> Self {
        let font = Face::parse(include_bytes!("../assets/JetBrainsMono-Regular.ttf"), 0)
            .expect("embedded font");
        let units = font.units_per_em() as f32;
        let advance = font
            .glyph_hor_advance(font.glyph_index('M').unwrap())
            .unwrap() as f32
            / units;
        let width = font_size * advance * columns.max(1) as f32 + padding * 2.0;
        let height = font_size * 1.5 * lines as f32 + padding * 2.0;
        // Fit both states as one stable box: no jump as progress changes.
        let fit = (size.width as f32 * 0.92 / width)
            .min(size.height as f32 * 0.92 / height)
            .min(1.0);
        let width = width * fit;
        let height = height * fit;
        let left = (size.width as f32 - width) * 0.5;
        let top = (size.height as f32 - height) * 0.5;
        let scale = font_size * fit / units;
        let advance = advance * font_size * fit;
        Self {
            elements: vec![],
            font,
            scale,
            advance,
            line_height: font_size * fit * 1.5,
            x: left + padding * fit + gutter as f32 * advance,
            y: top + padding * fit + font_size * fit * 1.1,
            panel: (left, top, width, height),
        }
    }
    pub fn panel(&mut self, radius: f32, color: Color) {
        let (x, y, w, h) = self.panel;
        let r = radius.min(w / 2.0).min(h / 2.0);
        let k = 0.552_284_8;
        let p = |x, y| Point { x, y };
        let path = vec![
            PathVerb::MoveTo(p(x + r, y)),
            PathVerb::LineTo(p(x + w - r, y)),
            PathVerb::CubicTo((
                p(x + w - r + k * r, y),
                p(x + w, y + r - k * r),
                p(x + w, y + r),
            )),
            PathVerb::LineTo(p(x + w, y + h - r)),
            PathVerb::CubicTo((
                p(x + w, y + h - r + k * r),
                p(x + w - r + k * r, y + h),
                p(x + w - r, y + h),
            )),
            PathVerb::LineTo(p(x + r, y + h)),
            PathVerb::CubicTo((
                p(x + r - k * r, y + h),
                p(x, y + h - r + k * r),
                p(x, y + h - r),
            )),
            PathVerb::LineTo(p(x, y + r)),
            PathVerb::CubicTo((p(x, y + r - k * r), p(x + r - k * r, y), p(x + r, y))),
            PathVerb::Close,
        ];
        self.elements.push(DrawCommand::FillPath((path, color)));
    }
    pub fn text(&mut self, text: &str, column: f32, line: f32, mut color: Color, opacity: f32) {
        color.a = (color.a as f32 * opacity.clamp(0.0, 1.0)).round() as u8;
        if color.a == 0 {
            return;
        }
        for (index, c) in text.chars().enumerate() {
            let glyph = self
                .font
                .glyph_index(c)
                .or_else(|| self.font.glyph_index('\u{fffd}'))
                .unwrap();
            let mut outline = Outline {
                path: vec![],
                scale: self.scale,
                x: self.x + (column + index as f32) * self.advance,
                y: self.y + line * self.line_height,
            };
            self.font.outline_glyph(glyph, &mut outline);
            if !outline.path.is_empty() {
                self.elements
                    .push(DrawCommand::FillPath((outline.path, color)));
            }
        }
    }
}
struct Outline {
    path: Vec<PathVerb>,
    scale: f32,
    x: f32,
    y: f32,
}
impl Outline {
    fn point(&self, x: f32, y: f32) -> Point {
        Point {
            x: self.x + x * self.scale,
            y: self.y - y * self.scale,
        }
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.push(PathVerb::MoveTo(self.point(x, y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.push(PathVerb::LineTo(self.point(x, y)));
    }
    fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
        self.path
            .push(PathVerb::QuadTo((self.point(x, y), self.point(x1, y1))));
    }
    fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.path.push(PathVerb::CubicTo((
            self.point(x, y),
            self.point(x1, y1),
            self.point(x2, y2),
        )));
    }
    fn close(&mut self) {
        self.path.push(PathVerb::Close);
    }
}
