//! Molecule Studio: deterministic self-drawing 2D structures and reactions
//! from SMILES. Output is a pure function of the parameters (no wall clock,
//! no randomness, no retained state); see NOTICE for credits.
wit_bindgen::generate!({ path: "wit", world: "source-plugin" });
use animathio::plugin::types::*;

mod animation;
mod draw;
mod layout;
mod molecule;
mod params;

use std::collections::BTreeMap;

use animation::{reaction_phase, structure_reveal};
use chematic_depict::DepictBondKind;
use draw::{prepare_label, toward_sign, with_alpha, BondStroke, Label, Painter};
use layout::{reaction_layout, structure_layout, Bounds, Pt, Transform};
use molecule::{
    display_label, lone_pairs, parse_fragment, parse_reaction, CarbonStyle, Fragment,
    BOND_LEN_UNITS,
};
use params::{
    bool_param, boolean, choice, color, color_param, enum_param, highlights, invalid, limit,
    num_param, number, string, text_param, Highlights,
};

const DEFAULT_CAFFEINE: &str = "CN1C=NC2=C1C(=O)N(C)C(=O)N2C";
const PRESETS: [&str; 10] = [
    "custom",
    "water",
    "benzene",
    "caffeine",
    "aspirin",
    "ethanol",
    "glucose",
    "esterification",
    "aspirin-synthesis",
    "ethanol-combustion",
];

fn preset_inputs(preset: &str) -> (&'static str, Option<&'static str>) {
    match preset {
        "water" => ("[H]O[H]", None),
        "benzene" => ("c1ccccc1", None),
        "aspirin" => ("CC(=O)Oc1ccccc1C(=O)O", None),
        "ethanol" => ("CCO", None),
        // alpha-D-glucopyranose: shows wedge/dash stereo from SMILES chirality
        "glucose" => ("OC[C@H]1O[C@H](O)[C@H](O)[C@@H](O)[C@@H]1O", None),
        // acetic acid + ethanol -> ethyl acetate + water
        "esterification" => ("CC(O)=O.CCO", Some("CCOC(C)=O.O")),
        // salicylic acid + acetic anhydride -> aspirin + acetic acid
        "aspirin-synthesis" => (
            "OC(=O)c1ccccc1O.CC(=O)OC(C)=O",
            Some("CC(=O)Oc1ccccc1C(=O)O.CC(O)=O"),
        ),
        // C2H5OH + 3 O2 -> 2 CO2 + 3 H2O
        "ethanol-combustion" => ("CCO.O=O.O=O.O=O", Some("O=C=O.O=C=O.O.O.O")),
        _ => (DEFAULT_CAFFEINE, None),
    }
}

struct MoleculeStudio;

