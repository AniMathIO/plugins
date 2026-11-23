//! SMILES parsing, depiction data, ring perception and tetrahedral wedge
//! assignment. The chemistry engine is `chematic` 1.2.3 (MIT OR Apache-2.0),
//! used as a dependency; the wedge assignment and everything visual below is
//! original code for this plugin (see NOTICE).
use crate::params::{invalid, limit};
use chematic_core::{
    bond_order_sum, implicit_hcount, AtomIdx, BondOrder, Chirality, Molecule, STEREO_H_SENTINEL,
};
use chematic_depict::{compute_depict_data, depict_data_with_coords, DepictBondKind, DepictData};

pub const MAX_SOURCE_BYTES: usize = 512;
pub const MAX_ATOMS: usize = 128;
pub const MAX_BONDS: usize = 256;
const MAX_FRAGMENTS: usize = 8;
pub use chematic_depict::BOND_LEN as BOND_LEN_UNITS;

#[derive(Debug, PartialEq)]
pub struct Wedge {
    pub bond: usize,
    pub center: AtomIdx,
    pub solid: bool,
}

pub struct Ring {
    pub atoms: Vec<usize>,
    pub bonds: Vec<usize>,
    pub aromatic_circle: bool,
}

pub struct Fragment {
    pub mol: Molecule,
    pub depict: DepictData,
    pub wedges: Vec<Wedge>,
    pub rings: Vec<Ring>,
}

fn check_source(smiles: &str) -> Result<(), crate::Diagnostic> {
    if smiles.len() > MAX_SOURCE_BYTES {
        return Err(limit("SMILES exceeds 512 bytes; split the scene"));
    }
    Ok(())
}

fn parse(smiles: &str) -> Result<Molecule, crate::Diagnostic> {
    check_source(smiles)?;
    let limits = chematic_smiles::SmilesParseLimits {
        max_input_bytes: MAX_SOURCE_BYTES,
        max_atoms: MAX_ATOMS,
        max_bonds: MAX_BONDS,
    };
    let mol = chematic_smiles::parse_with_limits(smiles, &limits).map_err(|e| match e {
        chematic_smiles::SmilesError::ResourceLimit { .. } => {
            limit("Molecule exceeds 128 atoms or 256 bonds")
        }
        e => invalid(&format!("Could not parse SMILES: {e}")),
    })?;
    if mol.atom_count() == 0 {
        return Err(invalid("SMILES needs at least one atom"));
    }
    for (idx, atom) in mol.atoms() {
        let Some(&neutral_max) = atom.element.normal_valences().last() else {
            continue;
        };
        if atom.wildcard {
            continue;
        }
        let charge = i16::from(atom.charge);
        let atomic_number = i16::from(atom.element.atomic_number());
        let maximum = match atomic_number {
            // For B through F, formal charge changes the valence electron
            // count, and the shell can accommodate at most eight electrons.
            5..=9 => {
                let electrons = atomic_number - 2 - charge;
                electrons.min(8 - electrons)
            }
            _ => i16::from(neutral_max) + charge,
        }
        .max(0) as u16;
        let used =
            u16::from(bond_order_sum(&mol, idx)) + u16::from(atom.explicit_hcount().unwrap_or(0));
        if used > maximum {
            return Err(invalid(&format!(
                "Atom {} ({}) has valence {used}, above its maximum of {maximum}",
                idx.0 + 1,
                atom.element.symbol(),
            )));
        }
    }
    Ok(mol)
}

pub fn parse_fragment(smiles: &str) -> Result<Fragment, crate::Diagnostic> {
    parse(smiles).map(depict)
}

