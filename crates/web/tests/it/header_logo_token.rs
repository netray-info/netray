//! R4 (pustina-findings): the inline header logo keeps logo.svg's glyph
//! geometry at every viewport, so its font size comes from a token no
//! `*Phone` value lowers.

use std::path::PathBuf;

fn read(rel: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap()
}

fn px(v: &serde_json::Value) -> f64 {
    v["$value"]
        .as_str()
        .unwrap()
        .trim_end_matches("px")
        .parse()
        .unwrap()
}

#[test]
fn header_logo_font_size_token_has_no_phone_variant_and_keeps_logo_svg_size() {
    let css = read("static/pages.css");
    let rule = css
        .split(".page-header__logo text {")
        .nth(1)
        .expect("pages.css has a `.page-header__logo text` rule")
        .split('}')
        .next()
        .unwrap();
    let var = rule
        .split("font-size: var(--font-size-")
        .nth(1)
        .expect("the rule sets font-size from a --font-size-* token")
        .split(')')
        .next()
        .unwrap();

    let tokens: serde_json::Value = serde_json::from_str(&read("tokens.json")).unwrap();
    let sizes = &tokens["font"]["size"];
    let phone = format!("{var}Phone");
    assert!(
        sizes.get(&phone).is_none(),
        "token `{var}` has a phone variant `{phone}` that lowers the logo glyphs at phone width"
    );

    let partial = read("templates/partials/header_logo.html");
    let scale: f64 = partial
        .split("<g transform=\"scale(")
        .nth(1)
        .expect("partial has a scaled <g>")
        .split(')')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    // logo.svg sets font-size 100 in its viewBox.
    let desktop = px(&sizes[var]) * scale;
    let phone_px = sizes.get(&phone).map_or(desktop, |p| px(p) * scale);
    assert_eq!(desktop, 100.0, "desktop glyph size");
    assert_eq!(phone_px, 100.0, "phone glyph size");
}
