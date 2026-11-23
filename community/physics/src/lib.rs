//! Deterministic vector scenes for teaching 2D mechanics.
wit_bindgen::generate!({ path: "wit", world: "source-plugin" });

use animathio::plugin::types::*;

mod analytic;
mod draw;
mod params;
mod sim;

use draw::{
    box_corners, circle_path, polyline, with_alpha, Extent, Painter, View, FOREGROUND, MUTED, TRAIL,
};
use params::*;

pub const ACCENT_DEFAULT: Color = Color {
    r: 240,
    g: 150,
    b: 40,
    a: 255,
};

#[derive(Clone, Copy)]
enum Demo {
    Projectile,
    Pendulum,
    Spring,
    Collision,
    Stack,
}

struct Params {
    scene: Demo,
    progress: f64,
    duration: f64,
    gravity: f64,
    bodies: usize,
    mass: f64,
    speed: f64,
    angle_deg: f64,
    length_m: f64,
    restitution: f64,
    friction: f64,
    show_vectors: bool,
    show_trail: bool,
    show_ground: bool,
    accent: Color,
}

const SCENES: [(&str, Demo); 5] = [
    ("projectile", Demo::Projectile),
    ("pendulum", Demo::Pendulum),
    ("spring", Demo::Spring),
    ("collision", Demo::Collision),
    ("stack", Demo::Stack),
];

fn read_params(list: &[ParamEntry]) -> Result<Params, Diagnostic> {
    let name = string(list, "scene", "projectile");
    let scene = SCENES
        .iter()
        .find(|(id, _)| *id == name)
        .map(|(_, scene)| *scene)
        .ok_or_else(|| error("Unsupported scene"))?;
    Ok(Params {
        scene,
        progress: number(list, "progress", 1.0, 0.0, 1.0)?,
        duration: number(list, "duration_s", 4.0, 0.5, 30.0)?,
        gravity: number(list, "gravity", 9.81, 0.0, 30.0)?,
        bodies: number(list, "bodies", 6.0, 1.0, sim::MAX_BODIES as f64)?.round() as usize,
        mass: number(list, "mass", 1.0, 0.1, 10.0)?,
        speed: number(list, "speed", 15.0, 0.0, 40.0)?,
        angle_deg: number(list, "angle_deg", 45.0, 0.0, 90.0)?,
        length_m: number(list, "length_m", 1.5, 0.2, 10.0)?,
        restitution: number(list, "restitution", 0.8, 0.0, 1.0)?,
        friction: number(list, "friction", 0.8, 0.0, 1.0)?,
        show_vectors: boolean(list, "show_vectors", true),
        show_trail: boolean(list, "show_trail", true),
        show_ground: boolean(list, "show_ground", true),
        accent: color(list, "accent", ACCENT_DEFAULT),
    })
}

struct PhysicsPlay;

impl Guest for PhysicsPlay {
    fn describe() -> Manifest {
        Manifest {
            name: "Physics Play".into(),
            version: "0.1.0".into(),
            description: "Deterministic 2D mechanics demos: projectile, pendulum, spring, collisions and falling stacks".into(),
            kinds: vec!["clip-source".into()],
            metadata: vec![
                ("param.duration_s.unit".into(), "s".into()),
                ("param.gravity.unit".into(), "m/s^2".into()),
                ("param.mass.unit".into(), "kg".into()),
                ("param.speed.unit".into(), "m/s".into()),
                ("param.angle_deg.unit".into(), "degrees".into()),
                ("param.length_m.unit".into(), "m".into()),
            ],
            params: vec![
                enum_param("scene", "Scene", &SCENES.map(|(name, _)| name), "projectile"),
                num_param("progress", "Progress", 0.0, 1.0, 1.0, 0.01),
                num_param("duration_s", "Duration (s)", 0.5, 30.0, 4.0, 0.1),
                num_param("gravity", "Gravity (m/s^2)", 0.0, 30.0, 9.81, 0.01),
                num_param("bodies", "Bodies", 1.0, sim::MAX_BODIES as f64, 6.0, 1.0),
                num_param("mass", "Mass (kg)", 0.1, 10.0, 1.0, 0.1),
                num_param("speed", "Speed (m/s)", 0.0, 40.0, 15.0, 0.1),
                num_param("angle_deg", "Angle (deg)", 0.0, 90.0, 45.0, 1.0),
                num_param("length_m", "Length (m)", 0.2, 10.0, 1.5, 0.1),
                num_param("restitution", "Restitution", 0.0, 1.0, 0.8, 0.05),
                num_param("friction", "Friction", 0.0, 1.0, 0.8, 0.05),
                bool_param("show_vectors", "Velocity vectors", true),
                bool_param("show_trail", "Trail", true),
                bool_param("show_ground", "Ground scale", true),
                color_param("accent", "Accent", ACCENT_DEFAULT),
            ],
        }
    }

