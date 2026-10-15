use serde_json::Value;
use std::fmt::Write as _;

struct Token {
    path: Vec<String>,
    value: String,
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|s| {
                    if s.contains(' ') {
                        format!("\"{s}\"")
                    } else {
                        s.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
        ),
        _ => None,
    }
}

fn walk(node: &Value, path: &mut Vec<String>, out: &mut Vec<Token>) {
    let Some(map) = node.as_object() else { return };
    if let Some(value) = map.get("$value").and_then(value_text) {
        out.push(Token {
            path: path.clone(),
            value,
        });
        return;
    }
    for (key, child) in map {
        if key.starts_with('$') {
            continue;
        }
        path.push(key.clone());
        walk(child, path, out);
        path.pop();
    }
}

fn words(segment: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut prev: Option<char> = None;
    for c in segment.chars() {
        let split =
            c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit());
        if split || out.is_empty() {
            out.push(String::new());
        }
        out.last_mut().unwrap().extend(c.to_lowercase());
        prev = Some(c);
    }
    out
}

fn join(path: &[String], sep: &str) -> String {
    path.iter()
        .flat_map(|s| words(s))
        .collect::<Vec<_>>()
        .join(sep)
}

fn theme_of(t: &Token) -> Option<&str> {
    t.path
        .iter()
        .map(String::as_str)
        .find(|s| matches!(*s, "light" | "dark"))
}

fn css_name(t: &Token) -> String {
    let kept: Vec<String> = t
        .path
        .iter()
        .filter(|s| !matches!(s.as_str(), "light" | "dark"))
        .cloned()
        .collect();
    join(&kept, "-")
}

fn is_phone(t: &Token) -> bool {
    t.path
        .last()
        .is_some_and(|s| s.len() > 5 && s.ends_with("Phone"))
}

fn is_glow(t: &Token) -> bool {
    t.path.len() == 3 && t.path[0] == "status" && t.path[1] == "glow"
}

/// One rule: an optional `color-scheme` (so embedded SVGs follow the theme), then the tokens.
fn block(
    out: &mut String,
    indent: &str,
    selector: &str,
    scheme: Option<&str>,
    decls: &[(String, String)],
) {
    writeln!(out, "{indent}{selector} {{").unwrap();
    if let Some(scheme) = scheme {
        writeln!(out, "{indent}  color-scheme: {scheme};").unwrap();
    }
    for (name, value) in decls {
        writeln!(out, "{indent}  --{name}: {value};").unwrap();
    }
    writeln!(out, "{indent}}}").unwrap();
}

fn main() {
    println!("cargo::rerun-if-changed=tokens.json");
    let text = std::fs::read_to_string("tokens.json").expect("failed to read tokens.json");
    let root: Value = serde_json::from_str(&text).expect("tokens.json is not JSON");
    let mut tokens = Vec::new();
    walk(&root, &mut Vec::new(), &mut tokens);

    let decl = |t: &Token| (css_name(t), t.value.clone());
    let mut light: Vec<_> = tokens
        .iter()
        .filter(|t| !is_phone(t) && !is_glow(t) && theme_of(t) != Some("dark"))
        .map(decl)
        .collect();
    let glows: Vec<&Token> = tokens.iter().filter(|t| is_glow(t)).collect();
    light.extend(glows.iter().map(|t| (css_name(t), "none".to_string())));
    let mut dark: Vec<_> = tokens
        .iter()
        .filter(|t| theme_of(t) == Some("dark"))
        .map(decl)
        .collect();
    dark.extend(glows.iter().map(|t| decl(t)));
    let phone: Vec<_> = tokens
        .iter()
        .filter(|t| is_phone(t))
        .map(|t| {
            let mut path = t.path.clone();
            let last = path.last_mut().unwrap();
            last.truncate(last.len() - "Phone".len());
            (join(&path, "-"), t.value.clone())
        })
        .collect();
    let phone_width = tokens
        .iter()
        .find(|t| t.path == ["layout", "phoneMax"])
        .expect("tokens.json has no layout.phoneMax")
        .value
        .clone();

    let mut css = String::new();
    block(&mut css, "", ":root", Some("light dark"), &light);
    writeln!(css, "@media (prefers-color-scheme: dark) {{").unwrap();
    block(&mut css, "  ", ":root", None, &dark);
    writeln!(css, "}}").unwrap();
    block(&mut css, "", "[data-theme=\"dark\"]", Some("dark"), &dark);
    writeln!(css, "@media (max-width: {phone_width}) {{").unwrap();
    block(&mut css, "  ", ":root", None, &phone);
    writeln!(css, "}}").unwrap();

    let mut rs = String::new();
    for t in &tokens {
        writeln!(
            rs,
            "pub const {}: &str = {:?};",
            join(&t.path, "_").to_uppercase(),
            t.value
        )
        .unwrap();
    }

    let out_dir = std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo");
    let out_dir = std::path::Path::new(&out_dir);
    std::fs::write(out_dir.join("tokens.css"), css).expect("failed to write tokens.css");
    std::fs::write(out_dir.join("tokens_values.rs"), rs).expect("failed to write tokens_values.rs");
}
