//! The plateless social card shows no percentage, and both cards set their grade letter in a
//! colour that reaches large-text contrast on the light background (pustina-findings R7).
use std::time::{Duration, SystemTime};

use netray_web::contrast::ratio;
use netray_web::og::render::{svg_for_entry, svg_for_grade};

/// `color.light.bg` in crates/web/tokens.json.
const LIGHT_BG: &str = "#f7f8fa";

fn t() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

#[test]
fn plain_card_shows_no_percentage() {
    // An admitted run reaches the card through its plate (protocol grades, og_light_card); the
    // plateless card is left for the unscored `?` and must not print a percentage either.
    let svg = svg_for_grade("example.com", "A", "", 91.7, t(), None);
    assert!(!svg.contains('%'), "plain card shows a percentage: {svg}");
}

/// The fill of the grade letter, the first text element with weight 700.
fn grade_fill(svg: &str) -> String {
    let marker = "font-weight=\"700\" fill=\"";
    let start = svg.find(marker).expect("grade text element") + marker.len();
    svg[start..start + 7].to_string()
}

#[test]
fn grade_letter_reaches_large_text_contrast_on_the_light_background() {
    let mut low = Vec::new();
    for grade in ["A+", "A", "B", "C", "D", "F"] {
        let plain = svg_for_grade("example.com", grade, "", 80.0, t(), None);
        let entry = svg_for_entry("example.com", grade, "", t(), "<g/>");
        for (card, svg) in [("plain", plain), ("entry", entry)] {
            let fill = grade_fill(&svg);
            let r = ratio(&fill, LIGHT_BG);
            if r < 3.0 {
                low.push(format!("{card} card, grade {grade}: {fill} is {r:.2}:1"));
            }
        }
    }
    assert!(low.is_empty(), "grade letter below 3:1: {low:#?}");
}
