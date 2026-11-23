//! Nearest-hit tracing through a convex prism or a slab with parallel faces.
//! At each boundary, transmission switches media and TIR keeps the ray in
//! the current medium. The caller assigns each ray a share of the scene budget.

use crate::geom::{self, V};

pub const MAX_DEPTH: usize = 8;
pub const MAX_SEGMENTS: usize = 256;

pub struct Surface {
    pub a: V,
    pub b: V,
}

#[derive(Debug, PartialEq)]
pub struct Trace {
    pub hits: Vec<V>,
    /// Direction in the exterior after the last hit, or the current direction
    /// if no further face was reached inside the visible region.
    pub onward: Option<V>,
    pub inside: bool,
}

pub fn trace(
    origin: V,
    direction: V,
    surfaces: &[Surface],
    outside_index: f64,
    inside_index: f64,
    segment_budget: usize,
) -> Trace {
    let mut pos = origin;
    let mut dir = direction;
    let mut inside = false;
    let mut hits = Vec::new();
    // Reserve room for the final segment off the element.
    for _ in 0..segment_budget.saturating_sub(1).min(MAX_DEPTH) {
        let hits_ahead = surfaces.iter().filter_map(move |surface| {
            let t = pos.ray_segment_hit(dir, surface.a, surface.b)?;
            Some((t, surface))
        });
        let nearest = hits_ahead.clone().min_by(|(a, _), (b, _)| a.total_cmp(b));
        let Some((t, surface)) = nearest else {
            return Trace {
                hits,
                onward: Some(dir),
                inside,
            };
        };
        pos = pos + dir * t;
        hits.push(pos);
        // At a shared vertex there is no unique surface normal.
        if hits_ahead
            .filter(|(other, _)| (t - other).abs() <= 1e-9 * t.max(1.0))
            .count()
            > 1
        {
            break;
        }
        let normal = (surface.b - surface.a).unit().perp();
        let (from, to) = if inside {
            (inside_index, outside_index)
        } else {
            (outside_index, inside_index)
        };
        if let Some(transmitted) = geom::refract(dir, normal, from, to) {
            dir = transmitted;
            inside = !inside;
        } else {
            dir = geom::reflect(dir, normal);
        }
        if !inside {
            return Trace {
                hits,
                onward: Some(dir),
                inside,
            };
        }
    }
    // An unfinished internal path stops at its last actual interaction.
    Trace {
        hits,
        onward: None,
        inside,
    }
}
