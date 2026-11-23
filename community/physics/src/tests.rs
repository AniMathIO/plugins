//! Native unit tests: analytic correctness against textbook values, determinism of
//! the free simulation, parameter handling and bounded work.

use super::*;
use draw::View;

fn base_params() -> Vec<ParamEntry> {
    let specs = PhysicsPlay::describe().params;
    specs
        .iter()
        .map(|spec| ParamEntry {
            id: spec.id.clone(),
            value: match &spec.kind {
                ParamKind::Number(range) => ParamValue::Number(range.default),
                ParamKind::Color(c) => ParamValue::Color(c.default),
                ParamKind::String(s) => ParamValue::String(s.default.clone()),
                ParamKind::EnumChoice(e) => ParamValue::EnumChoice(e.default.clone()),
                ParamKind::Bool(b) => ParamValue::Bool(b.default),
                ParamKind::Point(pt) => ParamValue::Point(pt.default),
                ParamKind::AssetRef(_) => ParamValue::String(String::new()),
            },
        })
        .collect()
}

fn with(list: &mut [ParamEntry], id: &str, value: ParamValue) {
    list.iter_mut()
        .find(|p| p.id == id)
        .expect("known param")
        .value = value;
}

fn with_all(list: &mut [ParamEntry], values: &[(&str, ParamValue)]) {
    for (id, value) in values {
        with(list, id, value.clone());
    }
}

fn render_snapshot(list: &[ParamEntry], size: Size) -> String {
    match render(list, size) {
        Ok(scene) => format!("{scene:?}"),
        Err(d) => format!("ERR {} {}", d.code, d.message),
    }
}

fn readouts(list: &[ParamEntry]) -> Vec<String> {
    render(list, CANVAS)
        .unwrap()
        .elements
        .into_iter()
        .filter_map(|element| match element {
            DrawCommand::TextRun(text) if text.x == 16.0 => Some(text.text),
            _ => None,
        })
        .collect()
}

const CANVAS: Size = Size {
    width: 1280,
    height: 720,
};

#[test]
fn elliptic_integral_matches_known_values() {
    let k = analytic::elliptic_k(0.0);
    assert!((k - std::f64::consts::FRAC_PI_2).abs() < 1e-12, "{k}");
    // Abramowitz & Stegun 17.3.1 reference values.
    assert!((analytic::elliptic_k(0.25) - 1.685_750_354_812_596).abs() < 1e-12);
    assert!((analytic::elliptic_k(0.5) - 1.854_074_677_301_372).abs() < 1e-12);
}

#[test]
fn pendulum_period_reduces_to_small_angle_and_grows_with_amplitude() {
    let (l, g) = (1.5, 9.81);
    let small = analytic::pendulum_period(l, g, 0.5);
    let small_angle = std::f64::consts::TAU * (l / g).sqrt();
    assert!(
        (small - small_angle).abs() / small_angle < 1e-5,
        "{small} vs {small_angle}"
    );
    // Exact ratio for a 60° release: T/T₀ = 2·K(0.25)/π ≈ 1.07318.
    let ratio = analytic::pendulum_period(l, g, 60.0) / small_angle;
    assert!((ratio - 2.0 * analytic::elliptic_k(0.25) / std::f64::consts::PI).abs() < 1e-12);
    assert!((ratio - 1.073_182).abs() < 1e-5, "{ratio}");
    // No gravity: no oscillation, period infinite.
    assert!(analytic::pendulum_period(l, 0.0, 45.0).is_infinite());
}

