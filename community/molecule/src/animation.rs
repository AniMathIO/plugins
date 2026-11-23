//! Deterministic write-on timing for molecular structures and reaction sides.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn window(progress: f32, start: f32, end: f32) -> f32 {
    ((progress - start) / (end - start)).clamp(0.0, 1.0)
}

fn slot_start(i: usize, count: usize, last_start: f32) -> f32 {
    i as f32 / count.saturating_sub(1).max(1) as f32 * last_start
}

pub struct Reveal {
    pub bonds: Vec<f32>,
    pub atoms: Vec<f32>,
}

pub const REVEAL_MODES: [&str; 3] = ["bond", "atom", "together"];

pub fn structure_reveal(
    progress: f32,
    bonds: &[(usize, usize)],
    atom_count: usize,
    reveal_by: &str,
) -> Reveal {
    let mut out = Reveal {
        bonds: vec![0.0; bonds.len()],
        atoms: vec![0.0; atom_count],
    };
    match reveal_by {
        "together" => {
            out.bonds.fill(ease(window(progress, 0.0, 1.0)));
            out.atoms.fill(ease(window(progress, 0.0, 0.55)));
        }
        "atom" => {
            let start = |i| slot_start(i, atom_count, 0.53);
            for (i, atom) in out.atoms.iter_mut().enumerate() {
                *atom = ease(window(progress, start(i), start(i) + 0.24));
            }
            for ((a, b), bond) in bonds.iter().copied().zip(&mut out.bonds) {
                let ready = start(a.max(b)) + 0.20;
                *bond = ease(window(progress, ready, ready + 0.27));
            }
        }
        _ => {
            let mut first = vec![None; atom_count];
            for (i, &(a, b)) in bonds.iter().enumerate() {
                let start = slot_start(i, bonds.len(), 0.70);
                out.bonds[i] = ease(window(progress, start, start + 0.30));
                first[a].get_or_insert(start);
                first[b].get_or_insert(start);
            }
            for (atom, start) in out.atoms.iter_mut().zip(first) {
                let start = start.unwrap_or(0.0);
                *atom = ease(window(progress, start, start + 0.18));
            }
        }
    }
    if progress >= 1.0 {
        out.bonds.fill(1.0);
        out.atoms.fill(1.0);
    }
    out
}

pub const REACT_REVEAL_END: f32 = 0.42;
const ARROW_START: f32 = 0.56;
pub const PRODUCT_START: f32 = 0.74;
pub const DIMMED_ALPHA: f32 = 0.45;

pub struct ReactionPhase {
    pub reactant_progress: f32,
    pub product_progress: f32,
    pub reactant_alpha: f32,
    pub arrow: f32,
}

pub fn reaction_phase(progress: f32) -> ReactionPhase {
    let dim = ease(window(progress, REACT_REVEAL_END, ARROW_START));
    ReactionPhase {
        reactant_progress: window(progress, 0.0, REACT_REVEAL_END),
        product_progress: window(progress, PRODUCT_START, 1.0),
        reactant_alpha: 1.0 - (1.0 - DIMMED_ALPHA) * dim,
        arrow: ease(window(progress, ARROW_START, PRODUCT_START)),
    }
}
