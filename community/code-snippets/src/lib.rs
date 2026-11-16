//! Deterministic code animation. Behavioral inspiration: Shiki Magic Move (MIT).
//! No upstream animation implementation is copied; see NOTICE.
wit_bindgen::generate!({ path: "wit", world: "source-plugin" });
use animathio::plugin::types::*;
use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings,
};

mod animation;
mod drawing;
use animation::{matches, Token};
use drawing::{color, Painter};

const CODE: &str = "fn main() {\n    println!(\"Hello, AniMathIO!\");\n}";
const MAX_BYTES: usize = 8192;
const MAX_CHARS: usize = 2048;
const MAX_TOKENS: usize = 512;
const MAX_LINES: usize = 80;
struct CodeSnippets;

fn syntax_set() -> SyntaxSet {
    syntect::dumps::from_uncompressed_data(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/syntaxes.bin"
    )))
    .expect("build-time syntax dump")
}
fn error(message: &str) -> Diagnostic {
    Diagnostic {
        code: "invalid-input".into(),
        message: message.into(),
        metadata: vec![],
    }
}
fn string<'a>(params: &'a [ParamEntry], id: &str, default: &'a str) -> &'a str {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match &p.value {
            ParamValue::String(s) | ParamValue::EnumChoice(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or(default)
}
fn number(
    params: &[ParamEntry],
    id: &str,
    default: f64,
    min: f64,
    max: f64,
) -> Result<f32, Diagnostic> {
    let n = params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.value {
            ParamValue::Number(n) => Some(n),
            _ => None,
        })
        .unwrap_or(default);
    if !n.is_finite() {
        return Err(error("Numeric parameters must be finite"));
    }
    Ok(n.clamp(min, max) as f32)
}
fn boolean(params: &[ParamEntry], id: &str, default: bool) -> bool {
    params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.value {
            ParamValue::Bool(b) => Some(b),
            _ => None,
        })
        .unwrap_or(default)
}
fn text_param(id: &str, label: &str, default: &str) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::String(StringParam {
            default: default.into(),
        }),
    }
}
fn enum_param(id: &str, label: &str, options: &[&str], default: &str) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::EnumChoice(EnumParam {
            options: options.iter().map(|s| (*s).into()).collect(),
            default: default.into(),
        }),
    }
}
fn num_param(id: &str, label: &str, min: f64, max: f64, default: f64) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Number(NumberRange {
            min,
            max,
            default,
            step: 0.01,
        }),
    }
}
fn bool_param(id: &str, label: &str, default: bool) -> ParamSpec {
    ParamSpec {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Bool(BoolParam { default }),
    }
}

fn validate(code: &str) -> Result<String, Diagnostic> {
    if code.len() > MAX_BYTES
        || code.chars().count() > MAX_CHARS
        || code.split('\n').count() > MAX_LINES
    {
        return Err(error(
            "Code exceeds 8192 bytes, 2048 characters or 80 lines",
        ));
    }
    // Tabs use four-column tab stops. CRLF has the same layout as LF.
    let mut result = String::new();
    let mut column = 0;
    for c in code.replace("\r\n", "\n").chars() {
        match c {
            '\n' => {
                result.push(c);
                column = 0;
            }
            '\t' => {
                let count = 4 - column % 4;
                result.extend(std::iter::repeat_n(' ', count));
                column += count;
            }
            c if c.is_control() => return Err(error("Unsupported control character in code")),
            c => {
                result.push(c);
                column += 1;
            }
        }
    }
    if result.chars().count() > MAX_CHARS {
        return Err(error("Expanded tabs exceed 2048 characters"));
    }
    Ok(result)
}

