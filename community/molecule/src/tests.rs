//! Native unit tests for the chemistry core, layout, animation and stereo
//! parity (no host imports are touched; full renders are covered by smoke.py
//! in the wasm sandbox).
use super::animation::{reaction_phase, structure_reveal, PRODUCT_START, REACT_REVEAL_END};
use super::layout::{fit_scale, reaction_layout, structure_layout, Bounds};
use super::molecule::{
    display_label, h_count, lone_pairs, parse_fragment, parse_reaction, CarbonStyle, Fragment,
};
use super::params::highlights;
use super::{inputs, number_position, MoleculeStudio};
use crate::{Guest, ParamEntry, ParamKind, ParamValue, Size};
use chematic_core::{AtomIdx, Chirality};

fn fragment(smiles: &str) -> Fragment {
    match parse_fragment(smiles) {
        Ok(f) => f,
        Err(e) => panic!("{smiles}: {}", e.message),
    }
}

fn fragment_err(smiles: &str) -> String {
    match parse_fragment(smiles) {
        Err(d) => d.code,
        Ok(_) => panic!("{smiles} unexpectedly parsed"),
    }
}

fn bonds(fragment: &Fragment) -> Vec<(usize, usize)> {
    fragment
        .depict
        .bonds
        .iter()
        .map(|b| (b.atom1.0 as usize, b.atom2.0 as usize))
        .collect()
}

#[test]
fn parse_accepts_everyday_molecules() {
    assert_eq!(fragment("O").mol.atom_count(), 1);
    assert_eq!(fragment("CCO").mol.atom_count(), 3);
    assert_eq!(fragment("c1ccccc1").mol.atom_count(), 6);
    assert_eq!(
        fragment("CN1C=NC2=C1C(=O)N(C)C(=O)N2C").mol.atom_count(),
        14
    );
    assert_eq!(fragment("CC(=O)Oc1ccccc1C(=O)O").mol.bond_count(), 13);
    // alpha-D-glucopyranose with stereo markers
    assert_eq!(
        fragment("OC[C@H]1O[C@H](O)[C@H](O)[C@@H](O)[C@@H]1O")
            .mol
            .atom_count(),
        12
    );
}

#[test]
fn parse_rejects_bad_grammar_cleanly() {
    assert_eq!(fragment_err(""), "invalid-input");
    assert_eq!(fragment_err("."), "invalid-input");
    assert_eq!(fragment_err("c1ccccc"), "invalid-input");
    assert_eq!(fragment_err("C1CC"), "invalid-input");
    assert_eq!(fragment_err("not-a-molecule"), "invalid-input");
    assert_eq!(fragment_err("C(C"), "invalid-input");
}

#[test]
fn parsing_rejects_valence_above_the_element_ceiling() {
    for (smiles, atom, used, maximum) in [
        ("CC(C)(C)(C)(C)C", "2 (C)", 6, 4),
        ("[CH5]", "1 (C)", 5, 4),
        ("C#[CH2]", "2 (C)", 5, 4),
        ("C[NH3]C", "2 (N)", 5, 3),
        ("[NH5+]", "1 (N)", 5, 4),
        ("[OH4+]", "1 (O)", 4, 3),
    ] {
        let diagnostic = parse_fragment(smiles).err().expect(smiles);
        assert_eq!(diagnostic.code, "invalid-input", "{smiles}");
        assert_eq!(
            diagnostic.message,
            format!("Atom {atom} has valence {used}, above its maximum of {maximum}"),
            "{smiles}",
        );
    }
    let diagnostic = parse_reaction("O", "O.[CH5]").err().unwrap();
    assert_eq!(diagnostic.code, "invalid-input");
    for smiles in ["[CH3]", "C[NH3+]", "[OH3+]", "OP(=O)(O)O", "[nH]1cccc1"] {
        fragment(smiles);
    }
}

#[test]
fn caps_produce_limit_exceeded() {
    // Over the 512-byte source cap.
    assert_eq!(fragment_err(&"C".repeat(600)), "limit-exceeded");
    // Within bytes but over the atom cap.
    let long_chain = "C".repeat(200);
    assert_eq!(fragment_err(&long_chain), "limit-exceeded");
}

