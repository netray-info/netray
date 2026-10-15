//! Identity SVGs are monochrome (V2 design system, Phase 4, requirement 8; criteria C3 and C5).
//! One table row per SVG; every rule is checked against the token values, not against literals.
use netray_web::tokens::values::{COLOR_DARK_INK, COLOR_LIGHT_INK, FONT_SANS};
use serde_json::Value;

const TOKENS: &str = include_str!("../tokens.json");

/// (file, carries the wordmark and therefore a font stack)
const FILES: [(&str, bool); 3] = [
    ("logo.svg", true),
    ("favicon.svg", false),
    ("badge-mark.svg", false),
];

const COLOUR_FUNCTIONS: [&str; 10] = [
    "rgb(", "rgba(", "hsl(", "hsla(", "hwb(", "lab(", "lch(", "oklab(", "oklch(", "color(",
];

fn read(name: &str) -> String {
    let path = format!("{}/static/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: cannot read {path}: {e}"))
}

/// Every hex colour leaf under `status` in tokens.json, lower-cased.
fn status_colours() -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        if let Some(obj) = v.as_object() {
            if let Some(s) = obj.get("$value").and_then(Value::as_str)
                && s.starts_with('#')
            {
                out.push(s.to_ascii_lowercase());
            }
            obj.values().for_each(|c| walk(c, out));
        }
    }
    let tokens: Value = serde_json::from_str(TOKENS).expect("tokens.json is JSON");
    let mut out = Vec::new();
    walk(&tokens["status"], &mut out);
    assert!(!out.is_empty(), "tokens.json has no status colours");
    out
}

/// Hex literals as (byte offset, text). A run of hex digits that is not a valid colour length is
/// reported as-is so it fails the ink comparison; `#id` selectors (non-hex run) are ignored.
fn hex_literals(s: &str) -> Vec<(usize, String)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    for (i, _) in s.match_indices('#') {
        if s[..i].ends_with("url(") || s[..i].ends_with("href=\"") {
            continue;
        }
        let run = b[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        let next = b.get(i + 1 + run).copied().unwrap_or(b' ');
        let ident_follows = next.is_ascii_alphanumeric() || next == b'-' || next == b'_';
        if run == 0 || (ident_follows && ![3, 4, 6, 8].contains(&run)) {
            continue; // an id selector such as #marker or #axis
        }
        out.push((i, s[i..i + 1 + run].to_string()));
    }
    out
}

/// Byte range of the body of the first `@media (prefers-color-scheme: dark)` block.
fn dark_block(s: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(rel) = s[from..].find("@media") {
        let at = from + rel;
        let open = at + s[at..].find('{')?;
        let prelude: String = s[at..open].chars().filter(|c| !c.is_whitespace()).collect();
        if prelude.contains("prefers-color-scheme:dark") {
            let mut depth = 0usize;
            for (i, c) in s[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((open + 1, open + i));
                        }
                    }
                    _ => {}
                }
            }
            return None;
        }
        from = open;
    }
    None
}

/// Values of a property, as CSS declaration (`name: v;`) or attribute (`name="v"`).
fn prop_values(s: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in s.match_indices(name) {
        let before = s[..i].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        let rest = s[i + name.len()..].trim_start();
        if let Some(r) = rest.strip_prefix(':') {
            let end = r.find([';', '}']).unwrap_or(r.len());
            out.push(r[..end].trim().to_string());
        } else if let Some(r) = rest.strip_prefix("=\"")
            && let Some(end) = r.find('"')
        {
            out.push(r[..end].trim().to_string());
        }
    }
    out
}

fn norm_font(v: &str) -> String {
    v.replace("&quot;", "\"")
        .replace('\'', "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn check(name: &str, is_logo: bool) -> Vec<String> {
    let svg = read(name);
    let low = svg.to_ascii_lowercase();
    let (light, dark) = (
        COLOR_LIGHT_INK.to_ascii_lowercase(),
        COLOR_DARK_INK.to_ascii_lowercase(),
    );
    let mut bad = Vec::new();

    for (_, lit) in hex_literals(&svg) {
        let l = lit.to_ascii_lowercase();
        if l != light && l != dark {
            bad.push(format!(
                "{name}: colour {lit} is neither color.light.ink nor color.dark.ink"
            ));
        }
    }
    for f in COLOUR_FUNCTIONS {
        if low.contains(f) {
            bad.push(format!("{name}: colour function {f} is not allowed"));
        }
    }
    if low.contains("url(") {
        bad.push(format!("{name}: url( is not allowed"));
    }
    for prop in ["fill", "stroke", "color", "stop-color"] {
        for v in prop_values(&svg, prop) {
            let l = v.to_ascii_lowercase();
            if l != "none" && l != light && l != dark {
                bad.push(format!(
                    "{name}: {prop} value {v:?} is not none or an ink colour"
                ));
            }
        }
    }

    match dark_block(&svg) {
        None => bad.push(format!(
            "{name}: no @media (prefers-color-scheme: dark) block"
        )),
        Some((a, z)) => {
            let hexes = hex_literals(&svg);
            let in_dark = |at: usize| at >= a && at < z;
            for (at, lit) in &hexes {
                let l = lit.to_ascii_lowercase();
                if l == light && in_dark(*at) {
                    bad.push(format!("{name}: light ink {lit} inside the dark block"));
                }
                if l == dark && !in_dark(*at) {
                    bad.push(format!("{name}: dark ink {lit} outside the dark block"));
                }
            }
            for (want, label) in [(&light, "light"), (&dark, "dark")] {
                if !hexes.iter().any(|(_, l)| l.to_ascii_lowercase() == *want) {
                    bad.push(format!("{name}: {label} ink {want} does not occur"));
                }
            }
        }
    }

    for s in status_colours() {
        if low.contains(&s) {
            bad.push(format!("{name}: status colour {s} appears"));
        }
    }
    if low.contains("gradient") {
        bad.push(format!("{name}: gradient is not allowed"));
    }

    for prop in ["rx", "ry", "border-radius"] {
        for v in prop_values(&svg, prop) {
            for tok in v.split_whitespace() {
                match tok.trim_end_matches("px").parse::<f64>() {
                    Ok(n) if n <= 2.0 => {}
                    _ => bad.push(format!("{name}: {prop} {v:?} exceeds 2 px")),
                }
            }
        }
    }

    if is_logo {
        let fonts = prop_values(&svg, "font-family");
        if fonts.is_empty() {
            bad.push(format!("{name}: no font-family"));
        }
        for f in fonts {
            if norm_font(&f) != norm_font(FONT_SANS) {
                bad.push(format!("{name}: font-family {f:?} differs from font.sans"));
            }
        }
    }
    bad
}

#[test]
fn identity_svgs_are_monochrome_ink_only() {
    let mut failures = Vec::new();
    for (name, is_logo) in FILES {
        failures.extend(check(name, is_logo));
    }
    assert!(
        failures.is_empty(),
        "identity violations:\n{}",
        failures.join("\n")
    );
}