fn depict(mol: Molecule) -> Fragment {
    let mut depict = water_depiction(&mol).unwrap_or_else(|| compute_depict_data(&mol));
    let rings = rings(&mol, &depict);
    let wedges = assign_wedges(&mol, &depict, &rings);
    // Keep circles for isolated aromatic rings. Use alternating orders for
    // fused systems when the chemistry engine can assign them.
    let outside_circle = |i| {
        !rings
            .iter()
            .any(|r| r.aromatic_circle && r.bonds.contains(&i))
    };
    if depict
        .bonds
        .iter()
        .any(|b| b.kind == DepictBondKind::Aromatic && outside_circle(b.idx.0 as usize))
    {
        let mut kekule = mol.clone();
        if chematic_perception::kekulize_inplace(&mut kekule).is_ok() {
            for bond in &mut depict.bonds {
                if bond.kind == DepictBondKind::Aromatic && outside_circle(bond.idx.0 as usize) {
                    bond.kind = match kekule.bond(bond.idx).order {
                        BondOrder::Single => DepictBondKind::Single,
                        BondOrder::Double => DepictBondKind::Double,
                        _ => DepictBondKind::Aromatic,
                    };
                }
            }
        }
    }
    Fragment {
        mol,
        depict,
        wedges,
        rings,
    }
}

// The generic skeletal layout uses idealized organic angles. Give explicit
// water its measured 104.5° H–O–H angle (NIST CCCBDB).
fn water_depiction(mol: &Molecule) -> Option<DepictData> {
    if mol.atom_count() != 3 || mol.bond_count() != 2 {
        return None;
    }
    let (sin, cos) = 52.25_f64.to_radians().sin_cos();
    let mut x = -BOND_LEN_UNITS * sin;
    let y = -BOND_LEN_UNITS * cos;
    let mut coords = Vec::with_capacity(3);
    for (id, atom) in mol.atoms() {
        let degree = mol.degree(id);
        coords.push(match (atom.element.atomic_number(), degree, atom.charge) {
            (8, 2, 0) => (0.0, 0.0),
            (1, 1, 0) => {
                let point = (x, y);
                x = -x;
                point
            }
            _ => return None,
        });
    }
    Some(depict_data_with_coords(mol, &coords))
}

fn parse_side(smiles: &str) -> Result<Vec<Molecule>, crate::Diagnostic> {
    check_source(smiles)?;
    let parts: Vec<_> = smiles.split('.').collect();
    if parts.len() > MAX_FRAGMENTS {
        return Err(limit("A reaction side exceeds 8 fragments"));
    }
    parts.into_iter().map(parse).collect()
}

pub fn parse_reaction(
    reactants: &str,
    products: &str,
) -> Result<(Vec<Fragment>, Vec<Fragment>), crate::Diagnostic> {
    let reactants = parse_side(reactants)?;
    let products = parse_side(products)?;
    let (atoms, bonds) = reactants
        .iter()
        .chain(&products)
        .fold((0, 0), |(a, b), mol| {
            (a + mol.atom_count(), b + mol.bond_count())
        });
    if atoms > MAX_ATOMS || bonds > MAX_BONDS {
        return Err(limit("Reaction exceeds a total of 128 atoms or 256 bonds"));
    }
    Ok((
        reactants.into_iter().map(depict).collect(),
        products.into_iter().map(depict).collect(),
    ))
}

fn rings(mol: &Molecule, depict: &DepictData) -> Vec<Ring> {
    let mut rings: Vec<Ring> = chematic_perception::find_sssr(mol)
        .rings()
        .iter()
        .filter_map(|ring| {
            let bonds: Option<Vec<_>> = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .map(|(&a, &b)| mol.bond_between(a, b).map(|(idx, _)| idx.0 as usize))
                .collect();
            Some(Ring {
                atoms: ring.iter().map(|a| a.0 as usize).collect(),
                bonds: bonds?,
                aromatic_circle: false,
            })
        })
        .collect();
    rings.sort_by_key(|r| r.atoms.len());
    let mut membership = vec![0; depict.bonds.len()];
    for ring in &rings {
        for &i in &ring.bonds {
            membership[i] += 1;
        }
    }
    for ring in &mut rings {
        ring.aromatic_circle = (5..=6).contains(&ring.atoms.len())
            && ring
                .bonds
                .iter()
                .all(|&i| membership[i] == 1 && depict.bonds[i].kind == DepictBondKind::Aromatic);
    }
    rings
}

/// The implicit-H count used for labels (`CCO` ends in "OH") and lone pairs.
pub fn h_count(mol: &Molecule, idx: usize) -> u8 {
    implicit_hcount(mol, AtomIdx(idx as u32))
}