#[test]
fn depiction_is_deterministic() {
    for smiles in [
        "CN1C=NC2=C1C(=O)N(C)C(=O)N2C",
        "CC(=O)Oc1ccccc1C(=O)O",
        "c1ccccc1",
    ] {
        let a = fragment(smiles);
        let b = fragment(smiles);
        for (x, y) in a.depict.atoms.iter().zip(&b.depict.atoms) {
            assert_eq!(x.pos.x.to_bits(), y.pos.x.to_bits());
            assert_eq!(x.pos.y.to_bits(), y.pos.y.to_bits());
        }
    }
}

#[test]
fn reaction_parsing_splits_and_caps_fragments() {
    let (left, right) = parse_reaction("CCO.O", "CC=O.O").unwrap();
    assert_eq!(left.len(), 2);
    assert_eq!(left[0].mol.atom_count(), 3);
    assert_eq!(left[1].mol.atom_count(), 1);
    assert_eq!(right.len(), 2);
    for oversized in [format!("{}O", "O.".repeat(8)), "C".repeat(128)] {
        assert_eq!(
            parse_reaction(&oversized, "O").err().unwrap().code,
            "limit-exceeded"
        );
    }
    assert!(parse_reaction("O..O", "O").is_err());
}

#[test]
fn named_presets_override_the_editors_stored_inputs() {
    let params: Vec<_> = MoleculeStudio::describe()
        .params
        .into_iter()
        .filter_map(|spec| {
            let value = match spec.kind {
                ParamKind::String(s) => ParamValue::String(s.default),
                ParamKind::EnumChoice(e) => ParamValue::EnumChoice(e.default),
                _ => return None,
            };
            Some(ParamEntry { id: spec.id, value })
        })
        .collect();
    assert_eq!(inputs(&params, "water").unwrap(), ("[H]O[H]", None));
    assert_eq!(
        inputs(&params, "ethanol-combustion").unwrap(),
        ("CCO.O=O.O=O.O=O", Some("O=C=O.O=C=O.O.O.O"))
    );
    let shorthand = |smiles: &str| {
        [ParamEntry {
            id: "smiles".into(),
            value: ParamValue::String(smiles.into()),
        }]
    };
    assert_eq!(
        inputs(&shorthand("CCO.O>>CC=O.O"), "custom").unwrap(),
        ("CCO.O", Some("CC=O.O"))
    );
    assert!(inputs(&shorthand("O>>N>>C"), "custom").is_err());
}

#[test]
fn isolated_aromatic_rings_get_circles_but_fused_rings_keep_bond_orders() {
    let benzene = fragment("c1ccccc1");
    assert_eq!(benzene.rings.len(), 1);
    assert_eq!(benzene.rings[0].bonds.len(), 6);
    assert!(benzene.rings[0].aromatic_circle);
    let fused = fragment("c1ccc2ccccc2c1");
    assert!(fused.rings.iter().all(|r| !r.aromatic_circle));
    assert_eq!(
        fused
            .depict
            .bonds
            .iter()
            .filter(|b| b.kind == chematic_depict::DepictBondKind::Double)
            .count(),
        5
    );
    assert!(fragment("CCO").rings.is_empty());
}

#[test]
fn explicit_water_uses_the_measured_bond_angle() {
    let f = fragment("[H]O[H]");
    let a = &f.depict.atoms[0].pos;
    let o = &f.depict.atoms[1].pos;
    let b = &f.depict.atoms[2].pos;
    let left = (a.x - o.x, a.y - o.y);
    let right = (b.x - o.x, b.y - o.y);
    let dot = left.0 * right.0 + left.1 * right.1;
    let angle = (dot / (left.0.hypot(left.1) * right.0.hypot(right.1)))
        .acos()
        .to_degrees();
    assert!((angle - 104.5).abs() < 1e-6);
}

#[test]
fn box_fitting_is_uniform_and_degenerate_safe() {
    assert!((fit_scale(80.0, 40.0, 160.0, 100.0) - 2.0).abs() < 1e-6);
    assert!((fit_scale(80.0, 40.0, 80.0, 100.0) - 1.0).abs() < 1e-6);
    // Zero extents fall back to one bond length instead of exploding.
    assert!(fit_scale(0.0, 0.0, 80.0, 40.0) > 0.0);
}

