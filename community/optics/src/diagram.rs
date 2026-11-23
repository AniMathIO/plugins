use std::f64::consts::FRAC_PI_2;

use crate::draw::{Painter, Palette, Rgba};
use crate::geom::{self, conjugates, eased, fmt, partial_polyline, V};
use crate::spectrum;
use crate::trace::{self, Surface};

const OUTLINE: Rgba = Rgba::new(215, 222, 235, 220);

fn phase(progress: f64, start: f64, end: f64) -> f64 {
    eased((progress - start) / (end - start))
}

pub struct Diagram<'a> {
    pub painter: &'a mut Painter,
    pub palette: &'a Palette,
    pub progress: f64,
    pub unit: f64,
    pub half_width: f64,
    pub half_height: f64,
    pub labels: bool,
    pub normal: bool,
    pub focal: bool,
}

#[derive(Debug)]
pub struct PrincipalRay {
    pub hit: V,
    pub outgoing: V,
}

/// Object to the left, y up. `side` is +1 for transmission and -1 for
/// reflection. Slopes are measured along the outgoing propagation axis.
pub fn principal_rays(
    f: f64,
    distance: f64,
    height: f64,
    side: f64,
    image_height: Option<f64>,
) -> [Option<PrincipalRay>; 3] {
    let ray = |y, slope| PrincipalRay {
        hit: V::new(0.0, y),
        outgoing: V::new(side, slope).unit(),
    };
    [
        Some(ray(height, -height / f)),
        Some(ray(0.0, -height / distance)),
        image_height.map(|y| ray(y, 0.0)),
    ]
}

/// Equilateral prism, with the incident ray aimed at the middle of its left
/// face. Positive incidence rotates the inward normal toward the apex.
pub fn prism_beam(unit: f64, angle: f64) -> ([V; 3], V, V) {
    let side = 0.45 * unit;
    let height = side * 3.0_f64.sqrt() / 2.0;
    let center = V::new(-0.08 * unit, 0.0);
    let corners = [
        center + V::new(0.0, height * 2.0 / 3.0),
        center + V::new(-side / 2.0, -height / 3.0),
        center + V::new(side / 2.0, -height / 3.0),
    ];
    let normal = (corners[1] - corners[0]).unit().perp();
    let (sin, cos) = angle.sin_cos();
    let incident = normal * cos + normal.perp() * sin;
    (corners, corners[0].lerp(corners[1], 0.5), incident)
}

/// Shrink a magnified principal-ray construction to the diagram bounds by
/// scaling the focal length, the distances and the heights together. The
/// thin-lens/mirror equations are scale invariant, so the magnification and
/// every angle stay the same while the image arrow and the third (focal or
/// centre) ray, which hit the element at m·height, come back on screen. This
/// is what keeps concave-mirror setups between C and F, or inside F, showing
/// their (strongly magnified) image instead of dropping it.
fn conjugate_fit_divisor(image: Option<V>, unit: f64, half_width: f64) -> f64 {
    let Some(point) = image else {
        return 1.0;
    };
    // 98% of the image-arrow guard in `principal`, so the scaled result can
    // never sit exactly on the cut-off; the ray budget is the 0.36·unit
    // aperture with its own margin.
    (point.x.abs() / (0.98 * (half_width - 0.075 * unit)))
        .max(point.y.abs() / (0.98 * 0.39 * unit))
        .max(point.y.abs() / (0.85 * 0.36 * unit))
        .max(1.0)
}

