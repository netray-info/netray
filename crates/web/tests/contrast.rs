//! Contract tests for the WCAG 2 contrast check over the design tokens (V2 design system, C5).
use netray_web::contrast::{ratio, violations};
use serde_json::Value;

const TOKENS: &str = include_str!("../tokens.json");

#[test]
fn ratio_matches_wcag_reference_values() {
    let rows = [
        ("#000000", "#ffffff", 21.0),
        ("#777777", "#ffffff", 4.48),
        ("#ffffff", "#ffffff", 1.0),
    ];
    for (a, b, want) in rows {
        let got = ratio(a, b);
        assert!(
            (got - want).abs() < 0.01,
            "ratio({a}, {b}) = {got}, want {want}"
        );
        let swapped = ratio(b, a);
        assert!(
            (got - swapped).abs() < 1e-9,
            "ratio must be order-independent"
        );
    }
}

#[test]
fn real_tokens_have_no_contrast_violations() {
    let found = violations(TOKENS);
    assert!(found.is_empty(), "unexpected violations: {found:#?}");
}

#[test]
fn weak_status_text_is_reported_with_path_hex_ground_and_ratio() {
    let mut tokens: Value = serde_json::from_str(TOKENS).expect("tokens.json is JSON");
    tokens["status"]["light"]["pass"]["text"] = Value::String("#9aa0a6".into());
    let found = violations(&tokens.to_string());

    let hit = found
        .iter()
        .find(|m| {
            m.contains("status.light.pass.text")
                && m.contains("#9aa0a6")
                && m.contains("color.light.bg")
        })
        .unwrap_or_else(|| panic!("no matching violation in {found:#?}"));
    let expected = format!(
        "{:.2}",
        ratio("#9aa0a6", tokens["color"]["light"]["bg"].as_str().unwrap())
    );
    assert!(
        hit.contains(&expected),
        "message {hit:?} must carry ratio {expected}"
    );
}
