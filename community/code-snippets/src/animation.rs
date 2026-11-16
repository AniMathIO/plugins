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
/// At most 513² u16 cells (inputs are capped at 512 tokens before this call).
pub fn matches(from: &[Token], to: &[Token]) -> Vec<Option<usize>> {
    let width = to.len() + 1;
    let mut lengths = vec![0u16; (from.len() + 1) * width];
    for i in (0..from.len()).rev() {
        for j in (0..to.len()).rev() {
            lengths[i * width + j] = if from[i].text == to[j].text {
                lengths[(i + 1) * width + j + 1] + 1
            } else {
                lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
            };
        }
    }
    let mut result = vec![None; from.len()];
    let (mut i, mut j) = (0, 0);
    while i < from.len() && j < to.len() {
        if from[i].text == to[j].text {
            result[i] = Some(j);
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * width + j] >= lengths[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    // Preserve moved identifiers outside the LCS, one-to-one, in source order.
    let mut used = vec![false; to.len()];
    for j in result.iter().flatten() {
        used[*j] = true;
    }
    for (i, token) in from.iter().enumerate() {
        if result[i].is_none()
            && token.text.chars().count() >= 3
            && token.text.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
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
