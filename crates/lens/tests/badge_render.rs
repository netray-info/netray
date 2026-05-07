/// Unit tests for SVG badge rendering (SDD §11.13).
use lens::badge::BadgeStyle;
use lens::badge::palette::color_for_grade;
use lens::badge::render::{escape_xml, svg_for_grade, width_px};

// ---------------------------------------------------------------------------
// §11.13: SVG snapshot matches palette for each grade × style (14 cases)
// ---------------------------------------------------------------------------

fn right_segment_fill(svg: &str) -> &str {
    // The second <rect> in the SVG carries the value_color fill.
    // For flat: `<rect rx="3" x="{label_width}" ... fill="{value_color}"/>`
    // For for-the-badge: second `<rect x="{label_width}" ... fill="{value_color}"/>`
    // Both cases have "fill=\"{color}\"" appearing after "x=" in the second rect.
    // We look for the SECOND occurrence of fill= to get the value color.
    let mut fill_positions = svg.match_indices("fill=\"#");
    fill_positions.next(); // skip first (label segment background)
    if let Some((pos, _)) = fill_positions.next() {
        let start = pos + 6; // after fill="
        let end = svg[start..].find('"').map(|i| start + i).unwrap_or(start);
        &svg[start..end]
    } else {
        ""
    }
}

macro_rules! grade_color_test {
    ($name:ident, $grade:expr, $expected_color:expr, $style:expr) => {
        #[test]
        fn $name() {
            let svg = svg_for_grade("lens", $grade, $style);
            let color = right_segment_fill(&svg);
            assert_eq!(
                color,
                $expected_color,
                "grade={} style={:?}: expected fill={} in SVG",
                $grade,
                $style,
                $expected_color
            );
        }
    };
}

// Flat style
grade_color_test!(flat_aplus_color, "A+", "#16a34a", BadgeStyle::Flat);
grade_color_test!(flat_a_color, "A", "#22c55e", BadgeStyle::Flat);
grade_color_test!(flat_b_color, "B", "#eab308", BadgeStyle::Flat);
grade_color_test!(flat_c_color, "C", "#f59e0b", BadgeStyle::Flat);
grade_color_test!(flat_d_color, "D", "#dc2626", BadgeStyle::Flat);
grade_color_test!(flat_f_color, "F", "#991b1b", BadgeStyle::Flat);
grade_color_test!(flat_error_color, "error", "#6b7280", BadgeStyle::Flat);

// For-the-badge style
grade_color_test!(ftb_aplus_color, "A+", "#16a34a", BadgeStyle::ForTheBadge);
grade_color_test!(ftb_a_color, "A", "#22c55e", BadgeStyle::ForTheBadge);
grade_color_test!(ftb_b_color, "B", "#eab308", BadgeStyle::ForTheBadge);
grade_color_test!(ftb_c_color, "C", "#f59e0b", BadgeStyle::ForTheBadge);
grade_color_test!(ftb_d_color, "D", "#dc2626", BadgeStyle::ForTheBadge);
grade_color_test!(ftb_f_color, "F", "#991b1b", BadgeStyle::ForTheBadge);
grade_color_test!(ftb_error_color, "error", "#6b7280", BadgeStyle::ForTheBadge);

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

#[test]
fn escape_xml_ampersand() {
    assert_eq!(escape_xml("a&b"), "a&amp;b");
}

#[test]
fn escape_xml_lt_gt() {
    assert_eq!(escape_xml("<x>"), "&lt;x&gt;");
}

#[test]
fn escape_xml_quotes() {
    assert_eq!(escape_xml("\"q\""), "&quot;q&quot;");
    assert_eq!(escape_xml("it's"), "it&#39;s");
}

#[test]
fn escape_xml_no_change_for_clean_string() {
    assert_eq!(escape_xml("hello world"), "hello world");
}

#[test]
fn label_with_ampersand_is_escaped_in_svg() {
    let svg = svg_for_grade("a&b", "A", BadgeStyle::Flat);
    assert!(svg.contains("a&amp;b"), "label must be HTML-escaped in SVG");
    assert!(!svg.contains(">a&b<"), "bare & must not appear");
}

// ---------------------------------------------------------------------------
// Width formula
// ---------------------------------------------------------------------------

#[test]
fn flat_width_4_chars() {
    assert_eq!(width_px(4, BadgeStyle::Flat), 38); // 4*7+10
}

#[test]
fn flat_width_0_chars() {
    assert_eq!(width_px(0, BadgeStyle::Flat), 10);
}

#[test]
fn for_the_badge_width_4_chars() {
    assert_eq!(width_px(4, BadgeStyle::ForTheBadge), 48); // 4*8+16
}

#[test]
fn for_the_badge_width_2_chars() {
    assert_eq!(width_px(2, BadgeStyle::ForTheBadge), 32); // 2*8+16
}

// ---------------------------------------------------------------------------
// Height
// ---------------------------------------------------------------------------

#[test]
fn flat_svg_height_is_20() {
    let svg = svg_for_grade("lens", "A", BadgeStyle::Flat);
    assert!(
        svg.contains("height=\"20\""),
        "flat badge height must be 20, svg: {svg}"
    );
}

#[test]
fn for_the_badge_height_is_28() {
    let svg = svg_for_grade("lens", "A", BadgeStyle::ForTheBadge);
    assert!(
        svg.contains("height=\"28\""),
        "for-the-badge height must be 28, svg: {svg}"
    );
}

// ---------------------------------------------------------------------------
// Unknown/error grade renders "?"
// ---------------------------------------------------------------------------

#[test]
fn error_grade_flat_shows_question_mark() {
    let svg = svg_for_grade("lens", "error", BadgeStyle::Flat);
    assert!(svg.contains(">?<"), "error grade must render ?");
    assert!(!svg.contains(">error<"));
}

#[test]
fn error_grade_ftb_shows_question_mark_uppercased() {
    let svg = svg_for_grade("lens", "error", BadgeStyle::ForTheBadge);
    assert!(svg.contains(">?<"), "error grade must render ? in for-the-badge");
}

// ---------------------------------------------------------------------------
// Grade colors via palette
// ---------------------------------------------------------------------------

#[test]
fn palette_a_plus_is_green_600() {
    assert_eq!(color_for_grade("A+"), "#16a34a");
}

#[test]
fn palette_f_is_red_800() {
    assert_eq!(color_for_grade("F"), "#991b1b");
}

#[test]
fn palette_unknown_is_gray() {
    assert_eq!(color_for_grade("Z"), "#6b7280");
    assert_eq!(color_for_grade(""), "#6b7280");
}
