//! Contract tests for the generated design tokens (V2 spec v2-design-system, R3 and R4).
use netray_web::tokens::{values, CSS};

/// A parsed top-level CSS block: selector or at-rule prelude (whitespace removed) and raw body.
struct Block {
    selector: String,
    body: String,
}

fn blocks(css: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut body_start) = (0usize, 0usize, 0usize);
    for (i, c) in css.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    body_start = i + 1;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    out.push(Block {
                        selector: css[start..body_start - 1]
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect(),
                        body: css[body_start..i].to_string(),
                    });
                    start = i + 1;
                }
            }
            _ => {}
        }
    }
    out
}

/// Custom-property declarations anywhere in a body (nested blocks are flattened).
fn decls(body: &str) -> Vec<(String, String)> {
    body.split(['{', '}', ';'])
        .filter_map(|d| d.trim().split_once(':'))
        .filter(|(k, _)| k.trim().starts_with("--"))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn find<'a>(all: &'a [Block], pred: impl Fn(&str) -> bool, what: &str) -> Vec<&'a Block> {
    let found: Vec<_> = all.iter().filter(|b| pred(&b.selector)).collect();
    assert!(!found.is_empty(), "no block for {what} in tokens.css");
    found
}

fn root() -> Vec<(String, String)> {
    let all = blocks(CSS);
    find(&all, |s| s == ":root", ":root")
        .iter()
        .flat_map(|b| decls(&b.body))
        .collect()
}

fn dark_media() -> Vec<(String, String)> {
    let all = blocks(CSS);
    find(
        &all,
        |s| s.starts_with("@media") && s.contains("prefers-color-scheme:dark"),
        "@media (prefers-color-scheme: dark)",
    )
    .iter()
    .flat_map(|b| decls(&b.body))
    .collect()
}

fn dark_attr() -> Vec<(String, String)> {
    let all = blocks(CSS);
    find(
        &all,
        |s| s.contains("[data-theme=\"dark\"]") || s.contains("[data-theme=dark]"),
        "[data-theme=\"dark\"]",
    )
    .iter()
    .flat_map(|b| decls(&b.body))
    .collect()
}

fn phone() -> Vec<(String, String)> {
    let all = blocks(CSS);
    find(
        &all,
        |s| s.starts_with("@media") && s.contains("max-width:390px"),
        "@media (max-width: 390px)",
    )
    .iter()
    .flat_map(|b| decls(&b.body))
    .collect()
}

fn get<'a>(set: &'a [(String, String)], name: &str) -> Option<&'a str> {
    set.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

#[test]
fn light_values_sit_on_root() {
    let root = root();
    for (name, value) in [
        ("--color-bg", "#f7f8fa"),
        ("--color-plate-border", "#d5d9df"),
        ("--status-warn-text", "#9a5f00"),
        ("--font-size-h1", "40px"),
    ] {
        assert_eq!(get(&root, name), Some(value), "{name} on :root");
    }
}

#[test]
fn dark_values_sit_in_both_dark_blocks() {
    for (label, set) in [("media", dark_media()), ("data-theme", dark_attr())] {
        for (name, value) in [
            ("--color-bg", "#07080a"),
            ("--status-warn-text", "#f0a93b"),
            ("--status-pass-hair", "#2c6b4a"),
        ] {
            assert_eq!(get(&set, name), Some(value), "{name} in dark {label} block");
        }
    }
}

#[test]
fn theme_segment_is_dropped_from_property_names() {
    for (label, set) in [("root", root()), ("dark", dark_media())] {
        assert!(
            set.iter().all(|(k, _)| !k.contains("-light-") && !k.contains("-dark-")),
            "{label} block leaks a theme segment: {set:?}"
        );
    }
}

#[test]
fn phone_tokens_override_their_base_property() {
    let phone = phone();
    for (name, value) in [
        ("--font-size-h1", "26px"),
        ("--font-size-grade", "64px"),
        ("--font-size-caption", "10px"),
        ("--spectrum-plate-height", "80px"),
        ("--spectrum-trace-row", "6px"),
    ] {
        assert_eq!(get(&phone, name), Some(value), "{name} in phone query");
    }
    assert!(
        phone.iter().all(|(k, _)| !k.ends_with("-phone")),
        "phone query must not define *-phone properties: {phone:?}"
    );
    let root = root();
    assert_eq!(get(&root, "--font-size-grade"), Some("88px"));
    assert_eq!(get(&root, "--spectrum-plate-height"), Some("92px"));
}

#[test]
fn glow_is_none_on_root_and_set_only_in_dark_blocks() {
    let root = root();
    for (name, dark) in [
        ("--status-glow-fail", "0 0 8px rgba(255,93,85,0.75)"),
        ("--status-glow-warn", "0 0 6px rgba(240,169,59,0.6)"),
        ("--status-glow-pass", "0 0 3px rgba(63,185,122,0.35)"),
    ] {
        assert_eq!(get(&root, name), Some("none"), "{name} on :root");
        assert_eq!(get(&dark_media(), name), Some(dark), "{name} in dark media");
        assert_eq!(get(&dark_attr(), name), Some(dark), "{name} in data-theme dark");
    }
}

#[test]
fn rust_constants_carry_every_value() {
    let rows: [(&str, &str, &str); 9] = [
        ("COLOR_LIGHT_BG", values::COLOR_LIGHT_BG, "#f7f8fa"),
        ("COLOR_DARK_BG", values::COLOR_DARK_BG, "#07080a"),
        ("STATUS_DARK_PASS_HAIR", values::STATUS_DARK_PASS_HAIR, "#2c6b4a"),
        ("STATUS_LIGHT_WARN_TEXT", values::STATUS_LIGHT_WARN_TEXT, "#9a5f00"),
        ("FONT_SIZE_H1_PHONE", values::FONT_SIZE_H1_PHONE, "26px"),
        ("FONT_WEIGHT_REGULAR", values::FONT_WEIGHT_REGULAR, "400"),
        ("LAYOUT_PHONE", values::LAYOUT_PHONE, "390px"),
        (
            "FONT_SANS",
            values::FONT_SANS,
            "ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", sans-serif",
        ),
        (
            "STATUS_GLOW_FAIL",
            values::STATUS_GLOW_FAIL,
            "0 0 8px rgba(255,93,85,0.75)",
        ),
    ];
    for (name, got, want) in rows {
        assert_eq!(got, want, "{name}");
    }
}
