//! Final review of v2-result-fidelity: the hard-fail sentence carries the 3 px fail bar
//! (requirement 11) and the score rounds down so it never reaches the next grade's threshold
//! (requirement 5).

fn pages_css() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/pages.css")).unwrap()
}

fn rule(css: &str, selector: &str) -> String {
    let start = css
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("no rule {selector}"));
    let body = &css[start..];
    body[..body.find('}').unwrap()].to_string()
}

#[test]
fn the_hard_fail_sentence_draws_a_3px_fail_bar() {
    let body = rule(&pages_css(), ".hard-fail__sentence");
    assert!(
        body.contains("border-inline-start: var(--line-warn) solid var(--status-fail-mark)"),
        "{body}"
    );
    let tokens = netray_web::tokens::CSS;
    assert!(tokens.contains("--line-warn: 3px"), "line-warn is 3 px");
}

#[test]
fn the_score_is_rounded_down() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/pages/result/mod.rs"
    ))
    .unwrap();
    assert!(
        src.contains("views::score_number("),
        "the score goes through score_number"
    );
    assert!(
        !src.contains("overall_percentage.round()"),
        "no half-up rounding"
    );
}