#[test]
fn nonlinear_pendulum_keeps_energy_and_reaches_its_turning_points() {
    let (l, g, angle) = (0.2, 30.0, 90.0_f64);
    let theta0 = angle.to_radians();
    let period = analytic::pendulum_period(l, g, angle);
    let initial_energy = analytic::pendulum_energy(l, g, theta0, 0.0);
    for step in 0..=64 {
        let time = step as f64 / 64.0 * period;
        let (theta, omega) = analytic::pendulum_state(theta0, g / l, period, time);
        let energy = analytic::pendulum_energy(l, g, theta, omega);
        assert!((energy - initial_energy).abs() / initial_energy < 1e-6);
        match step {
            16 => assert!(theta.abs() < 1e-6 && omega < 0.0),
            32 => assert!((theta + theta0).abs() < 1e-12 && omega.abs() < 1e-12),
            48 => assert!(theta.abs() < 1e-6 && omega > 0.0),
            64 => assert_eq!((theta, omega), (theta0, 0.0)),
            _ => {}
        }
    }
    assert_eq!(
        analytic::pendulum_state(theta0, 0.0, f64::INFINITY, 30.0),
        (theta0, 0.0)
    );
}

#[test]
fn projectile_matches_textbook_formulas() {
    let (v, angle, g) = (15.0, 45.0, 9.81);
    let flight = analytic::flight_time(v, angle, g).unwrap();
    let expected_range = v * v * (2.0 * angle.to_radians()).sin() / g;
    let range = analytic::projectile_position(v, angle, g, flight).0;
    assert!(
        (range - expected_range).abs() < 1e-9,
        "{range} vs {expected_range}"
    );
    // Apex exactly halfway: height (v·sinα)²/(2g).
    let apex = analytic::projectile_position(v, angle, g, flight * 0.5).1;
    let expected_apex = (v * angle.to_radians().sin()).powi(2) / (2.0 * g);
    assert!((apex - expected_apex).abs() < 1e-9);
    // Horizontal motion is uniform; vertical acceleration is −g.
    let (vx, vy) = analytic::projectile_velocity(v, angle, g, 0.3);
    assert!((vx - v * angle.to_radians().cos()).abs() < 1e-12);
    let (_, vy2) = analytic::projectile_velocity(v, angle, g, 0.8);
    assert!(((vy2 - vy) / (0.8 - 0.3) + g).abs() < 1e-9);
    // Ground-level horizontal launch cannot stay aloft in gravity.
    assert!(analytic::flight_time(v, angle, 0.0).is_none());
    assert_eq!(analytic::flight_time(v, 0.0, g), Some(0.0));
}

#[test]
fn spring_is_exact_hooke_motion() {
    let (l, g) = (1.5, 9.81);
    let omega = analytic::spring_omega(l, g);
    assert!((omega - (g / l).sqrt()).abs() < 1e-12);
    let amplitude = 0.3 * l;
    assert_eq!(analytic::spring_offset(amplitude, omega, 0.0), amplitude);
    assert!(analytic::spring_velocity(amplitude, omega, 0.0).abs() < 1e-12);
    // Half a period later: mirror position, zero velocity.
    let half = std::f64::consts::PI / omega;
    assert!((analytic::spring_offset(amplitude, omega, half) + amplitude).abs() < 1e-9);
    assert!(analytic::spring_velocity(amplitude, omega, half).abs() < 1e-9);
    // Same period as the small-angle pendulum of the same length (tuned spring).
    let pendulum_small = std::f64::consts::TAU * (l / g).sqrt();
    let spring = std::f64::consts::TAU / omega;
    assert!((spring - pendulum_small).abs() < 1e-12);
    assert_eq!(analytic::spring_omega(l, 0.0), 0.0);
}

#[test]
fn full_duration_uses_one_stable_step_grid() {
    for duration in [0.5, 4.0, 20.1, 30.0] {
        let full = sim::Plan::new(1.0, duration);
        assert!((full.time() - duration).abs() < 1e-12);
        assert!(full.steps <= sim::MAX_STEPS);
        assert!(full.dt <= 1.0 / sim::HZ);
        let mut previous = 0;
        for step in 0..=64 {
            let plan = sim::Plan::new(f64::from(step) / 64.0, duration);
            assert!(plan.steps >= previous);
            assert_eq!(plan.dt, full.dt);
            previous = plan.steps;
        }
    }
    assert_eq!(sim::Plan::new(1.0, 30.0).steps, sim::MAX_STEPS);
}

