//! `progress` → timeline mapping: which event is playing, which are done, and
//! where each packet sits on its link. Pure and continuous by construction.

/// Idle lead-in before event 0 starts (in event slots).
pub const LEAD_IN: f32 = 0.5;
/// Hold after the last event completes (in event slots).
pub const LEAD_OUT: f32 = 0.25;

/// Timeline position `u` for `progress`: event `i` occupies the slot `[i, i+1)`.
#[must_use]
pub fn position(progress: f32, events: usize) -> f32 {
    let span = events as f32 + LEAD_IN + LEAD_OUT;
    -LEAD_IN + progress.clamp(0.0, 1.0) * span
}

/// Index of the event currently in flight, `None` before the first and during
/// the final hold.
#[must_use]
pub fn playing(u: f32, events: usize) -> Option<usize> {
    if u < 0.0 {
        return None;
    }
    let index = u.floor() as usize;
    if index >= events {
        None
    } else {
        Some(index)
    }
}

/// Number of fully completed events at timeline position `u`.
#[must_use]
pub fn done(u: f32) -> usize {
    if u < 0.0 {
        0
    } else {
        u.floor() as usize
    }
}

/// Smoothstep easing within one event slot.
#[must_use]
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Eased fraction of the current event slot (0..=1), `None` when idle.
#[must_use]
pub fn phase(u: f32, events: usize) -> Option<f32> {
    playing(u, events).map(|index| ease(u - index as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_exact() {
        let u0 = position(0.0, 10);
        assert!((u0 - -LEAD_IN).abs() < 1e-6);
        let u1 = position(1.0, 10);
        assert!((u1 - (10.0 + LEAD_OUT)).abs() < 1e-6);
        assert_eq!(done(u0), 0);
        assert_eq!(done(u1), 10);
        assert_eq!(playing(u0, 10), None);
        assert_eq!(playing(u1, 10), None);
        assert_eq!(playing(position(0.9, 10), 10), Some(9));
        assert!(phase(position(0.5, 10), 10).is_some());
        assert_eq!(phase(u1, 10), None);
    }

    #[test]
    fn phases_move_forward_and_stay_in_slot() {
        for slot in 0..8_usize {
            let mut previous = 0.0_f32;
            for step in 0..20 {
                let u = slot as f32 + step as f32 / 20.0;
                let t = phase(u, 8).unwrap();
                assert!((0.0..=1.0).contains(&t));
                assert!(t >= previous - 1e-6, "phase went backwards in slot {slot}");
                previous = t;
            }
            assert_eq!(
                phase(slot as f32 + 1.0, 8),
                if slot + 1 < 8 { Some(0.0) } else { None }
            );
        }
    }
}
