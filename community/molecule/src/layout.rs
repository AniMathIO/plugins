//! Fit uniformly scaled depiction bounds into the canvas.
use crate::molecule::BOND_LEN_UNITS;
use crate::Size;

pub type Pt = (f32, f32);

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: Pt,
    pub max: Pt,
}

impl Bounds {
    pub fn around(center: Pt, half_w: f32, half_h: f32) -> Self {
        Self {
            min: (center.0 - half_w, center.1 - half_h),
            max: (center.0 + half_w, center.1 + half_h),
        }
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: (self.min.0.min(other.min.0), self.min.1.min(other.min.1)),
            max: (self.max.0.max(other.max.0), self.max.1.max(other.max.1)),
        }
    }

    fn width(self) -> f32 {
        self.max.0 - self.min.0
    }

    fn height(self) -> f32 {
        self.max.1 - self.min.1
    }

    fn center(self) -> Pt {
        (
            (self.min.0 + self.max.0) * 0.5,
            (self.min.1 + self.max.1) * 0.5,
        )
    }
}

#[derive(Clone, Copy)]
pub struct Transform {
    pub scale: f32,
    offset: Pt,
}

impl Transform {
    fn centered(bounds: Bounds, center: Pt, scale: f32) -> Self {
        let (x, y) = bounds.center();
        Self {
            scale,
            offset: (center.0 - x * scale, center.1 - y * scale),
        }
    }

    pub fn apply(self, (x, y): Pt) -> Pt {
        (
            x * self.scale + self.offset.0,
            y * self.scale + self.offset.1,
        )
    }

    pub fn bond_px(self) -> f32 {
        self.scale * BOND_LEN_UNITS as f32
    }
}

pub fn fit_scale(width: f32, height: f32, box_w: f32, box_h: f32) -> f32 {
    (box_w / width.max(1.0)).min(box_h / height.max(1.0))
}

pub fn structure_layout(bounds: Bounds, size: Size) -> Transform {
    let (w, h) = (size.width as f32, size.height as f32);
    let scale = fit_scale(bounds.width(), bounds.height(), w * 0.84, h * 0.80)
        .min(w.min(h) * 0.31 / BOND_LEN_UNITS as f32);
    Transform::centered(bounds, (w * 0.5, h * 0.5), scale)
}

pub struct ReactionSide {
    pub fragments: Vec<Transform>,
    pub pluses: Vec<Pt>,
}

pub struct ReactionLayout {
    pub left: ReactionSide,
    pub right: ReactionSide,
    pub arrow: (Pt, Pt),
    pub bond_px: f32,
}

/// Each fragment gets its own fitted ink box. Both sides share one scale,
/// and the signs sit in the actual gaps between adjacent boxes.
pub fn reaction_layout(reactants: &[Bounds], products: &[Bounds], size: Size) -> ReactionLayout {
    const BOND: f32 = BOND_LEN_UNITS as f32;
    const PLUS_GAP: f32 = BOND * 0.64;
    const ARROW_GAP: f32 = BOND * 2.5;
    let (w, h) = (size.width as f32, size.height as f32);
    let axis = h * 0.5;
    let side_width = |side: &[Bounds]| {
        side.iter().map(|b| b.width()).sum::<f32>() + PLUS_GAP * side.len().saturating_sub(1) as f32
    };
    let total_width = side_width(reactants) + ARROW_GAP + side_width(products);
    let height = reactants
        .iter()
        .chain(products)
        .map(|b| b.height())
        .fold(BOND, f32::max);
    let scale = fit_scale(total_width, height, w * 0.87, h * 0.78).min(w.min(h) * 0.20 / BOND);
    let place = |side: &[Bounds], mut x: f32| {
        let mut fragments = Vec::with_capacity(side.len());
        let mut pluses = Vec::with_capacity(side.len().saturating_sub(1));
        for (index, &bounds) in side.iter().enumerate() {
            if index > 0 {
                pluses.push((x + PLUS_GAP * scale * 0.5, axis));
                x += PLUS_GAP * scale;
            }
            let width = bounds.width() * scale;
            fragments.push(Transform::centered(bounds, (x + width * 0.5, axis), scale));
            x += width;
        }
        ReactionSide { fragments, pluses }
    };
    let start = (w - total_width * scale) * 0.5;
    let arrow_x = start + side_width(reactants) * scale;
    let arrow_gap = ARROW_GAP * scale;
    ReactionLayout {
        left: place(reactants, start),
        right: place(products, arrow_x + arrow_gap),
        arrow: (
            (arrow_x + arrow_gap * 0.12, axis),
            (arrow_x + arrow_gap * 0.88, axis),
        ),
        bond_px: scale * BOND,
    }
}