#[test]
fn isolated_equal_mass_collision_obeys_restitution_and_conserves_momentum() {
    for speed in [5.0, 40.0] {
        for restitution in [0.0, 0.2, 1.0] {
            let material = sim::Material {
                gravity: 0.0,
                friction: 0.0,
                restitution,
            };
            let time = if speed == 5.0 { 0.85 } else { 0.2 };
            let state = sim::collision(2, speed, material, sim::Plan::new(time, 1.0), false);
            assert!(!state.boundary_impulse);
            let [a, b] = state.motion.bodies.as_slice() else {
                panic!("expected two disks")
            };
            assert!((a.vx + b.vx - speed).abs() < 1e-12);
            assert!((b.vx - a.vx - restitution * speed).abs() < 1e-12);
            let energy = 0.5 * (a.vx.powi(2) + b.vx.powi(2));
            let expected = 0.25 * speed.powi(2) * (1.0 + restitution.powi(2));
            assert!((energy - expected).abs() < 1e-12);
        }
    }
}

#[test]
fn default_collision_still_has_kinetic_energy_at_midpoint() {
    let p = read_params(&[]).unwrap();
    let state = sim::collision(
        p.bodies,
        p.speed,
        p.material(),
        sim::Plan::new(0.5, p.duration),
        false,
    );
    let energy: f64 = state
        .motion
        .bodies
        .iter()
        .map(|body| 0.5 * p.mass * (body.vx.powi(2) + body.vy.powi(2)))
        .sum();
    assert!(energy > 0.0);
    assert!(state.boundary_impulse);
}