    fn render_clip(
        params: Vec<ParamEntry>,
        _time_ticks: i64,
        size: Size,
    ) -> Result<Scene, Diagnostic> {
        render(&params, size)
    }
}

/// Launch angles for `count` projectiles: evenly spread ±9° around `angle_deg`.
fn launch_angles(angle_deg: f64, count: usize) -> impl Iterator<Item = f64> {
    (0..count).map(move |index| {
        if count == 1 {
            return angle_deg;
        }
        let low = (angle_deg - 9.0).max(0.0);
        let high = (angle_deg + 9.0).min(90.0);
        low + (high - low) * index as f64 / (count - 1) as f64
    })
}

fn render(list: &[ParamEntry], size: Size) -> Result<Scene, Diagnostic> {
    if size.width == 0 || size.height == 0 {
        return Err(error("Canvas dimensions must be positive"));
    }
    let p = read_params(list)?;
    let extent = extent_for(&p).sanitized();
    let view = View::fit(&extent, size, 0.07);
    let sim_time = p.progress * p.duration;
    let mut painter = Painter::new();
    match p.scene {
        Demo::Projectile => draw_projectile(&mut painter, &view, &extent, &p, sim_time, size),
        Demo::Pendulum => draw_pendulum(&mut painter, &view, &p, sim_time, size),
        Demo::Spring => draw_spring(&mut painter, &view, &p, sim_time, size),
        Demo::Collision => draw_collision(&mut painter, &view, &p, size),
        Demo::Stack => draw_stack(&mut painter, &view, &extent, &p, size),
    }
    Ok(Scene {
        coordinate_space: CoordinateSpace::Canvas,
        elements: painter.elements,
    })
}

/// Progress-independent framing: derived from parameters only, so the camera never
/// moves while progress scrubs.
fn extent_for(p: &Params) -> Extent {
    match p.scene {
        Demo::Projectile => {
            let mut extent = Extent::new();
            let radius = projectile_radius(p.speed);
            extent.add(0.0, 0.0);
            for angle in launch_angles(p.angle_deg, p.bodies) {
                let flight = analytic::flight_time(p.speed, angle, p.gravity)
                    .unwrap_or(p.duration)
                    .min(p.duration);
                let apex = if p.gravity > 0.0 {
                    (p.speed * angle.to_radians().sin() / p.gravity).min(flight)
                } else {
                    flight
                };
                for time in [apex, flight] {
                    let (x, y) = analytic::projectile_position(p.speed, angle, p.gravity, time);
                    extent.add(x, y + radius);
                }
            }
            extent.pad(radius + 0.4);
            extent
        }
        Demo::Pendulum => {
            let l = p.length_m;
            let bob = (0.16 * l).clamp(0.06, 0.5);
            let reach = (l * p.angle_deg.to_radians().sin()).abs().max(0.3 * l);
            let mut extent = Extent::new();
            extent.add(-reach - bob, -l - bob);
            extent.add(reach + bob, 0.12 * l);
            extent
        }
        Demo::Spring => {
            let l = p.length_m;
            let amplitude = 0.3 * l;
            let half = 0.12 * l;
            let mut extent = Extent::new();
            extent.add(-0.55 * l, -l - amplitude - half - 0.15 * l);
            extent.add(0.55 * l, 0.1 * l);
            extent
        }
        Demo::Collision => {
            let mut extent = Extent::new();
            extent.add(0.0, 0.0);
            extent.add(sim::ARENA_W, sim::ARENA_H);
            extent
        }
        Demo::Stack => {
            let mut extent = Extent::new();
            extent.add(-2.4, -0.1);
            extent.add(1.6 + sim::DOMINO_SPACING * p.bodies as f64, 2.0);
            extent
        }
    }
}