#[test]
fn reaction_signs_use_box_gaps_and_the_whole_row_fits() {
    let size = Size {
        width: 1280,
        height: 720,
    };
    let wide = Bounds::around((10.0, 50.0), 80.0, 25.0);
    let narrow = Bounds::around((0.0, 0.0), 20.0, 10.0);
    let layout = reaction_layout(&[wide, narrow], &[wide], size);
    let [a, b] = layout.left.fragments[..] else {
        panic!("two reactants");
    };
    let c = layout.right.fragments[0];
    let plus = layout.left.pluses[0];
    assert_eq!(a.scale, b.scale);
    assert_eq!(b.scale, c.scale);
    assert!((a.apply(wide.max).0 + b.apply(narrow.min).0 - 2.0 * plus.0).abs() < 1e-3);
    assert!(a.apply(wide.min).0 >= 83.0);
    assert!(c.apply(wide.max).0 <= 1197.0);
    assert!(b.apply(narrow.max).0 < layout.arrow.0 .0);
    assert!(layout.arrow.1 .0 < c.apply(wide.min).0);
    let tx = structure_layout(wide, size);
    assert!((tx.apply((10.0, 50.0)).0 - 640.0).abs() < 1e-3);
    assert!((tx.apply((10.0, 50.0)).1 - 360.0).abs() < 1e-3);
}

#[test]
fn highlight_specs_parse_strictly() {
    let mixed = highlights("1,4-6,b0,b2-3,6,b3").unwrap();
    assert_eq!(mixed.atoms, [0, 3, 4, 5]);
    assert_eq!(mixed.bonds, [0, 2, 3]);
    assert!(highlights("").unwrap().atoms.is_empty());
    for bad in ["0", "5-2", "129", "b256", "b4-0", "x", "1-2-3", "1,,b0"] {
        assert!(highlights(bad).is_err(), "{bad}");
    }
}

#[test]
fn bond_reveal_hits_exact_endpoints_and_is_monotone() {
    let molecule = fragment("CC(=O)Oc1ccccc1C(=O)O");
    let bs = bonds(&molecule);
    let n_atoms = molecule.mol.atom_count();
    let at = |p: f32| structure_reveal(p, &bs, n_atoms, "bond");
    let zero = at(0.0);
    assert!(zero.bonds.iter().all(|&t| t == 0.0));
    assert!(zero.atoms.iter().all(|&t| t == 0.0));
    let one = at(1.0);
    assert!(one.bonds.iter().all(|&t| t == 1.0));
    assert!(one.atoms.iter().all(|&t| t == 1.0));
    let mut previous = (zero.bonds.clone(), zero.atoms.clone());
    for step in [0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
        let current = at(step);
        for i in 0..bs.len() {
            assert!(current.bonds[i] >= previous.0[i] - 1e-6);
        }
        for i in 0..n_atoms {
            assert!(current.atoms[i] >= previous.1[i] - 1e-6);
        }
        previous = (current.bonds.clone(), current.atoms.clone());
    }
    // With staggered windows the first bond finishes well before the last
    // one starts (windows overlap in the middle of the sequence).
    let early = at(0.35);
    assert_eq!(early.bonds[0], 1.0);
    assert_eq!(early.bonds[bs.len() - 1], 0.0);
}

#[test]
fn reveal_modes_behave() {
    let molecule = fragment("CCO");
    let bs = bonds(&molecule);
    let together = |p: f32| structure_reveal(p, &bs, 3, "together");
    assert!(together(0.0).bonds.iter().all(|&t| t == 0.0));
    assert_eq!(together(0.5).bonds[0], together(0.5).bonds[1]);
    assert!(together(1.0).bonds.iter().all(|&t| t == 1.0));
    // Atom mode: bonds only start once both endpoint atoms are mostly in.
    let by_atom = |p: f32| structure_reveal(p, &bs, 3, "atom");
    let early = by_atom(0.1);
    assert_eq!(early.bonds[0], 0.0);
    assert!(by_atom(1.0).bonds.iter().all(|&t| t == 1.0));
    for mode in ["bond", "atom", "together"] {
        let mut prev = structure_reveal(0.0, &bs, 3, mode);
        for sample in 1..=100 {
            let curr = structure_reveal(sample as f32 / 100.0, &bs, 3, mode);
            assert!(curr.bonds.iter().zip(&prev.bonds).all(|(a, b)| a >= b));
            assert!(curr.atoms.iter().zip(&prev.atoms).all(|(a, b)| a >= b));
            prev = curr;
        }
    }
}