/// Split highlighted spans into identifier/number runs and individual punctuation.
/// Whitespace only affects placement, never steals a diff match.
fn tokenize(
    code: &str,
    language: &str,
    ss: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
) -> Result<Vec<Token>, Diagnostic> {
    let ext = match language {
        "rust" => "rs",
        "python" => "py",
        "javascript" => "js",
        "typescript" => "ts",
        "c" => "c",
        "cpp" | "c++" => "cpp",
        "json" => "json",
        "latex" | "tex" => "tex",
        "bash" => "sh",
        _ => return Err(error("Unsupported language")),
    };
    let syntax = ss
        .find_syntax_by_extension(ext)
        .ok_or_else(|| error("Language syntax is unavailable"))?;
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut tokens = Vec::new();
    let mut offset = 0;
    for (line, text) in LinesWithEndings::from(code).enumerate() {
        let spans = highlighter
            .highlight_line(text, ss)
            .map_err(|_| error("Syntax highlighting failed"))?;
        let mut column = 0;
        for (style, span) in spans {
            let mut word = String::new();
            let mut start = column;
            let mut start_offset = offset;
            let flush = |word: &mut String, tokens: &mut Vec<Token>, start, start_offset| {
                if !word.is_empty() {
                    tokens.push(Token {
                        text: std::mem::take(word),
                        column: start as f32,
                        line: line as f32,
                        offset: start_offset,
                        color: color(style.foreground),
                    });
                }
            };
            for c in span.chars() {
                if c.is_alphanumeric() || c == '_' {
                    if word.is_empty() {
                        start = column;
                        start_offset = offset;
                    }
                    word.push(c);
                } else {
                    flush(&mut word, &mut tokens, start, start_offset);
                    if !c.is_whitespace() {
                        tokens.push(Token {
                            text: c.to_string(),
                            column: column as f32,
                            line: line as f32,
                            offset,
                            color: color(style.foreground),
                        });
                    }
                }
                column += 1;
                offset += 1;
            }
            flush(&mut word, &mut tokens, start, start_offset);
        }
    }
    if tokens.len() > MAX_TOKENS {
        return Err(error("Code exceeds 512 syntax tokens"));
    }
    Ok(tokens)
}

fn highlighted(spec: &str, lines: usize) -> Result<Vec<bool>, Diagnostic> {
    if spec.len() > 512 {
        return Err(error("Highlight selection is too long"));
    }
    let mut selected = vec![spec.trim().is_empty(); lines];
    if spec.trim().is_empty() {
        return Ok(selected);
    }
    for part in spec.split(',') {
        let (a, b) = part
            .trim()
            .split_once('-')
            .map_or((part.trim(), part.trim()), |(a, b)| (a.trim(), b.trim()));
        let a: usize = a
            .parse()
            .map_err(|_| error("Use highlight lines like 2,4-6"))?;
        let b: usize = b
            .parse()
            .map_err(|_| error("Use highlight lines like 2,4-6"))?;
        if a == 0 || b < a || b > MAX_LINES {
            return Err(error("Highlight lines must be within 1..80 and ordered"));
        }
        for line in a..=b {
            if let Some(value) = selected.get_mut(line - 1) {
                *value = true;
            }
        }
    }
    Ok(selected)
}
fn dimensions(code: &str) -> (usize, usize) {
    (
        code.split('\n')
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0),
        code.split('\n').count(),
    )
}

