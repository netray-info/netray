//! WCAG 2 contrast check over the status colours of `tokens.json`.

use serde_json::Value;

fn parse_hex(hex: &str) -> Option<[f64; 3]> {
    let digits = hex.strip_prefix('#').filter(|d| d.len() == 6)?;
    let channel = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?].map(|c| f64::from(c) / 255.0))
}

fn luminance(rgb: [f64; 3]) -> f64 {
    let [r, g, b] = rgb.map(|c| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn ratio_of(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// WCAG 2 contrast ratio of two `#rrggbb` colours, lighter on top; panics on any other input.
pub fn ratio(a: &str, b: &str) -> f64 {
    let parse = |hex| parse_hex(hex).unwrap_or_else(|| panic!("not a #rrggbb colour: {hex}"));
    ratio_of(parse(a), parse(b))
}

/// Status colours that miss their WCAG minimum against the page and the plate, one message per
/// miss: `text` needs 4.5, every other part (`mark`, `hair`, any new one) 3.0. Every state and
/// part under `status.light` and `status.dark` is checked, and one present in only one theme is a
/// violation: the other theme would fall back to it unchecked.
pub fn violations(tokens_json: &str) -> Vec<String> {
    let tokens: Value = match serde_json::from_str(tokens_json) {
        Ok(tokens) => tokens,
        Err(e) => return vec![format!("tokens are not JSON: {e}")],
    };
    let mut out = Vec::new();
    let colours = |theme: &str| -> Vec<(String, String)> {
        let mut found = Vec::new();
        let Some(states) = tokens
            .pointer(&format!("/status/{theme}"))
            .and_then(Value::as_object)
        else {
            return found;
        };
        for (state, parts) in states.iter().filter(|(k, _)| !k.starts_with('$')) {
            let Some(parts) = parts.as_object() else {
                continue;
            };
            for (part, node) in parts.iter().filter(|(k, _)| !k.starts_with('$')) {
                if let Some(hex) = node.get("$value").and_then(Value::as_str) {
                    found.push((format!("{state}.{part}"), hex.to_string()));
                }
            }
        }
        found
    };
    let (light, dark) = (colours("light"), colours("dark"));
    for (theme, mine, other) in [("light", &light, &dark), ("dark", &dark, &light)] {
        if mine.is_empty() {
            out.push(format!("status.{theme} holds no status colours"));
        }
        for (key, _) in mine
            .iter()
            .filter(|(k, _)| !other.iter().any(|(o, _)| o == k))
        {
            let missing = if theme == "light" { "dark" } else { "light" };
            out.push(format!(
                "status.{theme}.{key} has no status.{missing}.{key}"
            ));
        }
        for (key, fg) in mine {
            let path = format!("status.{theme}.{key}");
            let min = if key.ends_with(".text") { 4.5 } else { 3.0 };
            for ground in ["bg", "plate"] {
                let ground_path = format!("color.{theme}.{ground}");
                let Some(bg) = tokens
                    .pointer(&format!("/color/{theme}/{ground}/$value"))
                    .and_then(Value::as_str)
                else {
                    out.push(format!("{ground_path} is missing"));
                    continue;
                };
                let (Some(f), Some(b)) = (parse_hex(fg), parse_hex(bg)) else {
                    out.push(format!("{path} {fg} on {ground_path} ({bg}): not #rrggbb"));
                    continue;
                };
                let got = ratio_of(f, b);
                if got < min {
                    out.push(format!(
                        "{path} {fg} on {ground_path} ({bg}): {got:.2} < {min:?}"
                    ));
                }
            }
        }
    }
    out
}