impl Guest for MoleculeStudio {
    fn describe() -> Manifest {
        Manifest {
            name: "Molecule Studio".into(),
            version: "0.1.0".into(),
            description:
                "Self-drawing 2D molecular structures and reactions from SMILES, with CPK colors and wedge/dash stereo"
                    .into(),
            kinds: vec!["clip-source".into()],
            metadata: vec![
                (
                    "param.smiles.description".into(),
                    "One SMILES string; use reaction mode (or >>) for reactions".into(),
                ),
                (
                    "param.progress.description".into(),
                    "Keyframe 0 -> 1 to draw the structure in".into(),
                ),
                ("param.highlight.description".into(), "Atoms 1,4-6 and bonds b0,b2-3".into()),
                ("param.stroke_width.unit".into(), "% of bond length".into()),
                ("param.font_size.unit".into(), "% of bond length".into()),
            ],
            params: vec![
                enum_param("preset", "Preset", &PRESETS, "custom"),
                text_param("smiles", "SMILES", DEFAULT_CAFFEINE),
                text_param("product_smiles", "Product SMILES", ""),
                enum_param("mode", "Mode", &["structure", "reaction"], "structure"),
                num_param("progress", "Progress", 0.0, 1.0, 1.0),
                enum_param("reveal_by", "Reveal by", &["bond", "atom", "together"], "bond"),
                bool_param("cpk", "CPK colors", true),
                enum_param("show_carbons", "Carbons", &["bare", "labeled"], "bare"),
                bool_param("show_hydrogens", "Hydrogens on carbon labels", false),
                bool_param("show_lone_pairs", "Lone pairs", false),
                bool_param("atom_numbers", "Atom numbers", false),
                text_param("highlight", "Highlight (1,4-6, b0)", ""),
                enum_param(
                    "bond_order_animation",
                    "Double bonds",
                    &["inner-line", "double-stroke"],
                    "inner-line",
                ),
                num_param("stroke_width", "Stroke width (%)", 2.0, 16.0, 5.0),
                num_param("font_size", "Label size (%)", 30.0, 85.0, 49.0),
                text_param("arrow_label", "Arrow label", ""),
                color_param("ink", "Bond and text color", INK),
                color_param(
                    "accent",
                    "Accent",
                    Color {
                        r: 255,
                        g: 158,
                        b: 40,
                        a: 255,
                    },
                ),
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

struct Config<'a> {
    progress: f32,
    reveal_by: &'a str,
    cpk: bool,
    carbons: CarbonStyle,
    hydrogens: bool,
    lone_pairs: bool,
    atom_numbers: bool,
    highlights: Highlights,
    bond_style: &'a str,
    stroke_fraction: f32,
    font_fraction: f32,
    arrow_label: &'a str,
    accent: Color,
    ink: Color,
}

const INK: Color = Color {
    r: 32,
    g: 41,
    b: 56,
    a: 255,
};

fn inputs<'a>(
    params: &'a [ParamEntry],
    preset: &str,
) -> Result<(&'a str, Option<&'a str>), Diagnostic> {
    // The editor may store the default custom fields in the clip. A named
    // preset always supplies its own inputs.
    if preset != "custom" {
        return Ok(preset_inputs(preset));
    }
    let smiles = string(params, "smiles", DEFAULT_CAFFEINE);
    if smiles.len() > molecule::MAX_SOURCE_BYTES {
        return Err(limit("SMILES exceeds 512 bytes; split the scene"));
    }
    if let Some((left, right)) = smiles.split_once(">>") {
        if right.contains('>') || left.contains('>') {
            return Err(invalid(
                "Use one >> separator between reactants and products",
            ));
        }
        return Ok((left, Some(right)));
    }
    let mode = choice(params, "mode", &["structure", "reaction"], "structure")?;
    let products = string(params, "product_smiles", "");
    Ok((smiles, (mode == "reaction").then_some(products)))
}

fn render(params: &[ParamEntry], size: Size) -> Result<Scene, Diagnostic> {
    if size.width == 0 || size.height == 0 {
        return Err(invalid("Canvas dimensions must be positive"));
    }
    let preset = choice(params, "preset", &PRESETS, "custom")?;
    let (smiles, products) = inputs(params, preset)?;
    let carbons = match choice(params, "show_carbons", &["bare", "labeled"], "bare")? {
        "labeled" => CarbonStyle::Labeled,
        _ => CarbonStyle::Bare,
    };
    let arrow_label = string(params, "arrow_label", "");
    if arrow_label.len() > 128 {
        return Err(limit("Arrow label exceeds 128 bytes"));
    }
    let config = Config {
        progress: number(params, "progress", 1.0, 0.0, 1.0)?,
        reveal_by: choice(params, "reveal_by", &animation::REVEAL_MODES, "bond")?,
        cpk: boolean(params, "cpk", true),
        carbons,
        hydrogens: boolean(params, "show_hydrogens", false),
        lone_pairs: boolean(params, "show_lone_pairs", false),
        atom_numbers: boolean(params, "atom_numbers", false),
        highlights: highlights(string(params, "highlight", ""))?,
        bond_style: choice(
            params,
            "bond_order_animation",
            &["inner-line", "double-stroke"],
            "inner-line",
        )?,
        stroke_fraction: number(params, "stroke_width", 5.0, 2.0, 16.0)? / 100.0,
        font_fraction: number(params, "font_size", 49.0, 30.0, 85.0)? / 100.0,
        arrow_label,
        accent: color(
            params,
            "accent",
            Color {
                r: 255,
                g: 158,
                b: 40,
                a: 255,
            },
        ),
        ink: color(params, "ink", INK),
    };
    let mut painter = Painter::new();
    let mut glyphs = Glyphs::default();
    if let Some(products) = products {
        if smiles.trim().is_empty() || products.trim().is_empty() {
            return Err(invalid(
                "Reaction mode needs both reactant and product SMILES",
            ));
        }
        let (reactants, products) = parse_reaction(smiles, products)?;
        let left: Vec<_> = reactants
            .iter()
            .map(|f| Prepared::new(f, &config, &mut glyphs))
            .collect();
        let right: Vec<_> = products
            .iter()
            .map(|f| Prepared::new(f, &config, &mut glyphs))
            .collect();
        render_reaction(&config, &left, &right, &glyphs, &mut painter, size);
    } else {
        let fragment = parse_fragment(smiles)?;
        let molecule = Prepared::new(&fragment, &config, &mut glyphs);
        let transform = structure_layout(molecule.bounds, size);
        molecule.draw(
            &config,
            &glyphs,
            transform,
            &mut painter,
            config.progress,
            1.0,
        );
    }
    Ok(Scene {
        coordinate_space: CoordinateSpace::Canvas,
        elements: painter.elements,
    })
}

#[derive(Default)]
struct Glyphs {
    atoms: BTreeMap<String, Option<Label>>,
    numbers: BTreeMap<usize, Option<Label>>,
}

struct Prepared<'a> {
    fragment: &'a Fragment,
    positions: Vec<Pt>,
    labels: Vec<Option<String>>,
    number_positions: Vec<Pt>,
    bounds: Bounds,
}

impl<'a> Prepared<'a> {
    fn new(fragment: &'a Fragment, config: &Config, glyphs: &mut Glyphs) -> Self {
        let bond = BOND_LEN_UNITS as f32;
        let em = bond * config.font_fraction;
        let stroke = bond * config.stroke_fraction;
        let positions: Vec<Pt> = fragment
            .depict
            .atoms
            .iter()
            .map(|a| (a.pos.x as f32, a.pos.y as f32))
            .collect();
        let labels: Vec<_> = (0..positions.len())
            .map(|i| {
                let atom = chematic_core::AtomIdx(i as u32);
                let bond_on_right = fragment.mol.degree(atom) == 1
                    && fragment.mol.neighbors(atom).any(|(other, _)| {
                        positions[other.0 as usize].0 - positions[i].0 > bond * 0.1
                    });
                let text = display_label(
                    &fragment.mol,
                    i,
                    config.carbons,
                    config.hydrogens,
                    bond_on_right,
                )?;
                glyphs
                    .atoms
                    .entry(text.clone())
                    .or_insert_with(|| prepare_label(&text, em));
                Some(text)
            })
            .collect();
        let center = centroid(positions.iter().copied(), positions.len());
        let mut number_positions = Vec::new();
        let bounds = positions
            .iter()
            .zip(&labels)
            .enumerate()
            .map(|(i, (&pos, text))| {
                let mut bounds = Bounds::around(pos, stroke * 2.5, stroke * 2.5);
                let label = text
                    .as_ref()
                    .and_then(|s| glyphs.atoms.get(s))
                    .and_then(Option::as_ref);
                if let Some(label) = label {
                    bounds = bounds.union(Bounds::around(pos, label.half_w, label.half_h));
                }
                if config.highlights.atoms.binary_search(&i).is_ok() {
                    bounds = bounds.union(Bounds::around(pos, stroke * 4.5, stroke * 4.5));
                }
                if config.lone_pairs && lone_pairs(&fragment.mol, i) > 0 {
                    let radius = em * 0.84 + stroke * 6.0;
                    bounds = bounds.union(Bounds::around(pos, radius, radius));
                }
                if config.atom_numbers {
                    let number = (i + 1).to_string();
                    let radius = label.map_or(0.0, |l| l.half_w.hypot(l.half_h));
                    let clearance = (stroke * 2.0).max(radius) + em * 0.53;
                    let neighbors = fragment
                        .mol
                        .neighbors(chematic_core::AtomIdx(i as u32))
                        .map(|(other, _)| positions[other.0 as usize]);
                    let position = number_position(pos, center, clearance, neighbors);
                    number_positions.push(position);
                    if let Some(label) = glyphs
                        .numbers
                        .entry(i)
                        .or_insert_with(|| prepare_label(&number, em * 0.42))
                    {
                        bounds = bounds.union(Bounds::around(position, label.half_w, label.half_h));
                    }
                }
                bounds
            })
            .reduce(Bounds::union)
            .unwrap();
        Self {
            fragment,
            positions,
            labels,
            number_positions,
            bounds,
        }
    }