#[test]
fn reaction_timeline_hits_exact_endpoints() {
    let at = |p: f32| reaction_phase(p);
    let zero = at(0.0);
    assert_eq!(zero.reactant_progress, 0.0);
    assert_eq!(zero.arrow, 0.0);
    assert_eq!(zero.product_progress, 0.0);
    assert_eq!(zero.reactant_alpha, 1.0);
    let one = at(1.0);
    assert_eq!(one.product_progress, 1.0);
    assert_eq!(one.arrow, 1.0);
    assert!((one.reactant_alpha - 0.70).abs() < 1e-4);
    // Reactants fully drawn before the arrow starts.
    let pre_arrow = at(REACT_REVEAL_END);
    assert_eq!(pre_arrow.reactant_progress, 1.0);
    assert_eq!(pre_arrow.arrow, 0.0);
    assert_eq!(at(PRODUCT_START - 0.01).product_progress, 0.0);
}

#[test]
fn lone_pairs_are_chemically_conservative() {
    // Water oxygen: two lone pairs.
    let water = fragment("O");
    assert_eq!(lone_pairs(&water.mol, 0), 2);
    // Ethanol oxygen: two pairs.
    let ethanol = fragment("CCO");
    assert_eq!(lone_pairs(&ethanol.mol, 2), 2);
    // Carbonyl oxygen of acetone: two pairs.
    let acetone = fragment("CC(=O)C");
    assert_eq!(lone_pairs(&acetone.mol, 2), 2);
    // Carbons get none.
    assert_eq!(lone_pairs(&ethanol.mol, 0), 0);
}

#[test]
fn implicit_h_counts_match_valence() {
    let ethanol = fragment("CCO");
    assert_eq!(h_count(&ethanol.mol, 0), 3);
    assert_eq!(h_count(&ethanol.mol, 1), 2);
    assert_eq!(h_count(&ethanol.mol, 2), 1);
    // Bracket atoms keep their declared count.
    let amine = fragment("C[NH2]");
    assert_eq!(h_count(&amine.mol, 1), 2);
}

#[test]
fn labels_follow_carbon_and_hydrogen_options() {
    let ethanol = fragment("CCO");
    let bare = |i: usize| display_label(&ethanol.mol, i, CarbonStyle::Bare, false, false);
    let shown = |i: usize| display_label(&ethanol.mol, i, CarbonStyle::Labeled, true, false);
    assert_eq!(bare(0), None); // skeletal carbon
    assert_eq!(bare(2).as_deref(), Some("OH"));
    assert_eq!(
        display_label(&ethanol.mol, 2, CarbonStyle::Bare, false, true).as_deref(),
        Some("HO")
    );
    assert_eq!(shown(0).as_deref(), Some("CH₃"));
    assert_eq!(shown(1).as_deref(), Some("CH₂"));
    assert_eq!(shown(2).as_deref(), Some("OH"));
    assert_eq!(
        display_label(&ethanol.mol, 0, CarbonStyle::Bare, true, false).as_deref(),
        Some("CH₃")
    );
    assert_eq!(
        display_label(&ethanol.mol, 1, CarbonStyle::Bare, true, false),
        None
    );
}

#[test]
fn heteroatom_hydrogens_and_superscript_charges_are_always_shown() {
    for (smiles, atom, expected) in [
        ("CCO", 2, "OH"),
        ("CC(=O)O", 2, "O"),
        ("CC(=O)O", 3, "OH"),
        ("CN", 1, "NH₂"),
        ("CNC", 1, "NH"),
        ("CS", 1, "SH"),
        ("OP(=O)(O)O", 0, "OH"),
        ("[nH]1cccc1", 0, "NH"),
        ("C[NH3+]", 1, "NH₃⁺"),
        ("O", 0, "H₂O"),
        ("[OH3+]", 0, "H₃O⁺"),
        ("CC(=O)[O-]", 3, "O⁻"),
        ("[Ca+2]", 0, "Ca²⁺"),
    ] {
        let f = fragment(smiles);
        for hydrogens in [false, true] {
            let label = display_label(&f.mol, atom, CarbonStyle::Bare, hydrogens, false);
            assert_eq!(label.as_deref(), Some(expected), "{smiles}");
        }
    }
}

