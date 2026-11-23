//! Small fixed-step solver for the two contact demos. Each call starts at t = 0.

pub const ARENA_W: f64 = 12.0;
pub const ARENA_H: f64 = 5.0;
pub const BALL_RADIUS: f64 = 0.35;
pub const DOMINO_HX: f64 = 0.09;
pub const DOMINO_HY: f64 = 0.45;
pub const DOMINO_SPACING: f64 = 0.55;
pub const STACK_BALL_R: f64 = 0.28;

pub const HZ: f64 = 120.0;
pub const MAX_STEPS: usize = 3600;
pub const MAX_BODIES: usize = 12;
const TRAIL_INTERVAL: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub dt: f64,
    pub steps: usize,
}

impl Plan {
    pub fn new(progress: f64, duration: f64) -> Self {
        // Derive dt from duration alone to trace the same trajectory when scrubbing.
        let total = ((duration * HZ).ceil() as usize).clamp(1, MAX_STEPS);
        Self {
            dt: duration / total as f64,
            steps: (progress * total as f64).round() as usize,
        }
    }

    pub fn time(self) -> f64 {
        self.dt * self.steps as f64
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub vx: f64,
    pub vy: f64,
}

#[derive(Clone, Copy)]
pub struct Material {
    pub gravity: f64,
    pub restitution: f64,
    pub friction: f64,
}

/// Solver work counted per render, so tests can bound the fuel a scene needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Work {
    /// Visits of the disk-contact loop and of the domino transfer/support loops.
    pub pair_checks: u32,
    /// Wall/containment checks on disks.
    pub wall_checks: u32,
}

pub struct Simulation {
    pub bodies: Vec<Snapshot>,
    pub trail: Vec<(f64, f64)>,
    /// Only the tests read this; the render ignores how much work it took.
    #[cfg_attr(not(test), allow(dead_code))]
    pub work: Work,
}

pub struct Collision {
    pub motion: Simulation,
    pub boundary_impulse: bool,
}

fn record(trail: &mut Vec<(f64, f64)>, body: Snapshot, step: usize, plan: Plan, enabled: bool) {
    if enabled && (step == plan.steps || step.is_multiple_of(TRAIL_INTERVAL)) {
        trail.push((body.x, body.y));
    }
}

/// Advance the equal-mass disks, resolving walls and then all disk pairs in
/// a fixed order. The first row starts on the floor along the launch line.
pub fn collision(
    count: usize,
    speed: f64,
    material: Material,
    plan: Plan,
    trace: bool,
) -> Collision {
    let mut bodies = Vec::with_capacity(count);
    bodies.push(Snapshot {
        x: 1.4,
        y: BALL_RADIUS,
        vx: speed,
        ..Snapshot::default()
    });
    for i in 0..count - 1 {
        bodies.push(Snapshot {
            x: 5.4 + 0.85 * (i % 6) as f64,
            y: BALL_RADIUS + 1.15 * (i / 6) as f64,
            ..Snapshot::default()
        });
    }

    let mut trail = Vec::new();
    let mut work = Work::default();
    let mut boundary_impulse = false;
    record(&mut trail, bodies[0], 0, plan, trace);
    for step in 1..=plan.steps {
        for body in &mut bodies {
            body.x += body.vx * plan.dt;
            body.y += (body.vy - 0.5 * material.gravity * plan.dt) * plan.dt;
            body.vy -= material.gravity * plan.dt;
            boundary_impulse |= contain(body, material);
            work.wall_checks += 1;
        }
        for i in 1..bodies.len() {
            let (earlier, later) = bodies.split_at_mut(i);
            for a in earlier {
                contact(a, &mut later[0], material);
                work.pair_checks += 1;
            }
        }
        // A pair's penetration correction can move a disk across a wall.
        for body in &mut bodies {
            boundary_impulse |= contain(body, material);
            work.wall_checks += 1;
        }
        record(&mut trail, bodies[0], step, plan, trace);
    }
    Collision {
        motion: Simulation {
            bodies,
            trail,
            work,
        },
        boundary_impulse,
    }
}