fn domino_overlap(a: sim::Snapshot, b: sim::Snapshot) -> f64 {
    let axes = |angle: f64| {
        let (sin, cos) = angle.sin_cos();
        [(cos, sin), (-sin, cos)]
    };
    let a_axes = axes(a.angle);
    let b_axes = axes(b.angle);
    let dot = |(x, y): (f64, f64), (u, v): (f64, f64)| x * u + y * v;
    let radius = |axis, body_axes: [(f64, f64); 2]| {
        sim::DOMINO_HX * dot(axis, body_axes[0]).abs()
            + sim::DOMINO_HY * dot(axis, body_axes[1]).abs()
    };
    a_axes
        .into_iter()
        .chain(b_axes)
        .map(|axis| {
            radius(axis, a_axes) + radius(axis, b_axes) - dot(axis, (b.x - a.x, b.y - a.y)).abs()
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn neighboring_dominoes_do_not_penetrate_during_replay() {
    let p = read_params(&[]).unwrap();
    let plan = sim::Plan::new(1.0, p.duration);
    for count in [6, 12] {
        for steps in 0..=plan.steps {
            let sample = sim::Plan { steps, ..plan };
            let state = sim::stack(count, p.speed, p.material(), sample, false);
            for (index, pair) in state.bodies[1..].windows(2).enumerate() {
                let overlap = domino_overlap(pair[0], pair[1]);
                assert!(
                    overlap < 0.001,
                    "{count} dominoes, pair {index}, t = {:.4} s: {overlap:.6} m",
                    sample.time()
                );
            }
        }
    }
}

#[test]
fn domino_ball_rebounds_with_configured_restitution() {
    let p = read_params(&[]).unwrap();
    let sample = |restitution, time| {
        let material = sim::Material {
            restitution,
            ..p.material()
        };
        sim::stack(
            p.bodies,
            p.speed,
            material,
            sim::Plan::new(time / p.duration, p.duration),
            false,
        )
        .bodies[0]
    };
    assert_eq!(sample(0.0, 0.2).vx, 0.0);
    let arm = sim::STACK_BALL_R;
    let inertia = ((2.0 * sim::DOMINO_HX).powi(2) + (2.0 * sim::DOMINO_HY).powi(2)) / 3.0;
    for restitution in [0.4, 0.8, 1.0] {
        let after = sample(restitution, 0.2);
        assert!(after.vx < 0.0);
        let impulse = p.speed - after.vx;
        let contact_speed = impulse * arm * arm / inertia;
        assert!((contact_speed - after.vx - p.speed * restitution).abs() < 1e-12);
        assert!(sample(restitution, 0.3).x < after.x);
    }
}

#[test]
fn domino_contact_propagates_down_the_row() {
    let material = sim::Material {
        gravity: 9.81,
        restitution: 0.8,
        friction: 0.8,
    };
    let run =
        |progress, speed| sim::stack(12, speed, material, sim::Plan::new(progress, 4.0), true);
    let underway = run(0.1, 15.0).bodies;
    assert!(underway[1].angle < 0.0);
    assert_eq!(underway[12].angle, 0.0);
    let complete = run(1.0, 15.0).bodies;
    assert!(
        complete[1..].iter().all(|block| block.angle < -0.6),
        "{complete:?}"
    );
    assert!(run(1.0, 0.0).bodies.iter().all(|body| body.angle == 0.0));
}

#[test]
fn contact_solvers_are_deterministic_at_maximum_work() {
    let material = sim::Material {
        gravity: 30.0,
        restitution: 1.0,
        friction: 0.0,
    };
    let plan = sim::Plan::new(1.0, 30.0);
    let run = || {
        [
            sim::collision(12, 40.0, material, plan, true).motion,
            sim::stack(12, 40.0, material, plan, true),
        ]
    };
    for (first, second) in run().into_iter().zip(run()) {
        let bits =
            |body: &sim::Snapshot| [body.x, body.y, body.angle, body.vx, body.vy].map(f64::to_bits);
        assert!(first
            .bodies
            .iter()
            .map(bits)
            .eq(second.bodies.iter().map(bits)));
        assert_eq!(first.trail, second.trail);
        assert!(first.trail.len() <= 226);
        assert!(first
            .bodies
            .iter()
            .flat_map(bits)
            .all(|value| f64::from_bits(value).is_finite()));
    }
    let state = sim::collision(12, 40.0, material, plan, false).motion;
    assert!(state.trail.is_empty());
    for body in state.bodies {
        assert!((sim::BALL_RADIUS..=sim::ARENA_W - sim::BALL_RADIUS).contains(&body.x));
        assert!((sim::BALL_RADIUS..=sim::ARENA_H - sim::BALL_RADIUS).contains(&body.y));
    }
}

#[test]
fn solver_work_stays_within_published_bounds() {
    let plan = sim::Plan::new(1.0, 30.0);
    let material = sim::Material {
        gravity: 30.0,
        restitution: 1.0,
        friction: 0.0,
    };
    // The README bounds disk work at 66 pair checks and two wall passes per step,
    // and the domino row at two visits per pair per step. Measure both solvers
    // at their heaviest settings and at the defaults.
    for (bodies, speed, material) in [
        (12, 40.0, material),
        (6, 15.0, read_params(&[]).unwrap().material()),
    ] {
        let collision = sim::collision(bodies, speed, material, plan, false).motion;
        assert!(collision.work.pair_checks > 0);
        assert!(collision.work.pair_checks <= 66 * plan.steps as u32);
        assert!(collision.work.wall_checks <= 24 * plan.steps as u32);
        let stack = sim::stack(bodies, speed, material, plan, false);
        assert!(stack.work.pair_checks > 0);
        assert!(stack.work.pair_checks <= 2 * 11 * plan.steps as u32);
    }
}

#[test]
fn every_scene_renders_deterministically_at_every_progress() {
    for (scene_name, _) in SCENES {
        for k in 0..=4 {
            let mut list = base_params();
            with_all(
                &mut list,
                &[
                    ("scene", ParamValue::EnumChoice(scene_name.to_string())),
                    ("progress", ParamValue::Number(k as f64 / 4.0)),
                ],
            );
            let first = render_snapshot(&list, CANVAS);
            let second = render_snapshot(&list, CANVAS);
            assert_eq!(first, second, "{scene_name} progress {k}/4");
            assert!(!first.starts_with("ERR"), "{scene_name}: {first}");
        }
    }
}

#[test]
fn progress_changes_every_scene() {
    for (scene_name, _) in SCENES {
        let mut list = base_params();
        with(
            &mut list,
            "scene",
            ParamValue::EnumChoice(scene_name.to_string()),
        );
        with(&mut list, "progress", ParamValue::Number(0.0));
        let start = render_snapshot(&list, CANVAS);
        with(&mut list, "progress", ParamValue::Number(1.0));
        let end = render_snapshot(&list, CANVAS);
        assert_ne!(start, end, "{scene_name}");
    }
}

#[test]
fn default_scenes_report_drawn_quantities() {
    let mut actual = Vec::new();
    for (name, _) in SCENES {
        let list = [
            ParamEntry {
                id: "scene".into(),
                value: ParamValue::EnumChoice(name.into()),
            },
            ParamEntry {
                id: "progress".into(),
                value: ParamValue::Number(0.5),
            },
        ];
        actual.push(readouts(&list));
    }
    assert_eq!(
        actual,
        [
            vec!["45.0° launch  |  Height = 1.6 m", "t = 2.00 s / 4.00 s"],
            vec!["T = 2.56 s", "angle = 9.3 deg", "t = 2.00 s / 4.00 s"],
            vec!["T = 2.46 s", "Extension = 1.68 m", "t = 2.00 s / 4.00 s"],
            vec![
                "Momentum (not conserved) = (-0.9, 0.0) kg m/s",
                "Kinetic energy = 7.54 J",
                "t = 2.00 s / 4.00 s"
            ],
            vec!["Toppled = 6/6", "t = 2.00 s / 4.00 s"],
        ]
    );
}

#[test]
fn projectile_includes_the_selected_angle_and_frames_resting_bodies() {
    let mut list = base_params();
    with(&mut list, "progress", ParamValue::Number(0.5));
    let p = read_params(&list).unwrap();
    let angles: Vec<_> = launch_angles(&p).collect();
    assert_eq!(angles, [36.0, 40.5, 48.0, 51.0, 54.0, 45.0]);
    let radius = projectile_radius(p.speed);
    let view = View::fit(&extent_for(&p).sanitized(), CANVAS, 0.07);
    let (x, y) = analytic::projectile_position(p.speed, 45.0, p.gravity, 2.0);
    let center = view.point(x, y + radius);
    assert!(render(&list, CANVAS)
        .unwrap()
        .elements
        .iter()
        .any(|element| {
            matches!(element, DrawCommand::FillPath((path, color))
            if color.r == p.accent.r && color.g == p.accent.g && color.b == p.accent.b
                && color.a == p.accent.a
                && matches!(path.first(), Some(PathVerb::MoveTo(start))
                    if start.x == center.x + view.dist(radius) && start.y == center.y))
        }));
    with(&mut list, "angle_deg", ParamValue::Number(0.0));
    assert_eq!(readouts(&list)[0], "0.0° launch  |  Range = 0.0 m");
    with(&mut list, "speed", ParamValue::Number(0.0));
    let p = read_params(&list).unwrap();
    assert_eq!(launch_angles(&p).collect::<Vec<_>>(), [0.0]);
    let extent = extent_for(&p);
    assert!(extent.max_x - extent.min_x >= 3.0 && extent.max_y - extent.min_y >= 2.0);
    assert_eq!(readouts(&list)[0], "0.0° launch  |  Range = 0.0 m");
}

#[test]
fn projectile_velocity_vectors_clear_the_ground_and_other_balls() {
    let from = Point { x: 0.0, y: 30.0 };
    let to = Point { x: 100.0, y: 80.0 };
    let tip = clip_projectile_vector(from, to, 60.0, 10.0, std::iter::empty()).unwrap();
    assert!((tip.y - 58.5).abs() < 1e-3);
    assert!((tip.x - 57.0).abs() < 1e-3);
    let level = Point { x: 100.0, y: 30.0 };
    let ball = Point { x: 50.0, y: 30.0 };
    let tip = clip_projectile_vector(from, level, 100.0, 10.0, std::iter::once(ball)).unwrap();
    assert!(tip.x < 40.0);
    let near = Point { x: 5.0, y: 30.0 };
    assert!(clip_projectile_vector(from, level, 100.0, 10.0, std::iter::once(near)).is_none());
    let shallow = Point { x: 100.0, y: 29.9 };
    assert!(clip_projectile_vector(from, shallow, 35.0, 10.0, std::iter::empty()).is_none());
}

#[test]
fn max_duration_max_bodies_renders_within_bounds() {
    // The released host smoke test separately checks its memory and fuel limits.
    for (scene_name, _) in SCENES {
        let mut list = base_params();
        with_all(
            &mut list,
            &[
                ("scene", ParamValue::EnumChoice(scene_name.to_string())),
                ("duration_s", ParamValue::Number(30.0)),
                ("progress", ParamValue::Number(1.0)),
                ("bodies", ParamValue::Number(12.0)),
            ],
        );
        let scene =
            render(&list, CANVAS).unwrap_or_else(|err| panic!("{scene_name}: {}", err.message));
        assert!((3..1400).contains(&scene.elements.len()));
    }
}

#[test]
fn toggles_change_the_output() {
    let mut base = base_params();
    with(
        &mut base,
        "scene",
        ParamValue::EnumChoice("projectile".into()),
    );
    with(&mut base, "progress", ParamValue::Number(0.4));
    let with_vectors = render_snapshot(&base, CANVAS);
    let mut variant = base.clone();
    with(&mut variant, "show_vectors", ParamValue::Bool(false));
    let no_vectors = render_snapshot(&variant, CANVAS);
    assert!(
        with_vectors != no_vectors,
        "velocity arrows must affect the frame"
    );
    let mut variant = base.clone();
    with(&mut variant, "show_trail", ParamValue::Bool(false));
    assert_ne!(with_vectors, render_snapshot(&variant, CANVAS));
    let mut variant = base.clone();
    with(&mut variant, "show_ground", ParamValue::Bool(false));
    assert_ne!(with_vectors, render_snapshot(&variant, CANVAS));
    let mut variant = base.clone();
    with(
        &mut variant,
        "accent",
        ParamValue::Color(draw::rgb(0, 255, 0, 255)),
    );
    assert_ne!(with_vectors, render_snapshot(&variant, CANVAS));
    let mut variant = base.clone();
    with(&mut variant, "gravity", ParamValue::Number(1.62));
    assert_ne!(with_vectors, render_snapshot(&variant, CANVAS));
}

#[test]
fn invalid_input_is_rejected_cleanly() {
    assert!(render(
        &base_params(),
        Size {
            width: 0,
            height: 720
        }
    )
    .is_err());
    assert!(render(
        &base_params(),
        Size {
            width: 1280,
            height: 0
        }
    )
    .is_err());
    let mut bad_scene = base_params();
    with(
        &mut bad_scene,
        "scene",
        ParamValue::EnumChoice("orbit".into()),
    );
    assert!(render(&bad_scene, CANVAS).is_err());
    let mut nan = base_params();
    with(&mut nan, "progress", ParamValue::Number(f64::NAN));
    assert!(render(&nan, CANVAS).is_err());
    // Out-of-range numbers are clamped, not rejected.
    let mut high = base_params();
    with(&mut high, "bodies", ParamValue::Number(99.0));
    assert!(render(&high, CANVAS).is_ok());
}

#[test]
fn view_fits_and_is_uniform() {
    let mut extent = Extent::new();
    extent.add(-2.0, -1.0);
    extent.add(6.0, 3.0);
    let view = View::fit(&extent, CANVAS, 0.07);
    // Center maps to center.
    let center = view.point(2.0, 1.0);
    assert!((center.x - 640.0).abs() < 1e-3);
    assert!((center.y - 360.0).abs() < 1e-3);
    // Uniform scale; y flips.
    let right = view.point(6.0, 1.0);
    let top = view.point(2.0, 3.0);
    assert!(right.x > center.x);
    assert!(top.y < center.y);
    assert!((view.dist(4.0) as f64 - (right.x - center.x) as f64).abs() < 1e-3);
    // Everything lands inside the canvas with margins.
    let corner = view.point(-2.0, 3.0);
    assert!(corner.x >= 1280.0 * 0.06 - 1.0 && corner.x <= 1280.0 * 0.94 + 1.0);
}