#[test]
fn atom_numbers_use_the_open_space_between_bonds() {
    let p = (0.0, 0.0);
    let place = |neighbors: &[(f32, f32)], center| {
        number_position(p, center, 1.0, neighbors.iter().copied())
    };
    let (x, y) = place(&[(1.0, 0.0)], (-1.0, 0.0));
    assert!((x + 1.0).abs() < 1e-5 && y.abs() < 1e-5);
    let (x, y) = place(&[(1.0, 1.0), (1.0, -1.0)], (-1.0, 0.0));
    assert!((x + 1.0).abs() < 1e-5 && y.abs() < 1e-5);
    let (x, y) = place(&[(1.0, 0.0), (-1.0, 0.0)], (0.0, 1.0));
    assert!(x.abs() < 1e-5 && (y + 1.0).abs() < 1e-5);
}

#[test]
fn wedge_assignment_is_deterministic_and_finds_centers() {
    let a = fragment("OC[C@H]1O[C@H](O)[C@H](O)[C@@H](O)[C@@H]1O");
    let b = fragment("OC[C@H]1O[C@H](O)[C@H](O)[C@@H](O)[C@@H]1O");
    assert_eq!(a.wedges, b.wedges);
    assert_eq!(a.wedges.len(), 5); // five chiral carbons
    assert!(
        a.wedges
            .iter()
            .any(|w| a.depict.bonds[w.bond].atom2 == w.center),
        "test molecule must exercise a stereocenter stored as the second endpoint"
    );
    // Acyclic molecules without chirality get none.
    assert!(fragment("CCO").wedges.is_empty());
}

/// Apparent R/S of a drawn fragment at its chiral `center`: the signed
/// determinant over CIP-ranked ligand vectors, with the marked ligand lifted
/// toward/away from the viewer. In depict (y-down, z toward the viewer)
/// coordinates det[v1-v4, v2-v4, v3-v4] > 0 reads as R, < 0 as S.
/// `ligands[3] == None` places the implicit hydrogen opposite the wedge
/// (the flat-drawing reading convention).
fn apparent_sign(f: &Fragment, center: usize, ligands: &[Option<usize>; 4]) -> f64 {
    let pos = |i: usize| (f.depict.atoms[i].pos.x, f.depict.atoms[i].pos.y);
    let c = pos(center);
    let mark = f
        .wedges
        .iter()
        .find(|w| w.center.0 as usize == center)
        .unwrap();
    let lift = if mark.solid { 1.0 } else { -1.0 };
    let wedge_ligand = {
        let w = &f.depict.bonds[mark.bond];
        if w.atom1.0 as usize == center {
            w.atom2.0 as usize
        } else {
            w.atom1.0 as usize
        }
    };
    let v = |atom: Option<usize>| -> (f64, f64, f64) {
        match atom {
            Some(a) => {
                let p = pos(a);
                let z = if a == wedge_ligand { lift } else { 0.0 };
                (p.0 - c.0, p.1 - c.1, z)
            }
            // Implicit hydrogen: at the center in the drawing, opposite the
            // mark in depth.
            None => (0.0, 0.0, -lift),
        }
    };
    let (v1, v2, v3, v4) = (v(ligands[0]), v(ligands[1]), v(ligands[2]), v(ligands[3]));
    let sub = |a: (f64, f64, f64), b: (f64, f64, f64)| -> (f64, f64, f64) {
        (a.0 - b.0, a.1 - b.1, a.2 - b.2)
    };
    let (a, b, cc) = (sub(v1, v4), sub(v2, v4), sub(v3, v4));
    a.0 * (b.1 * cc.2 - b.2 * cc.1)
        + a.1 * (b.2 * cc.0 - b.0 * cc.2)
        + a.2 * (b.0 * cc.1 - b.1 * cc.0)
}

