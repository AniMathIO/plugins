//! Pure 2D geometry and optics math. All of it is textbook vector algebra; the
//! refraction/reflection conventions follow the tracing behaviour studied in
//! ricktu288/ray-optics (Apache-2.0; see NOTICE). Computed in a right-handed,
//! y-up math frame; the caller converts to canvas coordinates on emission.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// Finite 2D vector / point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V {
    pub x: f64,
    pub y: f64,
}

impl V {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn perp(self) -> Self {
        // Rotate 90 degrees counter-clockwise.
        Self::new(-self.y, self.x)
    }
    pub fn length2(self) -> f64 {
        self.dot(self)
    }
    pub fn length(self) -> f64 {
        self.length2().sqrt()
    }
    pub fn unit(self) -> Self {
        let len = self.length();
        if len > 1e-12 {
            self / len
        } else {
            Self::new(1.0, 0.0)
        }
    }
    pub fn lerp(self, o: Self, t: f64) -> Self {
        self + (o - self) * t
    }
    /// Smallest positive solution t with `self + dir * t` on segment `[a, b]`;
    /// `t` is along the ray, the hit point is `self + dir * t`.
    pub fn ray_segment_hit(self, dir: Self, a: Self, b: Self) -> Option<f64> {
        let e = b - a;
        let denom = dir.perp().dot(e);
        if denom.abs() < 1e-12 {
            return None;
        }
        let t = (a - self).perp().dot(e) / denom;
        let u = (a - self).perp().dot(dir) / denom;
        if t > 1e-9 && (-1e-9..=1.0 + 1e-9).contains(&u) {
            Some(t)
        } else {
            None
        }
    }
}

impl Add for V {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for V {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for V {
    type Output = Self;
    fn mul(self, k: f64) -> Self {
        Self::new(self.x * k, self.y * k)
    }
}
impl Div<f64> for V {
    type Output = Self;
    fn div(self, k: f64) -> Self {
        Self::new(self.x / k, self.y / k)
    }
}
impl Neg for V {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

/// Intersection of the lines `(a1, a2)` and `(b1, b2)`, if not parallel.
#[cfg(test)]
pub fn lines_intersection(a1: V, a2: V, b1: V, b2: V) -> Option<V> {
    let e = a2 - a1;
    let f = b2 - b1;
    let denom = e.perp().dot(f);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (b1 - a1).perp().dot(f) / denom;
    Some(a1 + e * t)
}

/// Mirror reflection of unit direction `d` about unit normal `n`.
pub fn reflect(d: V, n: V) -> V {
    d - n * (2.0 * d.dot(n))
}

/// Refraction of unit direction `d` across a surface with unit normal `n`
/// oriented against the incident ray (`d.dot(n) < 0`), from medium `n1` into
/// medium `n2` (Snell's law). Returns `None` on total internal reflection.
/// Same convention as the ray-optics tracing engine: the normal is flipped to
/// face the incident side, so boundary orientation never matters.
pub fn refract(d: V, n: V, n1: f64, n2: f64) -> Option<V> {
    if n1 == n2 {
        return Some(d);
    }
    let tangent = n.perp();
    let sin_t = (n1 / n2) * d.dot(tangent);
    if sin_t.abs() > 1.0 + 1e-12 {
        return None;
    }
    let sin_t = sin_t.clamp(-1.0, 1.0);
    let normal = if d.dot(n) > 0.0 { -n } else { n };
    Some(tangent * sin_t - normal * (1.0 - sin_t * sin_t).sqrt())
}

/// Snell angle pair in radians: `n1 * sin(i) = n2 * sin(t)`, `None` on TIR.
pub fn snell_angles(incident: f64, n1: f64, n2: f64) -> Option<f64> {
    let (sin, cos) = incident.sin_cos();
    refract(V::new(sin, -cos), V::new(0.0, 1.0), n1, n2).map(|ray| ray.x.asin())
}

/// Critical angle (radians) for light going from `n_dense` into `n_rare`,
/// `None` when no total internal reflection is possible.
pub fn critical_angle(n_dense: f64, n_rare: f64) -> Option<f64> {
    if n_dense <= n_rare {
        None
    } else {
        Some((n_rare / n_dense).asin())
    }
}

/// Thin lens / spherical mirror conjugate distances. `f` is the signed focal
/// length, `d_o` the object distance from the element. Returns
/// `(d_i, magnification)` from `1/f = 1/d_o + 1/d_i`, `m = -d_i / d_o`.
/// Positive `d_i` is a real image on the transmission (lens) / reflection
/// (mirror, in front) side; negative means a virtual image.
pub fn conjugates(f: f64, d_o: f64) -> Option<(f64, f64)> {
    if f.abs() < 1e-12 || (d_o - f).abs() < 1e-9 {
        return None; // Object at the focal plane images at infinity.
    }
    let d_i = f * d_o / (d_o - f);
    let m = -d_i / d_o;
    Some((d_i, m))
}

/// Total polyline length.
pub fn polyline_length(points: &[V]) -> f64 {
    points.windows(2).map(|w| (w[1] - w[0]).length()).sum()
}

/// Prefix of the polyline that covers fraction `t` of its total length. `t <= 0`
/// yields an empty vector, `t >= 1` the whole polyline; intermediate values cut
/// the crossing segment at exact length. This is what makes every ray grow with
/// the animation parameter.
pub fn partial_polyline(points: &[V], t: f64) -> Vec<V> {
    if points.len() < 2 || t <= 0.0 {
        return Vec::new();
    }
    if t >= 1.0 {
        return points.to_vec();
    }
    let total = polyline_length(points);
    let mut budget = total * t;
    let mut out = vec![points[0]];
    for w in points.windows(2) {
        let seg = (w[1] - w[0]).length();
        if seg <= 1e-12 {
            continue;
        }
        if budget >= seg {
            out.push(w[1]);
            budget -= seg;
        } else {
            out.push(w[0].lerp(w[1], budget / seg));
            break;
        }
    }
    out
}

/// Smooth 0..1 easing (smoothstep) with the input clamped.
pub fn eased(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Format a float for a diagram label: fixed decimals, ASCII only, `-0` cleaned.
pub fn fmt(value: f64, decimals: usize) -> String {
    let mut text = format!("{value:.decimals$}");
    if text == "-0"
        || text.starts_with("-0.") && text.trim_start_matches("-0.").parse::<f64>() == Ok(0.0)
    {
        text = text.trim_start_matches('-').to_string();
    }
    text
}