/// Canvas-space font sizes that follow the canvas short edge.
fn fonts(size: Size) -> (f32, f32) {
    let short = size.width.min(size.height) as f32;
    (
        (short * 0.034).clamp(11.0, 40.0),
        (short * 0.021).clamp(9.0, 24.0),
    )
}

fn readout_lines(painter: &mut Painter, lines: &[String], font: f32) {
    for (index, line) in lines.iter().enumerate() {
        painter.text(
            line,
            TextAlignment::Left,
            16.0,
            14.0 + index as f32 * font * 1.45,
            font,
            with_alpha(FOREGROUND, 235),
        );
    }
}

fn time_readout(painter: &mut Painter, size: Size, sim_time: f64, duration: f64, font: f32) {
    painter.text(
        &format!("t = {:.2} s / {:.2} s", sim_time, duration),
        TextAlignment::Left,
        16.0,
        size.height as f32 - font * 1.6,
        font,
        with_alpha(FOREGROUND, 235),
    );
}

fn velocity_arrow(
    painter: &mut Painter,
    view: &View,
    size: Size,
    (x, y): (f64, f64),
    (vx, vy): (f64, f64),
    body_radius_m: f64,
    accent: Color,
) {
    let speed = (vx * vx + vy * vy).sqrt();
    if speed < 2e-2 {
        return;
    }
    let short = size.width.min(size.height) as f64;
    let radius_px = view.dist(body_radius_m) as f64;
    let px_per_unit = short * 0.018;
    // Long enough to leave the body: slow arrows must stay readable.
    let min_len = radius_px + 16.0;
    let len = (speed * px_per_unit).max(min_len).min(short * 0.45);
    let (ux, uy) = (vx / speed, vy / speed);
    let center = view.point(x, y);
    let from = Point {
        x: center.x + (ux * (radius_px + 2.0)) as f32,
        y: center.y - (uy * (radius_px + 2.0)) as f32,
    };
    // The shaft starts at the body edge; keep it long enough to read as an arrow.
    let shaft = (len - radius_px).max(26.0);
    let to = Point {
        x: from.x + (ux * shaft) as f32,
        y: from.y - (uy * shaft) as f32,
    };
    painter.arrow(from, to, 2.5, with_alpha(accent, 220));
}

fn trail_stroke(painter: &mut Painter, view: &View, points: &[(f64, f64)]) {
    let canvas_points: Vec<Point> = points.iter().map(|(x, y)| view.point(*x, *y)).collect();
    painter.dashed(&canvas_points, 1.8, with_alpha(TRAIL, 165), 8.0, 6.5);
}

fn projectile_radius(speed: f64) -> f64 {
    (0.12 * speed.sqrt()).clamp(0.15, 0.6)
}

