//! Closed-form and fixed-step deterministic motion. Everything here is a pure
//! function of its arguments: the same parameters and time always produce the
//! same state, which is what makes `render_clip` deterministic.

const PENDULUM_HZ: f64 = 240.0;

/// Position of a projectile launched from the origin (metres; ground at y = 0)
/// with speed `v` at `angle_deg` under gravity `g`, at time `t` seconds.
pub fn projectile_position(v: f64, angle_deg: f64, g: f64, t: f64) -> (f64, f64) {
    let a = angle_deg.to_radians();
    (v * a.cos() * t, v * a.sin() * t - 0.5 * g * t * t)
}

/// Velocity of the same projectile at time `t` (metres per second).
pub fn projectile_velocity(v: f64, angle_deg: f64, g: f64, t: f64) -> (f64, f64) {
    let a = angle_deg.to_radians();
    (v * a.cos(), v * a.sin() - g * t)
}

/// Total flight time until the projectile returns to its launch height, or `None`
/// without gravity. Launching horizontally from the ground lands immediately.
pub fn flight_time(v: f64, angle_deg: f64, g: f64) -> Option<f64> {
    if g <= 0.0 {
        return None;
    }
    let upward = v * angle_deg.to_radians().sin();
    Some(2.0 * upward.max(0.0) / g)
}

/// Complete elliptic integral of the first kind K(m) (m = k²), computed with the
/// arithmetic-geometric mean, which converges quadratically. Deterministic and
/// accurate to double precision in well under 24 iterations.
pub fn elliptic_k(m: f64) -> f64 {
    let m = m.clamp(0.0, 0.999_999);
    let (mut a, mut b) = (1.0_f64, (1.0 - m).sqrt());
    for _ in 0..24 {
        let next = 0.5 * (a + b);
        b = (a * b).sqrt();
        a = next;
    }
    std::f64::consts::FRAC_PI_2 / a
}

/// Exact pendulum period for a release from rest at `theta0_deg`:
/// T = 4·K(sin²(θ₀/2))·√(L/g). Reduces to the small-angle 2π√(L/g) as θ₀ → 0.
pub fn pendulum_period(length_m: f64, g: f64, theta0_deg: f64) -> f64 {
    if g <= 0.0 || length_m <= 0.0 {
        return f64::INFINITY;
    }
    let m = (0.5 * theta0_deg.to_radians()).sin().powi(2);
    4.0 * elliptic_k(m) * (length_m / g).sqrt()
}

/// One classic RK4 step of θ'' = −(g/L)·sinθ over `h` seconds.
fn pendulum_rk4(theta: f64, omega: f64, g_over_l: f64, h: f64) -> (f64, f64) {
    let acc = |th: f64| -g_over_l * th.sin();
    let k1t = omega;
    let k1w = acc(theta);
    let k2t = omega + 0.5 * h * k1w;
    let k2w = acc(theta + 0.5 * h * k1t);
    let k3t = omega + 0.5 * h * k2w;
    let k3w = acc(theta + 0.5 * h * k2t);
    let k4t = omega + h * k3w;
    let k4w = acc(theta + h * k3t);
    (
        theta + h / 6.0 * (k1t + 2.0 * k2t + 2.0 * k3t + k4t),
        omega + h / 6.0 * (k1w + 2.0 * k2w + 2.0 * k3w + k4w),
    )
}

/// Use the period and symmetry to integrate at most one quarter swing. RK4
/// resolves the nonlinear motion and the last step reaches the requested time.
pub fn pendulum_state(theta0: f64, g_over_l: f64, period: f64, time: f64) -> (f64, f64) {
    if !period.is_finite() || theta0 == 0.0 {
        return (theta0, 0.0);
    }
    let phase = time % period;
    let half = period * 0.5;
    let quarter = period * 0.25;
    let (elapsed, angle_sign, speed_sign) = if phase < quarter {
        (phase, 1.0, 1.0)
    } else if phase < half {
        (half - phase, -1.0, 1.0)
    } else if phase < half + quarter {
        (phase - half, -1.0, -1.0)
    } else {
        (period - phase, 1.0, -1.0)
    };
    let steps = (elapsed * PENDULUM_HZ).ceil() as usize;
    let dt = elapsed / steps.max(1) as f64;
    let (mut theta, mut omega) = (theta0, 0.0);
    for _ in 0..steps {
        (theta, omega) = pendulum_rk4(theta, omega, g_over_l, dt);
    }
    (angle_sign * theta, speed_sign * omega)
}

/// Bob position for a pivot at the origin (metres; y up, down is negative).
pub fn pendulum_bob(length_m: f64, theta: f64) -> (f64, f64) {
    (length_m * theta.sin(), -length_m * theta.cos())
}

/// Bob velocity for angular state (θ, ω): the tangential vector ω·L.
pub fn pendulum_velocity(length_m: f64, theta: f64, omega: f64) -> (f64, f64) {
    (
        omega * length_m * theta.cos(),
        omega * length_m * theta.sin(),
    )
}

/// Mechanical energy per unit mass of the pendulum (J/kg): ½(L·ω)² + g·L·(1−cos θ).
/// Test helper for the RK4 energy-conservation check.
#[cfg(test)]
pub fn pendulum_energy(length_m: f64, g: f64, theta: f64, omega: f64) -> f64 {
    0.5 * (length_m * omega).powi(2) + g * length_m * (1.0 - theta.cos())
}

/// Undamped spring-mass offset from equilibrium, released from rest `amplitude`
/// metres below equilibrium: A·cos(ωt) (positive = further down).
pub fn spring_offset(amplitude: f64, omega: f64, t: f64) -> f64 {
    amplitude * (omega * t).cos()
}

/// Spring-mass velocity at time `t` (metres per second, positive = down).
pub fn spring_velocity(amplitude: f64, omega: f64, t: f64) -> f64 {
    -amplitude * omega * (omega * t).sin()
}

/// Undamped spring angular frequency for the tuned spring: ω = √(g/L). The spring
/// constant is chosen so the static stretch equals `length_m`, which makes the
/// spring period equal the small-angle pendulum period of the same length.
pub fn spring_omega(length_m: f64, g: f64) -> f64 {
    if g <= 0.0 || length_m <= 0.0 {
        return 0.0;
    }
    (g / length_m).sqrt()
}