impl Guest for CodeSnippets {
    fn describe() -> Manifest {
        Manifest {
            name: "Code Snippets".into(),
            version: "0.1.0".into(),
            description: "Syntax-highlighted code with type-on reveal and token morph animations"
                .into(),
            kinds: vec!["clip-source".into()],
            metadata: vec![
                ("param.font_size.unit".into(), "% of short edge".into()),
                ("param.padding.unit".into(), "% of short edge".into()),
                ("param.corner_radius.unit".into(), "% of short edge".into()),
            ],
            params: vec![
                text_param("code", "Code", CODE),
                text_param("code_to", "Morph target", ""),
                enum_param(
                    "language",
                    "Language",
                    &[
                        "rust",
                        "python",
                        "javascript",
                        "typescript",
                        "c",
                        "cpp",
                        "c++",
                        "json",
                        "latex",
                        "tex",
                        "bash",
                    ],
                    "rust",
                ),
                enum_param("theme", "Theme", &["ocean", "night", "paper"], "ocean"),
                num_param("font_size", "Font size (%)", 0.5, 12.0, 4.0),
                bool_param("line_numbers", "Line numbers", true),
                text_param("highlight_lines", "Highlight lines (2,4-6)", ""),
                bool_param("background", "Background panel", true),
                num_param("padding", "Panel padding (%)", 0.0, 12.0, 4.0),
                num_param("corner_radius", "Panel radius (%)", 0.0, 12.0, 2.0),
                enum_param("mode", "Animation", &["reveal", "morph"], "reveal"),
                enum_param(
                    "reveal_by",
                    "Reveal by",
                    &["character", "line"],
                    "character",
                ),
                num_param("progress", "Progress", 0.0, 1.0, 1.0),
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

fn render(params: &[ParamEntry], size: Size) -> Result<Scene, Diagnostic> {
    if size.width == 0 || size.height == 0 {
        return Err(error("Canvas dimensions must be positive"));
    }
    let code = validate(string(params, "code", CODE))?;
    let mode = string(params, "mode", "reveal");
    if !["reveal", "morph"].contains(&mode) {
        return Err(error("Unsupported animation mode"));
    }
    let morph = mode == "morph";
    let code_to = if morph {
        validate(string(params, "code_to", ""))?
    } else {
        code.clone()
    };
    let progress = number(params, "progress", 1.0, 0.0, 1.0)?;
    let ss = syntax_set();
    let themes = ThemeSet::load_defaults();
    let theme_name = match string(params, "theme", "ocean") {
        "ocean" => "base16-ocean.dark",
        "night" => "base16-eighties.dark",
        "paper" => "InspiredGitHub",
        _ => return Err(error("Unsupported theme")),
    };
    let theme = &themes.themes[theme_name];
    let language = string(params, "language", "rust");
    let old = tokenize(&code, language, &ss, theme)?;
    let new = if morph {
        tokenize(&code_to, language, &ss, theme)?
    } else {
        Vec::new()
    };
    let (cols, lines) = dimensions(&code);
    let (cols_to, lines_to) = dimensions(&code_to);
    let lines = lines.max(lines_to);
    let selected = highlighted(string(params, "highlight_lines", ""), lines)?;
    let short = size.width.min(size.height) as f32;
    let font = number(params, "font_size", 4.0, 0.5, 12.0)? * short / 100.0;
    let padding = number(params, "padding", 4.0, 0.0, 12.0)? * short / 100.0;
    let radius = number(params, "corner_radius", 2.0, 0.0, 12.0)? * short / 100.0;
    let numbers = boolean(params, "line_numbers", true);
    let gutter = if numbers {
        lines.to_string().len() + 2
    } else {
        0
    };
    let mut painter = Painter::new(
        size,
        font,
        padding,
        cols.max(cols_to) + gutter,
        lines,
        gutter,
    );
    if boolean(params, "background", true) {
        painter.panel(
            radius,
            color(
                theme
                    .settings
                    .background
                    .unwrap_or(syntect::highlighting::Color::BLACK),
            ),
        );
    }
    let foreground = color(
        theme
            .settings
            .foreground
            .unwrap_or(syntect::highlighting::Color::WHITE),
    );
    if numbers {
        for line in 0..lines {
            let alpha = if morph {
                if line < lines.min(dimensions(&code).1) && line < lines_to {
                    0.45
                } else if line < dimensions(&code).1 {
                    (1.0 - progress) * 0.45
                } else {
                    progress * 0.45
                }
            } else {
                0.45
            };
            painter.text(
                &format!("{:>width$}", line + 1, width = gutter - 2),
                -(gutter as f32),
                line as f32,
                foreground,
                alpha,
            );
        }
    }
    let dim = |line: f32| if selected[line as usize] { 1.0 } else { 0.25 };
    if morph && (progress == 0.0 || progress == 1.0) {
        let endpoint = if progress == 0.0 { &old } else { &new };
        for token in endpoint {
            painter.text(
                &token.text,
                token.column,
                token.line,
                token.color,
                dim(token.line),
            );
        }
    } else if morph {
        let pairs = matches(&old, &new);
        let mut used = vec![false; new.len()];
        for (i, token) in old.iter().enumerate() {
            if let Some(j) = pairs[i] {
                used[j] = true;
                let to = &new[j];
                painter.text(
                    &token.text,
                    token.column + (to.column - token.column) * progress,
                    token.line + (to.line - token.line) * progress,
                    drawing::blend(token.color, to.color, progress),
                    dim(token.line) + (dim(to.line) - dim(token.line)) * progress,
                );
            } else {
                painter.text(
                    &token.text,
                    token.column,
                    token.line,
                    token.color,
                    (1.0 - progress) * dim(token.line),
                );
            }
        }
        for (i, token) in new.iter().enumerate().filter(|(i, _)| !used[*i]) {
            let _ = i;
            painter.text(
                &token.text,
                token.column,
                token.line,
                token.color,
                progress * dim(token.line),
            );
        }
    } else {
        let reveal_by = string(params, "reveal_by", "character");
        if !["character", "line"].contains(&reveal_by) {
            return Err(error("Unsupported reveal unit"));
        }
        let visible = (code.chars().count() as f32 * progress).floor() as usize;
        let visible_lines = (dimensions(&code).1 as f32 * progress).floor() as usize;
        for token in &old {
            let count = if reveal_by == "line" {
                if (token.line as usize) < visible_lines {
                    token.text.chars().count()
                } else {
                    0
                }
            } else {
                visible.saturating_sub(token.offset)
            };
            painter.text(
                &token.text.chars().take(count).collect::<String>(),
                token.column,
                token.line,
                token.color,
                dim(token.line),
            );
        }
    }
    Ok(Scene {
        coordinate_space: CoordinateSpace::Canvas,
        elements: painter.elements,
    })
}
export!(CodeSnippets);

#[cfg(test)]
mod tests;