fn draw_projectile(
    painter: &mut Painter,
    view: &View,
    extent: &Extent,
    p: &Params,
    sim_time: f64,
    size: Size,
) {
    let (font, tick) = fonts(size);
    if p.show_ground {
        draw::draw_ground(painter, view, extent.min_x, extent.max_x, 5, size, tick);
    }
    let radius = projectile_radius(p.speed);
    for angle in launch_angles(p.angle_deg, p.bodies) {
        let (t, landed) = projectile_time(p, angle, sim_time);
        let (x, y) = analytic::projectile_position(p.speed, angle, p.gravity, t);
        let draw_y = y + radius;
        if p.show_trail && t > 0.0 {
            let trace: Vec<_> = (0..=120)
                .map(|step| {
                    let (x, y) = analytic::projectile_position(
                        p.speed,
                        angle,
                        p.gravity,
                        t * f64::from(step) / 120.0,
                    );
                    (x, y + radius)
                })
                .collect();
            trail_stroke(painter, view, &trace);
        }
        let center = view.point(x, draw_y);
        let outline = circle_path(center.x, center.y, view.dist(radius));
        painter.fill(outline.clone(), p.accent);
        painter.stroke(outline, 1.5, with_alpha(FOREGROUND, 200));
        if p.show_vectors && !landed {
            let velocity = analytic::projectile_velocity(p.speed, angle, p.gravity, t);
            velocity_arrow(painter, view, size, (x, draw_y), velocity, radius, p.accent);
        }
    }
    let (t, landed) = projectile_time(p, p.angle_deg, sim_time);
    let (x, y) = analytic::projectile_position(p.speed, p.angle_deg, p.gravity, t);
    readout_lines(
        painter,
        &[if landed {
            format!("Range = {x:.1} m")
        } else {
            format!("Height = {:.1} m", y.max(0.0))
        }],
        font,
    );
    time_readout(painter, size, sim_time, p.duration, font);
}

fn projectile_time(p: &Params, angle: f64, time: f64) -> (f64, bool) {
    match analytic::flight_time(p.speed, angle, p.gravity) {
        Some(flight) if time >= flight => (flight, true),
        _ => (time, false),
    }
}

fn ceiling_mount(painter: &mut Painter, view: &View, x: f64, half_width: f64, y: f64) {
    let a = view.point(x - half_width, y);
    let b = view.point(x + half_width, y);
    painter.line(a, b, 3.0, with_alpha(MUTED, 230));
    for k in 0..5 {
        let t = (a.x + (b.x - a.x) * k as f32 / 4.0, a.y);
        painter.line(
            Point { x: t.0, y: t.1 },
            Point {
                x: t.0 + 7.0,
                y: t.1 + 7.0,
            },
            1.5,
            with_alpha(MUTED, 160),
        );
    }
    let pivot = view.point(x, y);
    painter.fill(circle_path(pivot.x, pivot.y, 4.5), MUTED);
}

fn draw_pendulum(painter: &mut Painter, view: &View, p: &Params, sim_time: f64, size: Size) {
    let (font, _tick) = fonts(size);
    let l = p.length_m;
    let theta0 = p.angle_deg.to_radians();
    let bob_r = (0.16 * l).clamp(0.06, 0.5);
    let period = analytic::pendulum_period(l, p.gravity, p.angle_deg);
    let (theta, omega) = analytic::pendulum_state(theta0, p.gravity / l, period, sim_time);
    let (bx, by) = analytic::pendulum_bob(l, theta);
    if p.show_trail {
        let end = if sim_time >= period / 2.0 {
            -theta0
        } else {
            theta
        };
        let trace: Vec<_> = (0..=80)
            .map(|step| analytic::pendulum_bob(l, theta0 + (end - theta0) * f64::from(step) / 80.0))
            .collect();
        trail_stroke(painter, view, &trace);
    }
    ceiling_mount(painter, view, 0.0, 0.35 * l, 0.0);
    let pivot = view.point(0.0, 0.0);
    let bob = view.point(bx, by);
    painter.line(pivot, bob, 3.0, with_alpha(FOREGROUND, 230));
    painter.fill(circle_path(bob.x, bob.y, view.dist(bob_r)), p.accent);
    painter.stroke(
        circle_path(bob.x, bob.y, view.dist(bob_r)),
        1.5,
        with_alpha(FOREGROUND, 200),
    );
    if p.show_vectors {
        let (vx, vy) = analytic::pendulum_velocity(l, theta, omega);
        velocity_arrow(painter, view, size, (bx, by), (vx, vy), bob_r, p.accent);
    }
    let mut lines = Vec::new();
    if period.is_finite() {
        lines.push(format!("T = {:.2} s", period));
        lines.push(format!("angle = {:.1} deg", theta.to_degrees()));
    } else {
        lines.push("no gravity: at rest".to_string());
    }
    readout_lines(painter, &lines, font);
    time_readout(painter, size, sim_time, p.duration, font);
}