/// Tetrahedral centers become wedge/dash marks so the flat drawing conveys
/// the annotated `@`/`@@` configuration, exactly as printed depictions do.
///
/// Algorithm (original to this plugin, following the published OpenSMILES
/// tetrahedral convention):
/// 1. Ligand order `L` is `Molecule::stereo_neighbor_order` (the order the
///    parser recorded), with the bracket-H sentinel standing for the hydrogen.
/// 2. The annotation `@` (TH1) means: viewed from `L[0]` toward the center,
///    `L[1..4]` run anticlockwise. In screen coordinates (y down, like the
///    depict layout) that arrangement has positive signed determinant.
/// 3. One bond is chosen to carry the mark: a single bond, preferring exocyclic and
///    terminal ligands and then later ligands (drawn pointing outward).
/// 4. With the chosen ligand lifted to z=+1 (wedge) or z=−1 (dash), all other
///    drawn ligands in the plane and the bracket/implicit hydrogen opposite
///    the mark, the signed tetrahedron determinant must match the annotation
///    sign; the mark direction is chosen so it does.
fn assign_wedges(mol: &Molecule, depict: &DepictData, rings: &[Ring]) -> Vec<Wedge> {
    let mut wedges: Vec<Wedge> = Vec::new();
    let pos = |i: usize| -> (f64, f64) {
        let p = &depict.atoms[i].pos;
        (p.x, p.y)
    };
    for (idx, atom) in mol.atoms() {
        if !matches!(
            atom.chirality,
            Chirality::CounterClockwise | Chirality::Clockwise
        ) {
            continue;
        }
        let Some(order) = mol.stereo_neighbor_order(idx) else {
            continue;
        };
        if order.len() != 4 {
            continue;
        }
        // Annotation sign in screen coordinates (y down): TH1 (`@`) positive.
        let wanted: f64 = if atom.chirality == Chirality::CounterClockwise {
            1.0
        } else {
            -1.0
        };
        let center = idx.0 as usize;
        let center_pos = pos(center);
        // Ligand records: real atom indices plus at most one hydrogen sentinel.
        let ligands: Vec<Option<usize>> = order
            .iter()
            .map(|&v| {
                if v == STEREO_H_SENTINEL {
                    None
                } else {
                    Some(v as usize)
                }
            })
            .collect();
        // Prefer exocyclic bonds, then terminal ligands, then SMILES order.
        let candidate = ligands
            .iter()
            .enumerate()
            .filter_map(|(slot, &ligand)| {
                let ligand = ligand?;
                let (bond, entry) = mol.bond_between(idx, AtomIdx(ligand as u32))?;
                let bond = bond.0 as usize;
                if entry.order != BondOrder::Single || wedges.iter().any(|w| w.bond == bond) {
                    return None;
                }
                let outside_ring = !rings.iter().any(|ring| ring.bonds.contains(&bond));
                let terminal = mol.degree(AtomIdx(ligand as u32)) == 1;
                Some(((outside_ring, terminal, slot), bond, ligand))
            })
            .max();
        let Some((_, bond, lig)) = candidate else {
            continue;
        };
        // Signed tetrahedron determinant for a trial assignment.
        // `up` lifts the marked ligand toward the viewer (+z); the hydrogen,
        // when present, sits on the opposite side, as a flat drawing is read.
        let wedge_volume = {
            let lift = 1.0;
            // 3D positions relative to the center: drawn ligands in the plane,
            // marked ligand lifted, hydrogen exactly opposite the mark.
            let coords: Vec<(f64, f64, f64)> = ligands
                .iter()
                .map(|&l| match l {
                    Some(a) => {
                        let (x, y) = pos(a);
                        let dz = if a == lig { lift } else { 0.0 };
                        (x - center_pos.0, y - center_pos.1, dz)
                    }
                    None => (0.0, 0.0, -lift),
                })
                .collect();
            // det over (L1−L0, L2−L0, L3−L0).
            let sub = |a: usize, b: usize| -> [f64; 3] {
                [
                    coords[a].0 - coords[b].0,
                    coords[a].1 - coords[b].1,
                    coords[a].2 - coords[b].2,
                ]
            };
            let (a, b, c) = (sub(1, 0), sub(2, 0), sub(3, 0));
            a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        };
        if wedge_volume.abs() > 1e-6 {
            wedges.push(Wedge {
                bond,
                center: idx,
                solid: wedge_volume * wanted > 0.0,
            });
        }
    }
    wedges
}

