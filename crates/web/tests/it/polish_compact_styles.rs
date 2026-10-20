//! Result polish, Phase 1 (spec v2-result-polish, requirement 10; C10, C22): the compact evidence
//! table's styles resolve to the board values, the phone rules sit in the 480px query, and the
//! medium font weight is a token.
use std::fs;
use std::path::PathBuf;

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn static_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static")
}

/// Declaration blocks (whitespace squashed) whose selector list contains `selector`.
/// A selector list split at its top-level commas only (`:is(th, td)` stays whole).
fn top_level(list: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in list.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

fn blocks_in(css: &str, selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        if top_level(&squash(&rest[..open]))
            .iter()
            .any(|s| *s == squash(selector))
        {
            out.push(squash(&rest[open + 1..open + close]));
        }
        rest = &rest[open + close + 1..];
    }
    out
}

fn css_blocks(selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(static_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "css") {
            continue;
        }
        out.extend(blocks_in(&fs::read_to_string(&path).unwrap(), selector));
    }
    out
}

/// Inner text of every `@media (max-width: 480px) { ... }` region, by brace matching.
fn phone_regions(css: &str) -> Vec<String> {
    let marker = "@media (max-width: 480px)";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = css[from..].find(marker) {
        let start = from + at;
        let Some(open) = css[start..].find('{') else {
            break;
        };
        let body = start + open + 1;
        let mut depth = 1;
        let mut end = css.len();
        for (i, c) in css[body..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = body + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(css[body..end].to_string());
        from = end;
    }
    out
}

fn has(blocks: &[String], decl: &str) -> bool {
    blocks.iter().any(|b| b.split(';').any(|d| d == decl))
}

#[test]
fn the_compact_table_styles_resolve_to_the_board_values() {
    let rows: [(&str, &str); 15] = [
        (
            ".evidence-tabs__caption",
            "font-size:var(--font-size-small)",
        ),
        (".evidence-tabs__caption", "color:var(--color-muted)"),
        (".evidence-tabs__caption", "margin:0"),
        (".evidence-tabs__compact", "width:100%"),
        (".evidence-tabs__compact", "border-collapse:collapse"),
        (".evidence-tabs__compact", "font-family:var(--font-mono)"),
        (
            ".evidence-tabs__compact",
            "font-size:var(--font-size-small)",
        ),
        (".evidence-tabs__compact th", "padding:6px12px"),
        (
            ".evidence-tabs__compact th",
            "font-weight:var(--font-weight-medium)",
        ),
        (".evidence-tabs__compact th", "color:var(--color-muted)"),
        (".evidence-tabs__compact th", "text-align:left"),
        (".evidence-tabs__compact td", "padding:8px12px"),
        (
            ".evidence-tabs__compact td",
            "border-top:var(--line-hair)solidvar(--color-line)",
        ),
        (
            ".evidence-tabs__compact :is(th, td):first-child",
            "padding-left:0",
        ),
        (
            ".evidence-tabs__compact strong",
            "font-weight:var(--font-weight-bold)",
        ),
    ];
    let missing: Vec<String> = rows
        .iter()
        .filter(|(sel, decl)| !has(&css_blocks(sel), decl))
        .map(|(sel, decl)| format!("{sel} {{ {decl} }}"))
        .collect();
    assert!(missing.is_empty(), "missing declarations: {missing:#?}");
}

#[test]
fn the_compact_table_scrolls_sideways_on_a_phone() {
    let css = fs::read_to_string(static_dir().join("pages.css")).unwrap();
    let regions = phone_regions(&css);
    let phone =
        |sel: &str| -> Vec<String> { regions.iter().flat_map(|r| blocks_in(r, sel)).collect() };
    let rows = [
        (
            ".evidence-tabs__compact",
            "font-size:var(--font-size-label)",
        ),
        (".evidence-tabs__compact", "min-width:520px"),
        (".evidence-tabs__compact :is(th, td)", "padding-inline:10px"),
    ];
    let missing: Vec<String> = rows
        .iter()
        .filter(|(sel, decl)| !has(&phone(sel), decl))
        .map(|(sel, decl)| format!("@media (max-width: 480px) {sel} {{ {decl} }}"))
        .collect();
    assert!(missing.is_empty(), "missing declarations: {missing:#?}");
}

#[test]
fn the_medium_font_weight_is_a_token() {
    let tokens: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tokens.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(tokens["font"]["weight"]["medium"]["$value"], 500);
}