    fn draw(
        &self,
        config: &Config,
        glyphs: &Glyphs,
        tx: Transform,
        painter: &mut Painter,
        progress: f32,
        alpha: f32,
    ) {
        if progress <= 0.0 {
            return;
        }
        let fragment = self.fragment;
        let bonds: Vec<_> = fragment
            .depict
            .bonds
            .iter()
            .map(|b| (b.atom1.0 as usize, b.atom2.0 as usize))
            .collect();
        let reveal = structure_reveal(progress, &bonds, self.positions.len(), config.reveal_by);
        let positions: Vec<_> = self.positions.iter().map(|&p| tx.apply(p)).collect();
        let bond_px = tx.bond_px();
        let em = bond_px * config.font_fraction;
        let width = bond_px * config.stroke_fraction;
        let ink = with_alpha(config.ink, alpha);
        let labels: Vec<_> = self
            .labels
            .iter()
            .map(|text| {
                text.as_ref()
                    .and_then(|s| glyphs.atoms.get(s))
                    .and_then(Option::as_ref)
            })
            .collect();
        let accent = with_alpha(config.accent, alpha * 0.33);
        for &i in &config.highlights.atoms {
            if let Some(&position) = positions.get(i) {
                painter.disk(position, width * 4.5, with_alpha(accent, reveal.atoms[i]));
            }
        }
        for &i in &config.highlights.bonds {
            if let Some(&(a, b)) = bonds.get(i) {
                painter.line(
                    positions[a],
                    positions[b],
                    reveal.bonds[i],
                    width * 3.0,
                    accent,
                );
            }
        }
        for ring in fragment.rings.iter().filter(|ring| ring.aromatic_circle) {
            let progress = ring
                .bonds
                .iter()
                .map(|&i| reveal.bonds[i])
                .fold(1.0, f32::min);
            if progress <= 0.0 {
                continue;
            }
            let center = centroid(ring.atoms.iter().map(|&a| positions[a]), ring.atoms.len());
            let radius = ring
                .bonds
                .iter()
                .map(|&i| {
                    let (a, b) = bonds[i];
                    let m = draw::lerp(positions[a], positions[b], 0.5);
                    (m.0 - center.0).hypot(m.1 - center.1)
                })
                .fold(f32::INFINITY, f32::min)
                * 0.66;
            painter.circle(center, radius, progress, width, ink);
        }
        for (i, dep) in fragment.depict.bonds.iter().enumerate() {
            let t = reveal.bonds[i];
            if t <= 0.0 {
                continue;
            }
            let (a, b) = bonds[i];
            let p = positions[a];
            let q = positions[b];
            let stroke = BondStroke {
                points: [p, q],
                labels: [labels[a], labels[b]],
                scale: tx.scale,
                width,
                progress: t,
                color: ink,
            };
            if let Some(stereo) = fragment.wedges.iter().find(|w| w.bond == i) {
                painter.stereo_bond(stroke, stereo.center != dep.atom1, stereo.solid);
                continue;
            }
            let ring = fragment.rings.iter().find(|r| r.bonds.contains(&i));
            let side = ring.map_or(1.0, |r| {
                toward_sign(
                    p,
                    q,
                    centroid(r.atoms.iter().map(|&a| positions[a]), r.atoms.len()),
                )
            });
            let gap = (bond_px * 0.13).max(width * 1.45);
            match dep.kind {
                DepictBondKind::Double if config.bond_style == "inner-line" && ring.is_some() => {
                    painter.bond_line(stroke, 0.0, 0.0);
                    painter.bond_line(stroke, side * gap, 0.15);
                }
                DepictBondKind::Double => {
                    painter.bond_line(stroke, -gap * 0.5, 0.0);
                    painter.bond_line(stroke, gap * 0.5, 0.0);
                }
                DepictBondKind::Triple => {
                    for offset in [-gap, 0.0, gap] {
                        painter.bond_line(stroke, offset, 0.0);
                    }
                }
                _ => painter.bond_line(stroke, 0.0, 0.0),
            }
        }
        for (i, (&position, label)) in positions.iter().zip(labels).enumerate() {
            let visibility = reveal.atoms[i] * alpha;
            if visibility <= 0.0 {
                continue;
            }
            let atom = &fragment.mol.atom(chematic_core::AtomIdx(i as u32));
            let label_color =
                if config.cpk && !matches!(atom.element.atomic_number(), 1 | 6) && !atom.wildcard {
                    let [r, g, b] = chematic_depict::atom_color_rgb(atom.element.atomic_number());
                    Color {
                        r,
                        g,
                        b,
                        a: config.ink.a,
                    }
                } else {
                    config.ink
                };
            if config.lone_pairs && label.is_some() {
                let away = fragment
                    .mol
                    .neighbors(chematic_core::AtomIdx(i as u32))
                    .fold((0.0, 0.0), |(x, y), (other, _)| {
                        let p = positions[other.0 as usize];
                        (x + position.0 - p.0, y + position.1 - p.1)
                    });
                painter.lone_pairs(
                    position,
                    away,
                    em * 0.84,
                    lone_pairs(&fragment.mol, i),
                    width * 0.60,
                    with_alpha(label_color, visibility),
                );
            }
            if let Some(label) = label {
                painter.draw_label(
                    label,
                    position,
                    tx.scale,
                    with_alpha(label_color, visibility),
                );
            }
            if config.atom_numbers {
                if let Some(Some(label)) = glyphs.numbers.get(&i) {
                    painter.draw_label(
                        label,
                        tx.apply(self.number_positions[i]),
                        tx.scale,
                        with_alpha(config.ink, visibility * 0.72),
                    );
                }
            }
        }
    }
}

fn number_position(p: Pt, center: Pt, clearance: f32, neighbors: impl Iterator<Item = Pt>) -> Pt {
    use std::f32::consts::{FRAC_PI_2, TAU};

    let mut angles: Vec<_> = neighbors
        .filter_map(|q| {
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            (dx.hypot(dy) > 1e-3).then(|| dy.atan2(dx))
        })
        .collect();
    angles.sort_unstable_by(f32::total_cmp);
    if let Some(&first) = angles.first() {
        angles.push(first + TAU);
    }

    // Bisect the widest space between bonds. For equal gaps, prefer the
    // direction away from the rest of the molecule.
    let mut best_gap = 0.0;
    let mut best_outward = f32::NEG_INFINITY;
    let mut direction = -FRAC_PI_2;
    for pair in angles.windows(2) {
        let gap = pair[1] - pair[0];
        let midpoint = pair[0] + gap * 0.5;
        let (sin, cos) = midpoint.sin_cos();
        let outward = (p.0 - center.0) * cos + (p.1 - center.1) * sin;
        if gap > best_gap + 1e-3 || ((gap - best_gap).abs() <= 1e-3 && outward > best_outward) {
            best_gap = gap;
            best_outward = outward;
            direction = midpoint;
        }
    }
    let (sin, cos) = direction.sin_cos();
    (p.0 + cos * clearance, p.1 + sin * clearance)
}

fn centroid(points: impl Iterator<Item = Pt>, count: usize) -> Pt {
    let (x, y) = points.fold((0.0, 0.0), |(x, y), p| (x + p.0, y + p.1));
    (x / count as f32, y / count as f32)
}

fn render_reaction(
    config: &Config,
    reactants: &[Prepared<'_>],
    products: &[Prepared<'_>],
    glyphs: &Glyphs,
    painter: &mut Painter,
    size: Size,
) {
    let left: Vec<_> = reactants.iter().map(|m| m.bounds).collect();
    let right: Vec<_> = products.iter().map(|m| m.bounds).collect();
    let layout = reaction_layout(&left, &right, size);
    let phase = reaction_phase(config.progress);
    for (side, molecules, progress, alpha) in [
        (
            &layout.left,
            reactants,
            phase.reactant_progress,
            phase.reactant_alpha,
        ),
        (&layout.right, products, phase.product_progress, 1.0),
    ] {
        for (molecule, &tx) in molecules.iter().zip(&side.fragments) {
            molecule.draw(config, glyphs, tx, painter, progress, alpha);
        }
        let ink = with_alpha(config.ink, alpha * animation::ease(progress * 4.0));
        for &center in &side.pluses {
            painter.plus(center, layout.bond_px * 0.16, layout.bond_px * 0.045, ink);
        }
    }
    let (a, b) = layout.arrow;
    painter.arrow(a, b, phase.arrow, layout.bond_px * 0.064, config.accent);
    if phase.arrow > 0.0 && !config.arrow_label.trim().is_empty() {
        if let Some(label) = prepare_label(config.arrow_label, layout.bond_px * 0.42) {
            let scale = ((b.0 - a.0) * 0.48 / label.half_w.max(1.0)).min(1.0);
            let pos = ((a.0 + b.0) * 0.5, a.1 - layout.bond_px * 0.55);
            painter.draw_label(&label, pos, scale, with_alpha(config.ink, phase.arrow));
        }
    }
}

export!(MoleculeStudio);

#[cfg(test)]
mod tests;