fn contain(body: &mut Snapshot, material: Material) -> bool {
    let side = bound_axis(
        &mut body.x,
        &mut body.vx,
        &mut body.vy,
        ARENA_W,
        material,
        0.0,
    );
    // Treat very slow vertical bounces as resting contact to suppress chatter.
    let vertical = bound_axis(
        &mut body.y,
        &mut body.vy,
        &mut body.vx,
        ARENA_H,
        material,
        0.5,
    );
    side || vertical
}

fn bound_axis(
    position: &mut f64,
    normal_speed: &mut f64,
    tangent_speed: &mut f64,
    length: f64,
    material: Material,
    rest_speed: f64,
) -> bool {
    let min = BALL_RADIUS;
    let max = length - BALL_RADIUS;
    if (*position >= min || *normal_speed >= 0.0) && (*position <= max || *normal_speed <= 0.0) {
        *position = position.clamp(min, max);
        return false;
    }
    *position = position.clamp(min, max);
    let before = *normal_speed;
    *normal_speed = if before.abs() < rest_speed {
        0.0
    } else {
        -material.restitution * before
    };
    let friction = material.friction * (*normal_speed - before).abs();
    *tangent_speed -= tangent_speed.clamp(-friction, friction);
    true
}

fn contact(a: &mut Snapshot, b: &mut Snapshot, material: Material) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let distance_sq = dx * dx + dy * dy;
    let diameter = 2.0 * BALL_RADIUS;
    if distance_sq >= diameter * diameter {
        return;
    }
    let distance = distance_sq.sqrt();
    let (nx, ny) = if distance > 1e-9 {
        (dx / distance, dy / distance)
    } else {
        (1.0, 0.0)
    };
    let correction = 0.5 * (diameter - distance);
    a.x -= correction * nx;
    a.y -= correction * ny;
    b.x += correction * nx;
    b.y += correction * ny;

    let vx = b.vx - a.vx;
    let vy = b.vy - a.vy;
    let closing = vx * nx + vy * ny;
    if closing >= 0.0 {
        return;
    }
    let normal = -0.5 * (1.0 + material.restitution) * closing;
    let friction = material.friction * normal;
    let tangent = (0.5 * (vx * ny - vy * nx)).clamp(-friction, friction);
    let impulse_x = normal * nx - tangent * ny;
    let impulse_y = normal * ny + tangent * nx;
    a.vx -= impulse_x;
    a.vy -= impulse_y;
    b.vx += impulse_x;
    b.vy += impulse_y;
}

const FIRST_DOMINO: f64 = 0.7;
const BALL_START: f64 = -1.6;
const BLOCK_WIDTH: f64 = 2.0 * DOMINO_HX;
const BLOCK_HEIGHT: f64 = 2.0 * DOMINO_HY;

#[derive(Clone, Copy, Default)]
struct Domino {
    angle: f64,
    omega: f64,
    hit_next: bool,
    next_angle: f64,
    support_angle: f64,
}

impl Domino {
    fn snapshot(self, index: usize) -> Snapshot {
        let (sin, cos) = self.angle.sin_cos();
        let pivot = FIRST_DOMINO + DOMINO_SPACING * index as f64 + DOMINO_HX;
        Snapshot {
            x: pivot - DOMINO_HX * cos + DOMINO_HY * sin,
            y: DOMINO_HX * sin + DOMINO_HY * cos,
            angle: -self.angle,
            ..Snapshot::default()
        }
    }
}

