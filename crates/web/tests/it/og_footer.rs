//! The OG card footer reads "netray.info" without a label, and `<label> · netray.info` with one
//! (specs/features/v2-v1-remnants, Phase 1, requirements 1 and 2).

use std::time::SystemTime;

use netray_web::og::layout::DEFAULT_LABEL;
use netray_web::og::render::{svg_for_entry, svg_for_grade};

/// The text of the footer element: the last `<text>` of the card.
fn footer(svg: &str) -> &str {
    let start = svg.rfind("<text").expect("a text element");
    let open = start + svg[start..].find('>').expect("an open tag end") + 1;
    let close = open + svg[open..].find("</text>").expect("a close tag");
    &svg[open..close]
}

fn grade_card(label: &str) -> String {
    svg_for_grade("example.com", "A", label, 91.0, SystemTime::UNIX_EPOCH, None)
}

#[test]
fn c1_c4_the_default_card_footer_is_the_apex_alone() {
    let svg = grade_card(DEFAULT_LABEL);
    assert_eq!(footer(&svg), "netray.info", "{svg}");
    assert!(!svg.contains('\u{00b7}'), "no separator: {svg}");
}

#[test]
fn c1_c5_the_default_entry_card_footer_is_the_apex_alone() {
    let svg = svg_for_entry(
        "example.com",
        "A",
        DEFAULT_LABEL,
        SystemTime::UNIX_EPOCH,
        "<g/>",
    );
    assert_eq!(footer(&svg), "netray.info", "{svg}");
}

#[test]
fn c2_c6_an_explicit_label_keeps_its_form() {
    assert_eq!(footer(&grade_card("custom")), "custom \u{00b7} netray.info");
}

#[test]
fn c2_c7_an_empty_label_renders_no_separator() {
    assert_eq!(footer(&grade_card("")), "netray.info");
}