/// CIP ligand ranking that resolves the simple cases used in these tests:
/// atomic number first, then neighbor count (carboxyl carbon beats methyl).
/// Returns the four ligands highest first; `None` is the implicit hydrogen.
fn cip_ligands(f: &Fragment, center: usize) -> [Option<usize>; 4] {
    let mut ranked: Vec<usize> = f
        .mol
        .neighbors(AtomIdx(center as u32))
        .map(|(a, _)| a.0 as usize)
        .collect();
    ranked.sort_by(|&a, &b| {
        let atom_a = f.mol.atom(AtomIdx(a as u32));
        let atom_b = f.mol.atom(AtomIdx(b as u32));
        atom_b
            .element
            .atomic_number()
            .cmp(&atom_a.element.atomic_number())
            .then(
                f.mol
                    .degree(AtomIdx(b as u32))
                    .cmp(&f.mol.degree(AtomIdx(a as u32))),
            )
    });
    assert!(
        ranked.len() == 3 || ranked.len() == 4,
        "tetrahedral center expected"
    );
    let mut ligands: [Option<usize>; 4] = [None; 4];
    for (i, ligand) in ranked.iter().enumerate() {
        ligands[i] = Some(*ligand);
    }
    ligands
}

fn chiral_center(f: &Fragment) -> usize {
    (0..f.mol.atom_count())
        .find(|&i| {
            matches!(
                f.mol.atom(AtomIdx(i as u32)).chirality,
                Chirality::CounterClockwise | Chirality::Clockwise
            )
        })
        .expect("chiral center")
}

#[test]
fn wedges_convey_pubchem_absolute_configurations() {
    // PubChem CID 5950: (2S)-2-aminopropanoic acid, isomeric SMILES
    // "C[C@@H](C(=O)O)N". CIP order at the chiral carbon:
    // N (1) > carboxyl carbon (2) > methyl carbon (3) > H (4).
    let s_alanine = fragment("C[C@@H](C(=O)O)N");
    let r_alanine = fragment("C[C@H](C(=O)O)N");
    let cs = chiral_center(&s_alanine);
    let cr = chiral_center(&r_alanine);
    let s_ligands = cip_ligands(&s_alanine, cs);
    let r_ligands = cip_ligands(&r_alanine, cr);
    assert!(
        apparent_sign(&s_alanine, cs, &s_ligands) < 0.0,
        "(S) must read S"
    );
    assert!(
        apparent_sign(&r_alanine, cr, &r_ligands) > 0.0,
        "(R) must read R"
    );
}

#[test]
fn wedges_stay_consistent_across_opensmiles_equivalences() {
    // The six equivalent writings of one chiral center from the OpenSMILES
    // specification (section 3.8.2) must all parse to the same isomeric
    // canonical SMILES and produce the same apparent configuration; the @@
    // writings mirror it. CIP order by atomic number: Br > O > N > C.
    let at_writings = [
        "N[C@](Br)(O)C",
        "Br[C@](O)(N)C",
        "O[C@](Br)(C)N",
        "Br[C@](C)(O)N",
        "C[C@](Br)(N)O",
        "Br[C@](N)(C)O",
    ];
    let canonical = |smiles: &str| -> String {
        let mol = chematic_smiles::parse(smiles).expect(smiles);
        chematic_smiles::canonical_smiles(&mol)
    };
    let reference = canonical(at_writings[0]);
    for smiles in &at_writings[1..] {
        assert_eq!(canonical(smiles), reference, "{smiles} must be equivalent");
    }
    assert_ne!(
        canonical("N[C@@](Br)(O)C"),
        reference,
        "@@ must be the other enantiomer"
    );
    let mut signs = Vec::new();
    for smiles in &at_writings {
        let f = fragment(smiles);
        assert!(!f.wedges.is_empty(), "{smiles} needs a wedge");
        let center = chiral_center(&f);
        let ligands = cip_ligands(&f, center);
        signs.push(apparent_sign(&f, center, &ligands));
    }
    for sign in &signs[1..] {
        assert!(
            (*sign - signs[0]).abs() < 1e-9,
            "equivalent writings disagree"
        );
    }
    assert_ne!(signs[0], 0.0);
    // The @@ writing is the mirror image.
    let f = fragment("N[C@@](Br)(O)C");
    let center = chiral_center(&f);
    let ligands = cip_ligands(&f, center);
    let mirrored = apparent_sign(&f, center, &ligands);
    assert!(mirrored * signs[0] < 0.0, "@@ must mirror @");
}