/// The rolling ball hits the first block; each block rotates about its floor
/// corner and transfers an angular impulse at first contact with the next block.
pub fn stack(count: usize, speed: f64, material: Material, plan: Plan, trace: bool) -> Simulation {
    let mut ball = Snapshot {
        x: BALL_START,
        y: STACK_BALL_R,
        vx: speed,
        ..Snapshot::default()
    };
    let ball_contact = FIRST_DOMINO - DOMINO_HX - STACK_BALL_R;
    let hit_angle = ((DOMINO_SPACING - BLOCK_WIDTH) / BLOCK_HEIGHT).asin();
    let mut blocks = vec![
        Domino {
            support_angle: hit_angle,
            ..Domino::default()
        };
        count
    ];
    let mut trail = Vec::new();
    let transfer = 0.5 * (1.0 + material.restitution);
    // Moment of inertia about a corner, divided by mass.
    let inertia = (BLOCK_WIDTH * BLOCK_WIDTH + BLOCK_HEIGHT * BLOCK_HEIGHT) / 3.0;
    let flat = std::f64::consts::FRAC_PI_2;
    let slowdown = 0.05 * material.friction * material.gravity * plan.dt;
    let mut launched = false;
    record(&mut trail, ball, 0, plan, trace);
    let mut work = Work::default();
    for step in 1..=plan.steps {
        ball.vx -= ball.vx.clamp(-slowdown, slowdown);
        ball.x += ball.vx * plan.dt;
        if !launched && ball.x >= ball_contact {
            // Use restitution at the point of contact with the pivoted block.
            let arm = STACK_BALL_R;
            let impulse = (1.0 + material.restitution) * ball.vx / (1.0 + arm * arm / inertia);
            blocks[0].omega = impulse * arm / inertia;
            ball.x = ball_contact;
            ball.vx -= impulse;
            launched = true;
        }
        ball.angle = -(ball.x - BALL_START) / STACK_BALL_R;

        for block in &mut blocks {
            if block.angle == flat || (block.angle == 0.0 && block.omega == 0.0) {
                continue;
            }
            let (sin, cos) = block.angle.sin_cos();
            block.omega +=
                material.gravity * (DOMINO_HY * sin - DOMINO_HX * cos) / inertia * plan.dt;
            block.angle += block.omega * plan.dt;
            if block.angle < 0.0 || block.angle >= flat {
                block.angle = block.angle.clamp(0.0, flat);
                block.omega = 0.0;
            }
        }
        for i in 1..blocks.len() {
            let (earlier, later) = blocks.split_at_mut(i);
            let (block, next) = (&mut earlier[i - 1], &mut later[0]);
            work.pair_checks += 1;
            if !block.hit_next && block.angle >= hit_angle {
                // Keep enough information to transfer an impact even if the
                // proposed angle crossed the contact in a single step.
                let incoming = block.omega;
                block.angle = hit_angle;
                next.omega = transfer * incoming;
                block.omega -= next.omega;
                block.hit_next = true;
            }
        }
        // A leaning block rests against the next block until it tips farther.
        // Walk backward so the next block is already in its supported position.
        for i in (1..blocks.len()).rev() {
            let next = blocks[i].angle;
            let block = &mut blocks[i - 1];
            work.pair_checks += 1;
            if block.angle <= hit_angle {
                continue;
            }
            // A settled neighbor offers the same support on every step.
            if next != block.next_angle {
                block.next_angle = next;
                // An upright block first meets our top-right corner at hit_angle.
                block.support_angle = support_limit(next).max(hit_angle);
            }
            if block.angle > block.support_angle {
                block.angle = block.support_angle;
                block.omega = 0.0;
            }
        }
        record(&mut trail, ball, step, plan, trace);
    }

    let bodies = std::iter::once(ball)
        .chain(
            blocks
                .into_iter()
                .enumerate()
                .map(|(i, block)| block.snapshot(i)),
        )
        .collect();
    Simulation {
        bodies,
        trail,
        work,
    }
}

fn support_limit(next: f64) -> f64 {
    let (sin, cos) = next.sin_cos();
    // First vertex/face contact: our top-right corner on the next left face.
    let left_face = next + ((DOMINO_SPACING * cos - BLOCK_WIDTH) / BLOCK_HEIGHT).asin();

    // Opposite contact: either end of that face can reach our right face.
    // The lower endpoint becomes the near end of the top edge once flat.
    let left_x = DOMINO_SPACING - BLOCK_WIDTH * cos;
    let left_y = BLOCK_WIDTH * sin;
    let bottom_left = left_x.atan2(left_y);
    let top_left = (left_x + BLOCK_HEIGHT * sin).atan2(left_y + BLOCK_HEIGHT * cos);
    left_face.min(bottom_left).min(top_left)
}