fn draw_spring(painter: &mut Painter, view: &View, p: &Params, sim_time: f64, size: Size) {
    let (font, _tick) = fonts(size);
    let l = p.length_m;
    let amplitude = 0.3 * l;
    let half = (0.12 * l).clamp(0.05, 0.6);
    let omega = analytic::spring_omega(l, p.gravity);
    let offset = analytic::spring_offset(amplitude, omega, sim_time);
    let velocity = analytic::spring_velocity(amplitude, omega, sim_time);
    let y_mass = -l - offset;
    // Equilibrium marker.
    let eq_a = view.point(-0.45 * l, -l);
    let eq_b = view.point(0.45 * l, -l);
    painter.dashed(&[eq_a, eq_b], 1.5, with_alpha(MUTED, 180), 7.0, 6.0);
    if p.show_trail && omega > 0.0 {
        let first_half = sim_time.min(std::f64::consts::PI / omega);
        let end = -l - analytic::spring_offset(amplitude, omega, first_half);
        trail_stroke(painter, view, &[(0.0, -l - amplitude), (0.0, end)]);
    }
    ceiling_mount(painter, view, 0.0, 0.3 * l, 0.0);
    // Coil: zigzag between the anchor and the top of the mass.
    let anchor = view.point(0.0, 0.0);
    let top = view.point(0.0, y_mass + half);
    let coil_points = 15;
    let width = view.dist(0.14 * l).max(6.0);
    let mut zig: Vec<Point> = Vec::with_capacity(coil_points + 2);
    zig.push(anchor);
    for k in 1..=coil_points {
        let t = k as f32 / (coil_points + 1) as f32;
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        zig.push(Point {
            x: anchor.x + side * width * 0.5,
            y: anchor.y + (top.y - anchor.y) * t,
        });
    }
    zig.push(top);
    painter.stroke(polyline(&zig, false), 2.2, with_alpha(FOREGROUND, 230));
    let corners = box_corners(0.0, y_mass, half, half, 0.0, view);
    painter.fill(polyline(&corners, true), p.accent);
    painter.stroke(polyline(&corners, true), 1.5, with_alpha(FOREGROUND, 200));
    if p.show_vectors {
        velocity_arrow(
            painter,
            view,
            size,
            (0.0, y_mass),
            (0.0, -velocity),
            half,
            p.accent,
        );
    }
    let mut lines = Vec::new();
    if omega > 0.0 {
        let period = std::f64::consts::TAU / omega;
        lines.push(format!("T = {:.2} s", period));
        lines.push(format!("Extension = {:.2} m", l + offset));
    } else {
        lines.push("no gravity: at rest".to_string());
    }
    readout_lines(painter, &lines, font);
    time_readout(painter, size, sim_time, p.duration, font);
}

impl Params {
    fn material(&self) -> sim::Material {
        sim::Material {
            gravity: self.gravity,
            restitution: self.restitution,
            friction: self.friction,
        }
    }
}

