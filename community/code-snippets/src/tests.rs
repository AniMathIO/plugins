use super::*;
fn params(values: &[(&str, ParamValue)]) -> Vec<ParamEntry> {
    values
        .iter()
        .map(|(id, value)| ParamEntry {
            id: (*id).into(),
            value: value.clone(),
        })
        .collect()
}
fn snapshot(p: &[ParamEntry]) -> String {
    format!(
        "{:?}",
        render(
            p,
            Size {
                width: 1280,
                height: 720
            }
        )
        .unwrap()
    )
}
#[test]
fn limits_and_unicode() {
    assert!(validate(&"a".repeat(MAX_CHARS + 1)).is_err());
    assert!(validate(&"\n".repeat(MAX_LINES)).is_err());
    assert_eq!(validate("a\tβ\r\n").unwrap(), "a   β\n");
    assert!(validate("\0").is_err());
    assert!(validate(&"\t".repeat(1000)).is_err());
    assert!(number(
        &params(&[("p", ParamValue::Number(f64::NAN))]),
        "p",
        0.,
        0.,
        1.
    )
    .is_err());
}
#[test]
fn selection_is_strict_and_bounded() {
    assert_eq!(
        highlighted("2,4-6", 6).unwrap(),
        vec![false, true, false, true, true, true]
    );
    for bad in ["0", "5-2", "1-999999999", "x", "1,", "1-2-3"] {
        assert!(highlighted(bad, 6).is_err());
    }
}
#[test]
fn all_languages_highlight_and_preserve_layout() {
    let ss = syntax_set();
    let themes = ThemeSet::load_defaults();
    for language in [
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
    ] {
        let tokens = tokenize(
            "fn hello() {\n    hello(\"β\");\n}",
            language,
            &ss,
            &themes.themes["base16-ocean.dark"],
        )
        .unwrap();
        assert_eq!(tokens[0].text, "fn");
        assert!(
            tokens.iter().any(|t| t.line == 1. && t.column == 4.),
            "{language}"
        );
    }
    let tokens = tokenize(
        "fn main() { let x = 42; }",
        "rust",
        &ss,
        &themes.themes["base16-ocean.dark"],
    )
    .unwrap();
    assert!(tokens
        .iter()
        .any(|t| format!("{:?}", t.color) != format!("{:?}", tokens[0].color)));
}
#[test]
fn diff_is_one_to_one_and_retains_moved_identifiers() {
    let ss = syntax_set();
    let themes = ThemeSet::load_defaults();
    let token = |s| tokenize(s, "rust", &ss, &themes.themes["base16-ocean.dark"]).unwrap();
    let a = token("alpha();\nbeta();\nalpha();");
    let b = token("beta();\nalpha();\nalpha();");
    let pairs = matches(&a, &b);
    let mut seen = std::collections::HashSet::new();
    for (i, j) in pairs
        .iter()
        .enumerate()
        .filter_map(|(i, j)| j.map(|j| (i, j)))
    {
        assert_eq!(a[i].text, b[j].text);
        assert!(seen.insert(j));
    }
    for (i, t) in a
        .iter()
        .enumerate()
        .filter(|(_, t)| t.text == "alpha" || t.text == "beta")
    {
        assert!(pairs[i].is_some(), "{}", t.text);
    }
}
#[test]
fn reveal_and_morph_endpoints_are_exact_and_repeatable() {
    let base = [
        ("code", ParamValue::String("fn old() {}".into())),
        ("code_to", ParamValue::String("fn new() {}".into())),
        ("line_numbers", ParamValue::Bool(false)),
    ];
    let frame = |mode: &str, p: f64| {
        let mut v = params(&base);
        v.extend(params(&[
            ("mode", ParamValue::EnumChoice(mode.into())),
            ("progress", ParamValue::Number(p)),
        ]));
        snapshot(&v)
    };
    assert_eq!(frame("morph", 0.), frame("reveal", 1.));
    assert_ne!(frame("reveal", 0.), frame("reveal", 0.5));
    assert_ne!(frame("reveal", 0.5), frame("reveal", 1.));
    let end = params(&[
        ("code", ParamValue::String("fn new() {}".into())),
        ("line_numbers", ParamValue::Bool(false)),
    ]);
    assert_eq!(frame("morph", 1.), snapshot(&end));
    let first = frame("morph", 0.5);
    let _ = frame("morph", 0.2);
    assert_eq!(first, frame("morph", 0.5));
}
#[test]
fn line_reveal_keeps_glyph_counters_and_layout() {
    let p = params(&[
        ("code", ParamValue::String("O0\nB8".into())),
        ("reveal_by", ParamValue::EnumChoice("line".into())),
        ("line_numbers", ParamValue::Bool(false)),
        ("background", ParamValue::Bool(false)),
        ("progress", ParamValue::Number(0.5)),
    ]);
    let scene = render(
        &p,
        Size {
            width: 1280,
            height: 720,
        },
    )
    .unwrap();
    assert_eq!(scene.elements.len(), 2);
    if let DrawCommand::FillPath((path, _)) = &scene.elements[0] {
        assert!(
            path.iter().filter(|v| matches!(v, PathVerb::Close)).count() >= 2,
            "O counter must remain a subpath"
        );
    } else {
        panic!("expected glyph path");
    }
}