/// Non-bonding electron pairs on neutral second-row/higher heteroatoms, for
/// the optional lone-pair dots. Aromatic ring atoms and anything with a
/// formal charge are skipped (their electron accounting needs perception
/// beyond a flat drawing), so the dots never state wrong chemistry.
pub fn lone_pairs(mol: &Molecule, idx: usize) -> u8 {
    let atom = mol.atom(AtomIdx(idx as u32));
    if atom.charge != 0 || atom.aromatic || atom.wildcard {
        return 0;
    }
    let valence: u16 = match atom.element.atomic_number() {
        7 | 15 => 5,           // N, P
        8 | 16 => 6,           // O, S
        9 | 17 | 35 | 53 => 7, // F, Cl, Br, I
        _ => return 0,
    };
    let mut bonded: u16 = 0;
    let mut aromatic = false;
    for (_, bond) in mol.neighbors(AtomIdx(idx as u32)) {
        match mol.bond(bond).order {
            BondOrder::Single | BondOrder::Up | BondOrder::Down => bonded += 1,
            BondOrder::Double => bonded += 2,
            BondOrder::Triple => bonded += 3,
            BondOrder::Quadruple => bonded += 4,
            BondOrder::Aromatic => aromatic = true,
            _ => return 0,
        }
    }
    if aromatic {
        return 0;
    }
    let electrons = valence.saturating_sub(bonded + u16::from(h_count(mol, idx)));
    (electrons / 2) as u8
}

/// The atom's drawn label: element symbol, optionally with implicit hydrogens
/// (`OH`, `NH2`), optionally carbons (`CH3`, or bare `C` in a skeleton).
#[derive(Clone, Copy)]
pub enum CarbonStyle {
    Bare,
    Labeled,
}

pub fn display_label(
    mol: &Molecule,
    idx: usize,
    carbons: CarbonStyle,
    hydrogens: bool,
    bond_on_right: bool,
) -> Option<String> {
    let id = AtomIdx(idx as u32);
    let atom = mol.atom(id);
    if atom.wildcard {
        return Some("*".into());
    }
    let symbol = atom.element.symbol();
    let degree = mol.degree(id);
    let carbon = atom.element.atomic_number() == 6;
    let h = h_count(mol, idx);
    let show_h = h > 0 && (!carbon || hydrogens || degree == 0);
    if carbon
        && matches!(carbons, CarbonStyle::Bare)
        && degree != 0
        && atom.charge == 0
        && atom.isotope.is_none()
        && !(show_h && degree == 1)
    {
        return None;
    }

    const SUPERSCRIPT: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    const SUBSCRIPT: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
    let mut label = String::new();
    let append_h = |label: &mut String| {
        label.push('H');
        if h > 1 {
            label.push_str(&chemical_digits(h.into(), SUBSCRIPT));
        }
    };
    // Put terminal hydrogens away from the bond. Standalone water and
    // hydronium use the molecular formulas H₂O and H₃O⁺.
    let hydrogen_first = show_h && (bond_on_right || (degree == 0 && symbol == "O" && h >= 2));
    if hydrogen_first {
        append_h(&mut label);
    }
    if let Some(isotope) = atom.isotope {
        label.push_str(&chemical_digits(isotope, SUPERSCRIPT));
    }
    label.push_str(symbol);
    if show_h && !hydrogen_first {
        append_h(&mut label);
    }
    if atom.charge != 0 {
        let magnitude = atom.charge.unsigned_abs();
        if magnitude > 1 {
            label.push_str(&chemical_digits(magnitude.into(), SUPERSCRIPT));
        }
        label.push(if atom.charge < 0 { '⁻' } else { '⁺' });
    }
    Some(label)
}

fn chemical_digits(value: u16, digits: [char; 10]) -> String {
    value
        .to_string()
        .bytes()
        .map(|b| digits[(b - b'0') as usize])
        .collect()
}