impl Diagram<'_> {
    fn font(&self) -> f64 {
        (0.031 * self.unit).max(1.0)
    }

    fn width(&self) -> f64 {
        (0.004 * self.unit).max(0.6)
    }

    fn guide(&mut self, from: V, to: V, color: Rgba) {
        self.painter.dashed(
            from,
            to,
            self.width() * 0.44,
            0.010 * self.unit,
            0.008 * self.unit,
            color,
        );
    }

    fn label(&mut self, text: &str, at: V, opacity: f64) {
        if self.labels {
            self.painter.label(
                text,
                at,
                self.font(),
                self.palette.label.with_alpha(opacity),
            );
        }
    }

    /// Point along a ray at the edge of the padded diagram bounds.
    fn edge(&self, pos: V, dir: V, margin: f64) -> V {
        fn limit(x: f64, dx: f64, half: f64) -> f64 {
            if dx.abs() < 1e-12 {
                f64::INFINITY
            } else {
                (half.copysign(dx) - x) / dx
            }
        }
        let distance = limit(pos.x, dir.x, self.half_width - margin).min(limit(
            pos.y,
            dir.y,
            self.half_height.min(self.unit * 0.50) - margin,
        ));
        pos + dir * distance.max(0.0)
    }

    fn arrow(&mut self, a: V, b: V, fraction: f64, color: Rgba) {
        let length = (b - a).length();
        if length >= 0.045 * self.unit {
            self.painter
                .arrow_head(a.lerp(b, fraction), b - a, 0.014 * self.unit, color);
        }
    }

    fn grow_ray(&mut self, points: &[V], growth: f64, color: Rgba, arrows: bool) {
        let width = self.width();
        self.painter
            .stroke(&partial_polyline(points, growth), width, color);
        if arrows && growth >= 1.0 {
            for leg in points.windows(2) {
                self.arrow(leg[0], leg[1], 0.55, color);
            }
        }
    }

    pub fn principal(&mut self, f: f64, distance: f64, object_height: f64, mirror: bool) {
        let u = self.unit;
        let side = if mirror { -1.0 } else { 1.0 };
        let distance = distance.min(self.half_width - 0.10 * u);
        // Classify the conjugate at the original scale, before fitting can
        // shrink the focal gap below the near-infinity tolerance.
        let image = conjugates(f, distance).map(|(d, m)| V::new(side * d, m * object_height));
        let fit = conjugate_fit_divisor(image, u, self.half_width);
        let (f, distance, object_height) = (f / fit, distance / fit, object_height / fit);
        let object = V::new(-distance, 0.0);
        let tip = V::new(-distance, object_height);
        let image = image.map(|point| point / fit);
        let aperture = 0.36 * u;
        let rays = principal_rays(f, distance, object_height, side, image.map(|p| p.y));
        let height = rays
            .iter()
            .flatten()
            .filter(|r| r.hit.y.abs() <= aperture)
            .map(|r| r.hit.y.abs())
            .fold(0.19 * u, f64::max)
            + 0.025 * u;
        let color = self.palette.ray;
        let object_color = self.palette.object;
        let image_color = self.palette.image;
        let progress = self.progress;
        let labels = phase(progress, 0.74, 1.0);
        let name = match (mirror, f > 0.0) {
            (true, true) => "Concave mirror",
            (true, false) => "Convex mirror",
            (false, true) => "Converging lens",
            (false, false) => "Diverging lens",
        };

        self.guide(
            V::new(-self.half_width, 0.0),
            V::new(self.half_width, 0.0),
            self.palette.construction,
        );
        self.element(height, f > 0.0, mirror);
        self.painter
            .arrow_line(object, tip, self.width(), 0.022 * u, object_color);
        self.label(name, V::new(0.0, 0.452 * u), labels);
        self.label("Object", tip + V::new(-0.040 * u, 0.051 * u), labels);

        if self.focal {
            let mut marker = |x: f64, name: &str| {
                if x.abs() <= self.half_width - 0.05 * u {
                    self.painter
                        .dot(V::new(x, 0.0), 0.005 * u, self.palette.marker);
                    self.label(name, V::new(x, -0.048 * u), labels);
                }
            };
            if mirror {
                marker(-f, "F");
                marker(-2.0 * f, "C");
            } else {
                marker(-f.abs(), "F");
                marker(f.abs(), "F′");
                marker(-2.0 * f.abs(), "2F");
                marker(2.0 * f.abs(), "2F′");
            }
        }

        // Draw each incident leg and its continuation as one growing ray.
        // Back-projections start only once the physical ray hits the element.
        for (ray, (start, finish)) in
            rays.into_iter()
                .zip([(0.02, 0.34), (0.19, 0.52), (0.36, 0.69)])
        {
            let Some(ray) = ray.filter(|r| r.hit.y.abs() <= aperture) else {
                continue;
            };
            let end = self.edge(ray.hit, ray.outgoing, 0.04 * u);
            let growth = phase(progress, start, finish);
            self.grow_ray(&[tip, ray.hit, end], growth, color, true);
            if let Some(point) = image.filter(|p| side * p.x < 0.0) {
                let incoming_length = (tip - ray.hit).length();
                let interaction = incoming_length / (incoming_length + (end - ray.hit).length());
                let limit = self.edge(ray.hit, -ray.outgoing, 0.035 * u);
                let end = if (point - ray.hit).length() < (limit - ray.hit).length() {
                    point
                } else {
                    limit
                };
                self.guide(
                    ray.hit,
                    ray.hit.lerp(end, phase(growth, interaction, 1.0)),
                    color.with_alpha(0.72),
                );
            }
        }

        match image {
            Some(point)
                if point.x.abs() <= self.half_width - 0.075 * u && point.y.abs() <= 0.39 * u =>
            {
                let real = side * point.x > 0.0;
                let growth = phase(progress, 0.71, 0.88);
                let base = V::new(point.x, 0.0);
                let end = base.lerp(point, growth);
                if growth > 0.0 {
                    if !real {
                        self.guide(base, end, image_color);
                    } else {
                        self.painter.stroke(&[base, end], self.width(), image_color);
                    }
                }
                if growth >= 1.0 && (point - base).length() > 0.002 * u {
                    self.painter.arrow_head(
                        point,
                        point - base,
                        (point.y.abs() * 0.45).min(0.022 * u),
                        image_color,
                    );
                }
                let (label, y) = if real {
                    ("Real image", point.y - 0.062 * u)
                } else if point.y < 0.09 * u && !mirror {
                    ("Virtual image", -0.114 * u)
                } else {
                    ("Virtual image", point.y + 0.050 * u)
                };
                let x = if real {
                    point.x - 0.105 * u * side
                } else {
                    point.x
                };
                let x = x.clamp(-self.half_width + 0.132 * u, self.half_width - 0.132 * u);
                self.label(label, V::new(x, y), labels);
            }
            // Defensive: fitting scales finite conjugates inside the
            // guard above, so this arm only catches a future regression.
            Some(point) => self.label(
                if side * point.x < 0.0 {
                    "Virtual image beyond frame"
                } else {
                    "Real image beyond frame"
                },
                V::new(0.0, -0.45 * u),
                labels,
            ),
            None => self.label("Image at infinity", V::new(0.0, -0.45 * u), labels),
        }
    }

    fn element(&mut self, height: f64, positive: bool, mirror: bool) {
        let u = self.unit;
        let width = self.width() * 0.56;
        if mirror {
            // The reflecting plane is the paraxial reference. The hatched
            // silhouette behind it distinguishes concave and convex elements.
            let profile = |t: f64| {
                let sag = if positive { 1.0 - t * t } else { t * t };
                V::new((0.004 + 0.030 * sag) * u, height * t)
            };
            let back: Vec<_> = (-16..=16).map(|i| profile(f64::from(i) / 16.0)).collect();
            let mut cross_section = vec![V::new(0.0, -height)];
            cross_section.extend(&back);
            cross_section.push(V::new(0.0, height));
            self.painter
                .fill_polygon(&cross_section, self.palette.glass);
            cross_section.push(cross_section[0]);
            self.painter.stroke(&cross_section, width, OUTLINE);
            for i in -5..=5 {
                let t = f64::from(i) / 5.0;
                let a = profile(t);
                self.painter.stroke(
                    &[a, a + V::new(0.018 * u, 0.009 * u)],
                    width,
                    self.palette.construction,
                );
            }
        } else {
            let profile = |i: i32| {
                let t = f64::from(i) / 16.0;
                let x = if positive {
                    0.041 * (1.0 - t * t)
                } else {
                    0.012 + 0.030 * t * t
                };
                V::new(x * u, height * t)
            };
            let mut outline: Vec<_> = (-16..=16).map(profile).collect();
            outline.extend((-16..=16).rev().map(|i| {
                let q = profile(i);
                V::new(-q.x, q.y)
            }));
            self.painter.fill_polygon(&outline, self.palette.glass);
            outline.push(outline[0]);
            self.painter.stroke(&outline, width, OUTLINE);
        }
    }

    fn slab(&mut self, top: f64, bottom: f64, lower_face: bool) {
        let x = self.half_width;
        let corners = [
            V::new(-x, top),
            V::new(x, top),
            V::new(x, bottom),
            V::new(-x, bottom),
        ];
        self.painter.fill_polygon(&corners, self.palette.glass);
        self.painter
            .stroke(&corners[..2], self.width() * 0.53, OUTLINE);
        if lower_face {
            self.painter
                .stroke(&corners[2..], self.width() * 0.53, OUTLINE);
        }
    }

    fn arc(&mut self, center: V, radius: f64, normal_angle: f64, angle: f64, growth: f64) {
        if growth > 0.0 && angle.abs() > 1e-6 {
            self.painter.arc(
                center,
                radius * self.unit,
                normal_angle,
                normal_angle + angle * growth,
                self.width() * 0.50,
                self.palette.marker,
            );
        }
    }

    pub fn refraction(&mut self, outside: f64, inside: f64, angle: f64) {
        let u = self.unit;
        let top = 0.12 * u;
        let bottom = -0.17 * u;
        let hit = V::new(-0.17 * u, top);
        let (sin, cos) = angle.sin_cos();
        let incident = V::new(sin, -cos);
        let source = self.edge(hit, -incident, 0.05 * u);
        let x = self.half_width;
        let surfaces = [
            Surface {
                a: V::new(-x, top),
                b: V::new(x, top),
            },
            Surface {
                a: V::new(-x, bottom),
                b: V::new(x, bottom),
            },
        ];
        let trace = trace::trace(source, incident, &surfaces, outside, inside, 4);
        self.slab(top, bottom, true);
        if self.normal {
            self.guide(
                hit + V::new(0.0, -0.195 * u),
                hit + V::new(0.0, 0.215 * u),
                self.palette.construction,
            );
        }
        let mut points = vec![source];
        points.extend(trace.hits);
        if let Some(dir) = trace.onward {
            let last = *points.last().unwrap();
            points.push(self.edge(last, dir, if trace.inside { 0.0 } else { 0.025 * u }));
        }
        self.grow_ray(
            &points,
            phase(self.progress, 0.02, 0.73),
            self.palette.ray,
            true,
        );

        let arcs = phase(self.progress, 0.50, 0.80);
        let labels = phase(self.progress, 0.72, 1.0);
        self.arc(hit, 0.084, FRAC_PI_2, angle, arcs);
        self.label(
            &format!("θ₁ = {}°", fmt(angle.to_degrees(), 1)),
            hit + V::new(0.125 * u, 0.16 * u),
            labels,
        );
        self.label(
            &format!("n₁ = {}", fmt(outside, 2)),
            V::new(0.31 * u, 0.25 * u),
            labels,
        );
        self.label(
            &format!("n₂ = {}", fmt(inside, 2)),
            V::new(-0.33 * u, -0.095 * u),
            labels,
        );
        if let Some(transmitted) = geom::snell_angles(angle, outside, inside) {
            self.arc(hit, 0.102, -FRAC_PI_2, transmitted, arcs);
            self.label(
                &format!("θ₂ = {}°", fmt(transmitted.to_degrees(), 1)),
                hit + V::new(0.25 * u, -0.19 * u),
                labels,
            );
            self.label("n₁ sin θ₁ = n₂ sin θ₂", V::new(0.03 * u, 0.415 * u), labels);
        } else {
            self.arc(hit, 0.102, FRAC_PI_2, -angle, arcs);
            self.label(
                "Total internal reflection",
                V::new(0.03 * u, 0.415 * u),
                labels,
            );
        }
    }

    pub fn total_internal_reflection(&mut self, outside: f64, inside: f64, max_angle: f64) {
        let u = self.unit;
        let hit = V::new(-0.055 * u, 0.0);
        let angle = max_angle * eased(self.progress);
        let (sin, cos) = angle.sin_cos();
        let incident = V::new(sin, cos);
        let reflected = geom::reflect(incident, V::new(0.0, 1.0));
        let transmitted = geom::refract(incident, V::new(0.0, 1.0), inside, outside);
        let ray = self.palette.ray;

        self.slab(0.0, -self.half_height, false);
        if self.normal {
            self.guide(
                hit - V::new(0.0, 0.28 * u),
                hit + V::new(0.0, 0.24 * u),
                self.palette.construction,
            );
        }
        let start = self.edge(hit, -incident, 0.05 * u);
        self.grow_ray(&[start, hit], phase(self.progress, 0.0, 0.30), ray, true);
        let branch = phase(self.progress, 0.30, 0.58);
        if let Some(dir) = transmitted {
            self.grow_ray(&[hit, self.edge(hit, dir, 0.05 * u)], branch, ray, true);
        }
        if inside != outside {
            let color = ray.with_alpha(if transmitted.is_some() { 0.35 } else { 1.0 });
            self.grow_ray(
                &[hit, self.edge(hit, reflected, 0.05 * u)],
                branch,
                color,
                true,
            );
        }
        let markers = phase(self.progress, 0.26, 0.51);
        self.arc(hit, 0.12, -FRAC_PI_2, -angle, markers);
        self.label(
            &format!("θ₁ = {}°", fmt(angle.to_degrees(), 1)),
            start.lerp(hit, 0.52) + incident.perp() * (0.10 * u),
            markers,
        );
        // The ray starts in the slab, so the incident medium n₁ is the lower
        // one — the same n₁ = incident-medium convention as the refraction
        // setup, which is why the two labels swap sides relative to `outside`
        // and `inside` there.
        self.label(
            &format!("n₁ = {}", fmt(inside, 2)),
            V::new(0.34 * u, -0.24 * u),
            markers,
        );
        self.label(
            &format!("n₂ = {}", fmt(outside, 2)),
            V::new(-0.33 * u, 0.28 * u),
            markers,
        );
        if let Some(critical) = geom::critical_angle(inside, outside) {
            let (s, c) = critical.sin_cos();
            self.guide(
                hit,
                hit - V::new(s, c) * (0.22 * u * markers),
                self.palette.marker.with_alpha(0.72),
            );
            // Above the slab and left of the hit, outside both ray sweeps.
            self.label(
                &format!("sin θc = n₂/n₁ → θc = {}°", fmt(critical.to_degrees(), 1)),
                V::new(-0.28 * u, 0.36 * u),
                markers,
            );
        }
        if transmitted.is_none() {
            self.label("Total internal reflection", V::new(0.0, 0.40 * u), markers);
        }
    }

    pub fn dispersion(&mut self, index: f64, angle: f64, count: usize, spectrum_on: bool) {
        let u = self.unit;
        let (corners, entry, incident) = prism_beam(u, angle);
        let [top, left, right] = corners;
        self.painter.fill_polygon(&corners, self.palette.glass);
        self.painter
            .stroke(&[top, left, right, top], self.width() * 0.52, OUTLINE);
        let surfaces = [
            Surface { a: top, b: left },
            Surface { a: left, b: right },
            Surface { a: right, b: top },
        ];
        let wavelengths = spectrum::sample_wavelengths(if spectrum_on { count } else { 1 });
        let budget = trace::MAX_SEGMENTS / wavelengths.len();
        let source = self.edge(entry, -incident, 0.045 * u);
        let white = self.palette.white_beam;
        self.grow_ray(
            &[source, entry],
            phase(self.progress, 0.02, 0.27),
            white,
            true,
        );

        let growth = phase(self.progress, 0.27, 0.91);
        let labels = phase(self.progress, 0.90, 1.0);
        let mut exits = Vec::new();
        for wavelength in wavelengths {
            let n = spectrum::index_at(index, wavelength);
            let trace = trace::trace(source, incident, &surfaces, 1.0, n, budget);
            let mut path = trace.hits;
            if let (Some(&last), Some(dir)) = (path.last(), trace.onward) {
                let end = self.edge(last, dir, 0.04 * u);
                path.push(end);
                if !trace.inside && path.len() >= 3 {
                    exits.push((wavelength, last, end, dir));
                }
            }
            let color = if spectrum_on {
                let (r, g, b) = spectrum::wavelength_rgb(wavelength);
                Rgba::new(r, g, b, 255)
            } else {
                self.palette.ray
            };
            self.grow_ray(&path, growth, color, !spectrum_on);
        }
        self.label(
            &format!("n (550 nm) = {}", fmt(index, 2)),
            V::new(0.0, 0.39 * u),
            labels,
        );
        if spectrum_on {
            if let [first, .., last] = exits.as_slice() {
                let (_, a, b, dir) = *first;
                let (_, c, d, _) = *last;
                // Label only separated exits; a tight fan would overlap the text.
                if (b - d).length() > 0.032 * u {
                    let side = (b - d).dot(dir.perp()).signum();
                    self.label(
                        &format!("{:.0} nm", first.0),
                        a.lerp(b, 0.72) + dir.perp() * (side * 0.047 * u),
                        labels,
                    );
                    self.label(
                        &format!("{:.0} nm", last.0),
                        c.lerp(d, 0.72) - dir.perp() * (side * 0.047 * u),
                        labels,
                    );
                }
            }
        }
    }
}