fn draw_collision(painter: &mut Painter, view: &View, p: &Params, size: Size) {
    let (font, tick) = fonts(size);
    let left = view.point(0.0, 0.0);
    let right = view.point(sim::ARENA_W, 0.0);
    painter.stroke(
        polyline(
            &[
                left,
                view.point(0.0, sim::ARENA_H),
                view.point(sim::ARENA_W, sim::ARENA_H),
                right,
            ],
            false,
        ),
        2.0,
        MUTED,
    );
    if p.show_ground {
        draw::draw_ground(painter, view, 0.0, sim::ARENA_W, 2, size, tick);
    } else {
        painter.line(left, right, 2.0, MUTED);
    }

    let plan = sim::Plan::new(p.progress, p.duration);
    let state = sim::collision(p.bodies, p.speed, p.material(), plan, p.show_trail);
    trail_stroke(painter, view, &state.trail);
    let mut px = 0.0;
    let mut py = 0.0;
    let mut ke = 0.0;
    for (index, body) in state.bodies.into_iter().enumerate() {
        px += p.mass * body.vx;
        py += p.mass * body.vy;
        ke += 0.5 * p.mass * (body.vx * body.vx + body.vy * body.vy);
        let center = view.point(body.x, body.y);
        let fill = if index == 0 { p.accent } else { FOREGROUND };
        let outline = circle_path(center.x, center.y, view.dist(sim::BALL_RADIUS));
        painter.fill(outline.clone(), fill);
        painter.stroke(outline, 1.2, MUTED);
        if p.show_vectors {
            velocity_arrow(
                painter,
                view,
                size,
                (body.x, body.y),
                (body.vx, body.vy),
                sim::BALL_RADIUS,
                fill,
            );
        }
    }
    let lines = [
        format!("Momentum = ({px:.1}, {py:.1}) kg m/s"),
        format!("Kinetic energy = {ke:.1} J"),
    ];
    readout_lines(painter, &lines, font);
    time_readout(painter, size, plan.time(), p.duration, font);
}

fn draw_stack(painter: &mut Painter, view: &View, extent: &Extent, p: &Params, size: Size) {
    let (font, tick) = fonts(size);
    if p.show_ground {
        draw::draw_ground(painter, view, extent.min_x, extent.max_x, 2, size, tick);
    }
    let plan = sim::Plan::new(p.progress, p.duration);
    let state = sim::stack(p.bodies, p.speed, p.material(), plan, p.show_trail);
    trail_stroke(painter, view, &state.trail);

    let ball = state.bodies[0];
    let center = view.point(ball.x, ball.y);
    let outline = circle_path(center.x, center.y, view.dist(sim::STACK_BALL_R));
    painter.fill(outline.clone(), p.accent);
    painter.stroke(outline, 1.2, FOREGROUND);
    let (sin, cos) = ball.angle.sin_cos();
    painter.line(
        center,
        view.point(
            ball.x + cos * sim::STACK_BALL_R * 0.7,
            ball.y + sin * sim::STACK_BALL_R * 0.7,
        ),
        1.5,
        FOREGROUND,
    );
    if p.show_vectors {
        velocity_arrow(
            painter,
            view,
            size,
            (ball.x, ball.y),
            (ball.vx, ball.vy),
            sim::STACK_BALL_R,
            p.accent,
        );
    }

    let mut fallen = 0;
    for block in &state.bodies[1..] {
        if block.angle.abs() > 0.6 {
            fallen += 1;
        }
        let corners = box_corners(
            block.x,
            block.y,
            sim::DOMINO_HX,
            sim::DOMINO_HY,
            block.angle,
            view,
        );
        let outline = polyline(&corners, true);
        painter.fill(outline.clone(), FOREGROUND);
        painter.stroke(outline, 1.2, MUTED);
    }
    readout_lines(painter, &[format!("Toppled = {fallen}/{}", p.bodies)], font);
    time_readout(painter, size, plan.time(), p.duration, font);
}

export!(PhysicsPlay);

#[cfg(test)]
mod tests;
