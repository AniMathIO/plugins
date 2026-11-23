use super::*;
use crate::diagram;
use crate::geom::{self, V};

fn params(values: &[(&str, ParamValue)]) -> Vec<ParamEntry> {
    values
        .iter()
        .map(|(id, value)| ParamEntry {
            id: (*id).into(),
            value: value.clone(),
        })
        .collect()
}

fn scene(p: &[ParamEntry]) -> Scene {
    render(
        p,
        Size {
            width: 1280,
            height: 720,
        },
    )
    .unwrap()
}

fn texts(scene: &Scene) -> Vec<&str> {
    scene
        .elements
        .iter()
        .filter_map(|e| match e {
            DrawCommand::TextRun(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect()
}

/// Every emitted coordinate must be finite and stay on the canvas, including
/// the degenerate near-focus cases.
fn assert_finite_and_on_canvas(scene: &Scene, size: Size) {
    let (width, height) = (f64::from(size.width), f64::from(size.height));
    let ok = |x: f64, y: f64, what: &str| {
        assert!(
            x.is_finite() && y.is_finite(),
            "non-finite {what} ({x}, {y})"
        );
        assert!(
            x.abs() <= width * 1.01 && y.abs() <= height * 1.01,
            "off-canvas {what} ({x}, {y})"
        );
    };
    for element in &scene.elements {
        match element {
            DrawCommand::StrokePath((verbs, _)) | DrawCommand::FillPath((verbs, _)) => {
                for verb in verbs {
                    match verb {
                        PathVerb::MoveTo(p) | PathVerb::LineTo(p) => {
                            ok(f64::from(p.x), f64::from(p.y), "path point")
                        }
                        PathVerb::QuadTo((p, _)) | PathVerb::CubicTo((p, _, _)) => {
                            ok(f64::from(p.x), f64::from(p.y), "control point")
                        }
                        PathVerb::Close => {}
                    }
                }
            }
            DrawCommand::TextRun(t) => ok(f64::from(t.x), f64::from(t.y), "text"),
        }
    }
}

const TINY: f64 = 1e-9;

#[test]
fn snell_law_round_trip() {
    // Refraction direction obeys n1 sin(i) = n2 sin(t) across many pairs.
    for &(n1, n2) in &[
        (1.0, 1.5),
        (1.5, 1.0),
        (1.0, 2.42),
        (1.33, 1.5),
        (1.5, 1.33),
    ] {
        for deg in [0.0f64, 5.0, 17.0, 30.0, 45.0, 60.0, 80.0] {
            let i = deg.to_radians();
            let d = V::new(i.sin(), -i.cos());
            let n = V::new(0.0, 1.0);
            if let Some(t) = geom::refract(d, n, n1, n2) {
                let sin_i = i.sin();
                let sin_t = t.x; // unit vector: x component is the sine
                assert!(
                    (n1 * sin_i - n2 * sin_t).abs() < 1e-6,
                    "snell violated for {n1}->{n2} at {deg}deg"
                );
                // The ray crosses the surface: keeps heading against the normal.
                assert!(t.dot(n) < 0.0, "refracted ray must cross the surface");
            } else {
                // TIR only from denser to rarer, past the critical angle.
                let crit = geom::critical_angle(n1, n2).unwrap_or(f64::INFINITY);
                assert!(n1 > n2 && i > crit);
            }
        }
    }
}

#[test]
fn snell_angles_match_vector_refraction() {
    for deg in [1.0f64, 10.0, 33.0, 55.0] {
        let i = deg.to_radians();
        let vector = geom::refract(V::new(i.sin(), -i.cos()), V::new(0.0, 1.0), 1.0, 1.5).unwrap();
        let angle = geom::snell_angles(i, 1.0, 1.5).unwrap();
        assert!((vector.x - angle.sin()).abs() < 1e-9);
    }
    // 1.5 -> 1.0 past 41.8 degrees: TIR.
    assert!(geom::snell_angles(41.80f64.to_radians(), 1.5, 1.0).is_some());
    assert!(geom::snell_angles(41.82f64.to_radians(), 1.5, 1.0).is_none());
}

#[test]
fn total_internal_reflection_threshold_is_critical_angle() {
    let crit: f64 = (1.0f64 / 1.5).asin();
    assert!((geom::critical_angle(1.5, 1.0).unwrap() - crit).abs() < 1e-12);
    assert!(geom::critical_angle(1.0, 1.5).is_none());
    assert!(geom::critical_angle(1.5, 1.5).is_none());
    let below: f64 = 41.0f64.to_radians();
    let above = crit + 1e-7;
    let d_below = V::new(below.sin(), -below.cos());
    let d_at = V::new(crit.sin(), -crit.cos());
    let d_above = V::new(above.sin(), -above.cos());
    let n = V::new(0.0, 1.0);
    assert!(geom::refract(d_below, n, 1.5, 1.0).is_some());
    let grazing = geom::refract(d_at, n, 1.5, 1.0).expect("critical ray grazes the interface");
    assert!((grazing.x - 1.0).abs() < TINY && grazing.y.abs() < TINY);
    assert!(geom::refract(d_above, n, 1.5, 1.0).is_none());
    // The reflected ray keeps its magnitude and mirrors about the normal.
    let r = geom::reflect(d_above, n);
    assert!((r.length() - 1.0).abs() < 1e-12);
    assert!((r.x - d_above.x).abs() < 1e-12);
    assert!((r.y + d_above.y).abs() < 1e-12);
}

#[test]
fn thin_lens_equation_both_focal_signs() {
    for &f in &[200.0f64, -200.0, 144.0, -97.0] {
        for &d_o in &[100.0f64, 250.0, 400.0, 600.0] {
            if (d_o - f).abs() < TINY {
                continue;
            }
            let (d_i, m) = geom::conjugates(f, d_o).unwrap();
            // 1/f = 1/d_o + 1/d_i
            let lhs = 1.0 / f;
            let rhs = 1.0 / d_o + 1.0 / d_i;
            assert!(
                (lhs - rhs).abs() / lhs.abs() < 1e-9,
                "f={f} do={d_o} di={d_i}"
            );
            assert!((m - (-d_i / d_o)).abs() < 1e-12);
            // Converging with object beyond f gives a real, inverted image.
            if f > 0.0 && d_o > f {
                assert!(d_i > 0.0 && m < 0.0);
            }
            // Diverging always gives a virtual, upright image.
            if f < 0.0 {
                assert!(d_i < 0.0 && m > 0.0);
            }
        }
    }
    // Object at the focal plane: no finite image.
    assert!(geom::conjugates(150.0, 150.0).is_none());
}

#[test]
fn principal_rays_form_real_and_virtual_images_on_the_correct_side() {
    for side in [-1.0, 1.0] {
        for (f, d) in [(0.2, 0.35), (0.2, 0.12), (-0.2, 0.35)] {
            let height = 0.1;
            let (image_distance, magnification) = geom::conjugates(f, d).unwrap();
            let image = V::new(side * image_distance, magnification * height);
            let rays = diagram::principal_rays(f, d, height, side);
            for ray in rays.iter().flatten() {
                assert!(ray.outgoing.x * side > 0.0);
                let to_image = image - ray.hit;
                assert!(to_image.dot(ray.outgoing.perp()).abs() < TINY);
                assert_eq!(to_image.dot(ray.outgoing).signum(), image_distance.signum());
            }
            let [parallel, vertex, focal] = rays.map(Option::unwrap);
            let focus = V::new(side * f, 0.0);
            assert!((focus - parallel.hit).dot(parallel.outgoing.perp()).abs() < TINY);
            let tip = V::new(-d, height);
            if side > 0.0 {
                assert!((vertex.hit - tip).unit().dot(vertex.outgoing) > 1.0 - TINY);
            } else {
                let reflected = geom::reflect((vertex.hit - tip).unit(), V::new(-1.0, 0.0));
                assert!((reflected - vertex.outgoing).length() < TINY);
            }
            let incident_focus = V::new(-f, 0.0);
            assert!(
                (incident_focus - tip)
                    .dot((focal.hit - tip).unit().perp())
                    .abs()
                    < TINY
            );
            assert_eq!(focal.outgoing, V::new(side, 0.0));
        }

        let [a, b, c] = diagram::principal_rays(0.2, 0.2, 0.1, side);
        assert_eq!(a.unwrap().outgoing, b.unwrap().outgoing);
        assert!(c.is_none());
    }
}

#[test]
fn critical_angle_value_is_the_textbook_one() {
    let deg = geom::critical_angle(1.5, 1.0).unwrap().to_degrees();
    // asin(1/1.5) = 41.81... degrees.
    assert!((deg - 41.8103).abs() < 1e-3);
    let water = geom::critical_angle(1.33, 1.0).unwrap().to_degrees();
    assert!((water - 48.75).abs() < 0.05);
}

#[test]
fn snell_and_critical_angle_labels_are_explicit() {
    // Refraction air -> glass at 45 deg transmits at asin(sin 45 / 1.5).
    let refr = scene(&params(&[
        ("setup", ParamValue::EnumChoice("refraction".into())),
        ("angle_deg", ParamValue::Number(45.0)),
    ]));
    let t = texts(&refr);
    assert!(t.contains(&"θ₂ = 28.1°"), "{t:?}");
    assert!(t.contains(&"n₁ sin θ₁ = n₂ sin θ₂"));
    // Media are labelled by role: n₁ incident, n₂ transmitting.
    assert!(t.contains(&"n₁ = 1.00"), "{t:?}");
    assert!(t.contains(&"n₂ = 1.50"), "{t:?}");

    // Glass -> air past the critical angle: no θ₂, only total internal
    // reflection.
    let past_critical = scene(&params(&[
        ("setup", ParamValue::EnumChoice("refraction".into())),
        ("angle_deg", ParamValue::Number(60.0)),
        ("index_1", ParamValue::Number(1.5)),
        ("index", ParamValue::Number(1.0)),
    ]));
    let t = texts(&past_critical);
    assert!(t.contains(&"Total internal reflection"), "{t:?}");
    assert!(!t.iter().any(|s| s.contains("θ₂")));

    // The tir sweep states the ratio that defines the critical angle, and its
    // n₁ is the incident (denser) slab, matching the refraction convention.
    let tir = scene(&params(&[
        ("setup", ParamValue::EnumChoice("tir".into())),
        ("progress", ParamValue::Number(1.0)),
        ("angle_deg", ParamValue::Number(45.0)),
        ("index_1", ParamValue::Number(1.0)),
        ("index", ParamValue::Number(1.5)),
    ]));
    let t = texts(&tir);
    assert!(
        t.iter()
            .any(|s| s.contains("sin θc = n₂/n₁") && s.contains("41.8")),
        "{t:?}"
    );
    assert!(t.contains(&"n₁ = 1.50"), "{t:?}");
    assert!(t.contains(&"n₂ = 1.00"), "{t:?}");
    assert!(t.contains(&"θ₁ = 45.0°"), "{t:?}");
}

#[test]
fn mirror_curvature_maps_to_signed_focal_with_radius_floor() {
    let u = 720.0;
    // Concave (>= 0) focal is positive, convex negative, both R/2.
    assert!((mirror_focal(30.0, u) - 0.15 * u).abs() < 1e-9);
    assert!((mirror_focal(-30.0, u) + 0.15 * u).abs() < 1e-9);
    // Radii below the 10% floor use the floored radius; zero is concave.
    assert!((mirror_focal(5.0, u) - 0.05 * u).abs() < 1e-9);
    assert!((mirror_focal(-5.0, u) + 0.05 * u).abs() < 1e-9);
    assert!(mirror_focal(0.0, u) > 0.0);
    assert!((mirror_focal(50.0, u) - 0.25 * u).abs() < 1e-9);
    assert!((mirror_focal(-50.0, u) + 0.25 * u).abs() < 1e-9);
}

#[test]
fn near_focus_images_scale_into_finite_on_canvas_coordinates() {
    let size = Size {
        width: 1280,
        height: 720,
    };
    let cases: [(&str, Vec<ParamEntry>); 4] = [
        (
            // Converging lens a hair past f: a distant real image.
            "lens-diagram far past f",
            params(&[
                ("setup", ParamValue::EnumChoice("lens-diagram".into())),
                ("focal_length", ParamValue::Number(20.0)),
                ("object_distance", ParamValue::Number(20.5)),
            ]),
        ),
        (
            // A hair inside f: a distant virtual image.
            "lens-diagram just inside f",
            params(&[
                ("setup", ParamValue::EnumChoice("lens-diagram".into())),
                ("focal_length", ParamValue::Number(20.0)),
                ("object_distance", ParamValue::Number(19.5)),
            ]),
        ),
        (
            // Concave mirror with the object between C and F at the default
            // height: |m| = 30, previously drawn as "beyond frame".
            "mirror between C and F",
            params(&[
                ("setup", ParamValue::EnumChoice("mirror".into())),
                ("object_distance", ParamValue::Number(15.5)),
            ]),
        ),
        (
            // Inside F with a tall object: |m| grows even larger.
            "mirror inside F, tall object",
            params(&[
                ("setup", ParamValue::EnumChoice("mirror".into())),
                ("object_distance", ParamValue::Number(14.5)),
                ("object_height", ParamValue::Number(30.0)),
            ]),
        ),
    ];
    for (name, entries) in cases {
        let rendered = render(&entries, size).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_finite_and_on_canvas(&rendered, size);
        assert!(!rendered.elements.is_empty(), "{name}");
        // The auto-fit must keep magnified images on the diagram.
        let labels = texts(&rendered);
        assert!(
            !labels.iter().any(|s| s.contains("beyond frame")),
            "{name}: {labels:?}"
        );
    }
}

#[test]
fn dispersion_index_and_spectrum_are_sane() {
    // 550 nm is the calibration point.
    assert!((crate::spectrum::index_at(1.5, 550.0) - 1.5).abs() < 1e-12);
    // Shorter wavelength => larger index (blue bends more than red).
    assert!(crate::spectrum::index_at(1.5, 400.0) > crate::spectrum::index_at(1.5, 700.0));
    for w in 1..=5 {
        let samples = crate::spectrum::sample_wavelengths(w * 8);
        assert_eq!(samples.first().unwrap().round() as i64, 400);
        assert_eq!(samples.last().unwrap().round() as i64, 700);
        // Monotone index across the samples.
        for pair in samples.windows(2) {
            assert!(
                crate::spectrum::index_at(1.5, pair[0]) >= crate::spectrum::index_at(1.5, pair[1])
            );
        }
    }
    // Spectral colours are distinct and ordered violet -> red.
    let (vr, _vg, vb) = crate::spectrum::wavelength_rgb(410.0);
    let (rr, _rg, rb) = crate::spectrum::wavelength_rgb(680.0);
    assert!(vb > rb && vr < rr);
    assert!(crate::spectrum::wavelength_rgb(550.0).1 > 0);
}

#[test]
fn dispersion_vanishes_at_index_one_and_scales_with_n_minus_one() {
    // An index of 1 is vacuum: no dispersion at any wavelength.
    for nm in [400.0, 450.0, 550.0, 620.0, 700.0] {
        assert!(
            (crate::spectrum::index_at(1.0, nm) - 1.0).abs() < 1e-12,
            "{nm}"
        );
    }
    // The Cauchy coefficient is calibrated BK7-like at n = 1.5
    // (B = 0.004 µm²): n(400) - n(700) = 0.004 · (1/0.16 - 1/0.49).
    let spread =
        |base: f64| crate::spectrum::index_at(base, 400.0) - crate::spectrum::index_at(base, 700.0);
    assert!((spread(1.5) - 0.004 * (1.0 / 0.16 - 1.0 / 0.49)).abs() < 1e-12);
    // Weaker media disperse proportionally less, stronger ones more.
    assert!(spread(1.2) > 0.0 && spread(1.2) < spread(1.5));
    assert!(spread(1.9) > spread(1.5));
    // The calibration point stays exact for every base.
    for base in [1.0, 1.2, 1.5, 1.9, 2.5] {
        assert!((crate::spectrum::index_at(base, 550.0) - base).abs() < 1e-12);
    }
}

#[test]
fn prism_spectrum_bends_violet_more_than_red() {
    use crate::trace::{trace, Surface, MAX_SEGMENTS};
    let ([top, left, right], entry, incident) = diagram::prism_beam(1.0, 45.0_f64.to_radians());
    let surfaces = [
        Surface { a: top, b: left },
        Surface { a: left, b: right },
        Surface { a: right, b: top },
    ];
    assert!(((top - left).length() - (right - left).length()).abs() < TINY);
    let source = entry - incident;
    let wavelengths = crate::spectrum::sample_wavelengths(64);
    let budget = MAX_SEGMENTS / wavelengths.len();
    let mut deviation = f64::NEG_INFINITY;
    let mut segments = 0;
    for wavelength in wavelengths {
        let n = crate::spectrum::index_at(1.5, wavelength);
        let ray = trace(source, incident, &surfaces, 1.0, n, budget);
        assert_eq!(ray, trace(source, incident, &surfaces, 1.0, n, budget));
        assert!(!ray.inside);
        assert_eq!(ray.hits.len(), 2);
        assert!((ray.hits[0] - entry).length() < TINY);
        assert!((ray.hits[1] - right).dot((top - right).unit().perp()).abs() < TINY);
        let exit = ray.onward.unwrap();
        assert!(exit.x > 0.0 && exit.y < 0.0);
        let angle = exit.y.atan2(exit.x);
        assert!(
            angle > deviation,
            "shorter wavelengths must deviate farther down"
        );
        deviation = angle;
        segments += ray.hits.len() + 1;
    }
    assert!(segments <= MAX_SEGMENTS);
}

#[test]
fn prism_trace_budget_stops_at_boundary_during_internal_reflection() {
    use crate::trace::{trace, Surface};
    let ([top, left, right], entry, incident) = diagram::prism_beam(1.0, 5.0_f64.to_radians());
    let surfaces = [
        Surface { a: top, b: left },
        Surface { a: left, b: right },
        Surface { a: right, b: top },
    ];
    let source = entry - incident;
    let limited = trace(source, incident, &surfaces, 1.0, 1.5, 3);
    assert_eq!(limited.hits.len(), 2);
    assert!(limited.inside && limited.onward.is_none());
    let full = trace(source, incident, &surfaces, 1.0, 1.5, 9);
    assert!(full.hits.len() > limited.hits.len());
    let bounce = (full.hits[2] - full.hits[1]).unit();
    let before = (full.hits[1] - full.hits[0]).unit();
    let normal = (top - right).unit().perp();
    assert!((bounce - geom::reflect(before, normal)).length() < TINY);
}

#[test]
fn prism_corner_stops_at_the_last_defined_surface() {
    use crate::trace::{trace, Surface};
    let ([top, left, right], entry, incident) = diagram::prism_beam(1.0, 0.0);
    let surfaces = [
        Surface { a: top, b: left },
        Surface { a: left, b: right },
        Surface { a: right, b: top },
    ];
    let ray = trace(entry - incident, incident, &surfaces, 1.0, 1.5, 9);
    assert_eq!(ray.hits.len(), 2);
    assert!((ray.hits[1] - right).length() < TINY);
    assert!(ray.onward.is_none());
}

#[test]
fn ray_geometry_primitives() {
    let orig = V::new(0.0, 0.0);
    let dir = V::new(1.0, 0.0);
    // Segment crossing.
    let t = orig
        .ray_segment_hit(dir, V::new(5.0, -2.0), V::new(5.0, 2.0))
        .unwrap();
    assert!((t - 5.0).abs() < 1e-12);
    // Segment behind the ray origin: no hit.
    assert!(orig
        .ray_segment_hit(dir, V::new(-5.0, -2.0), V::new(-5.0, 2.0))
        .is_none());
    // Parallel segment: no hit.
    assert!(orig
        .ray_segment_hit(dir, V::new(5.0, 2.0), V::new(9.0, 2.0))
        .is_none());
    // Line intersection.
    let p = geom::lines_intersection(
        V::new(0.0, 0.0),
        V::new(10.0, 10.0),
        V::new(0.0, 10.0),
        V::new(10.0, 0.0),
    )
    .unwrap();
    assert!((p.x - 5.0).abs() < 1e-12 && (p.y - 5.0).abs() < 1e-12);
    assert!(geom::lines_intersection(
        V::new(0.0, 0.0),
        V::new(1.0, 0.0),
        V::new(0.0, 1.0),
        V::new(1.0, 1.0)
    )
    .is_none());
}

#[test]
fn partial_polyline_covers_exact_fractions() {
    let line = [V::new(0.0, 0.0), V::new(10.0, 0.0), V::new(10.0, 10.0)];
    assert!(geom::partial_polyline(&line, 0.0).is_empty());
    assert_eq!(geom::partial_polyline(&line, 1.0), line.to_vec());
    let half = geom::partial_polyline(&line, 0.5);
    let half_len = geom::polyline_length(&half);
    assert!(
        (half_len - 10.0).abs() < 1e-12,
        "half of 20 is 10, got {half_len}"
    );
    let quarter = geom::partial_polyline(&line, 0.25);
    assert!((quarter.last().unwrap().x - 5.0).abs() < 1e-12);
    // Monotone in t.
    let mut last = 0.0;
    for i in 1..=10 {
        let len = geom::polyline_length(&geom::partial_polyline(&line, i as f64 / 10.0));
        assert!(len >= last - TINY);
        last = len;
    }
}

#[test]
fn formats_are_stable_and_ascii() {
    assert_eq!(geom::fmt(41.8103, 1), "41.8");
    assert_eq!(geom::fmt(-0.004, 2), "0.00"); // negative zero cleans up
    assert_eq!(geom::fmt(1.5, 2), "1.50");
    assert!(geom::fmt(12.345, 2).is_ascii());
}

#[test]
fn all_setups_render_and_stay_bounded() {
    for setup in SETUPS {
        let scene = scene(&params(&[("setup", ParamValue::EnumChoice(setup.into()))]));
        assert!(
            !scene.elements.is_empty(),
            "{setup} must draw with default parameters"
        );
        assert!(scene.elements.len() <= MAX_ELEMENTS);
    }
}

#[test]
fn rendering_is_deterministic_and_progress_drives_rays() {
    let a = scene(&params(&[(
        "setup",
        ParamValue::EnumChoice("dispersion".into()),
    )]));
    let b = scene(&params(&[(
        "setup",
        ParamValue::EnumChoice("dispersion".into()),
    )]));
    assert!(
        format!("{a:?}") == format!("{b:?}"),
        "repeat render differs"
    );
    let mut frames = Vec::new();
    for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
        frames.push(scene(&params(&[
            ("setup", ParamValue::EnumChoice("lens-diagram".into())),
            ("progress", ParamValue::Number(p)),
        ])));
    }
    assert!(format!("{:?}", frames[0]) != format!("{:?}", frames[2]));
    assert!(format!("{:?}", frames[2]) != format!("{:?}", frames[4]));
    // Growth is monotone: a later progress frame is never a prefix-truncation
    // of an earlier one's ray work. Simply check all frames differ.
    for i in 1..frames.len() {
        assert!(format!("{:?}", frames[i - 1]) != format!("{:?}", frames[i]));
    }
}

#[test]
fn toggles_change_the_scene() {
    let with = scene(&params(&[
        ("setup", ParamValue::EnumChoice("refraction".into())),
        ("show_labels", ParamValue::Bool(true)),
    ]));
    let without = scene(&params(&[
        ("setup", ParamValue::EnumChoice("refraction".into())),
        ("show_labels", ParamValue::Bool(false)),
    ]));
    assert!(format!("{with:?}") != format!("{without:?}"));
    let colored = scene(&params(&[
        ("setup", ParamValue::EnumChoice("dispersion".into())),
        ("spectrum", ParamValue::Bool(true)),
    ]));
    let plain = scene(&params(&[
        ("setup", ParamValue::EnumChoice("dispersion".into())),
        ("spectrum", ParamValue::Bool(false)),
    ]));
    assert!(format!("{colored:?}") != format!("{plain:?}"));
}

#[test]
fn invalid_and_extreme_inputs_fail_cleanly() {
    assert!(render(
        &[ParamEntry {
            id: "setup".into(),
            value: ParamValue::EnumChoice("holodeck".into()),
        }],
        Size {
            width: 100,
            height: 100
        },
    )
    .is_err());
    assert!(render(
        &[],
        Size {
            width: 0,
            height: 100
        },
    )
    .is_err());
    assert!(render(
        &[ParamEntry {
            id: "progress".into(),
            value: ParamValue::Number(f64::NAN),
        }],
        Size {
            width: 100,
            height: 100
        },
    )
    .is_err());
    // Extreme but valid parameters still render (clamped, finite).
    let extreme = scene(&params(&[
        ("setup", ParamValue::EnumChoice("mirror".into())),
        ("curvature", ParamValue::Number(-50.0)),
        ("object_distance", ParamValue::Number(90.0)),
        ("object_height", ParamValue::Number(30.0)),
        ("angle_deg", ParamValue::Number(89.0)),
    ]));
    assert!(!extreme.elements.is_empty());
    let near_focus = scene(&params(&[
        ("setup", ParamValue::EnumChoice("lens-diagram".into())),
        ("focal_length", ParamValue::Number(20.0)),
        ("object_distance", ParamValue::Number(20.0)), // exactly at f
    ]));
    assert!(!near_focus.elements.is_empty());
}
