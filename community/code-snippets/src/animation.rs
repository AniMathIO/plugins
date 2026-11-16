use super::Color;
#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub column: f32,
    pub line: f32,
    pub offset: usize,
    pub color: Color,
}

/// Bounded token LCS, with stable tie-breaking for repeated punctuation.
/// At most 257² u16 cells (inputs are capped at 256 tokens before this call).
pub fn matches(from: &[Token], to: &[Token]) -> Vec<Option<usize>> {
    // Intern text once so the quadratic loop compares integers, not UTF-8 strings.
    let mut vocabulary = std::collections::BTreeMap::new();
    let mut next_id = 0u16;
    let mut ids = |tokens: &[Token]| {
        tokens
            .iter()
            .map(|t| {
                *vocabulary.entry(t.text.clone()).or_insert_with(|| {
                    let id = next_id;
                    next_id += 1;
                    id
                })
            })
            .collect::<Vec<_>>()
    };
    let from_ids = ids(from);
    let to_ids = ids(to);
    let width = to.len() + 1;
    let mut lengths = vec![0u16; (from.len() + 1) * width];
    for i in (0..from.len()).rev() {
        for j in (0..to.len()).rev() {
            lengths[i * width + j] = if from_ids[i] == to_ids[j] {
                lengths[(i + 1) * width + j + 1] + 1
            } else {
                lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
            };
        }
    }
    let mut result = vec![None; from.len()];
    let (mut i, mut j) = (0, 0);
    while i < from.len() && j < to.len() {
        if from_ids[i] == to_ids[j] {
            result[i] = Some(j);
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * width + j] >= lengths[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    // Preserve all moved tokens outside the LCS, one-to-one, in source order.
    let mut used = vec![false; to.len()];
    for j in result.iter().flatten() {
        used[*j] = true;
    }
    for (i, token) in from.iter().enumerate() {
        if result[i].is_none() {
            if let Some(j) = to
                .iter()
                .enumerate()
                .position(|(j, t)| !used[j] && t.text == token.text)
            {
                result[i] = Some(j);
                used[j] = true;
            }
        }
    }
    result
}

/// Overlap exits and entries while retained tokens ease between positions.
/// Like Magic Move's duration-based delays, these are overlapping windows,
/// not three sequential slices. Progress is normalized to the whole transition.
pub fn phases(progress: f32) -> (f32, f32, f32) {
    let travel = ((progress - 0.15) / 0.7).clamp(0.0, 1.0);
    (
        travel * travel * (3.0 - 2.0 * travel),
        (1.0 - progress / 0.6).clamp(0.0, 1.0),
        ((progress - 0.4) / 0.6).clamp(0.0, 1.0),
    )
}
