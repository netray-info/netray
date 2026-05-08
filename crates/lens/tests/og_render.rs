/// Unit tests for OG card rendering (SVG template and PNG rasterization).
use std::time::SystemTime;

use lens::og::fonts::init_font_db;
use lens::og::render::{format_utc_timestamp, svg_for_grade, svg_to_png, truncate_domain, xml_escape};

fn ts() -> SystemTime {
    // 2026-05-08 17:44:00 UTC → unix 1778262240
    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_778_262_240)
}

// ---------------------------------------------------------------------------
// xml_escape
// ---------------------------------------------------------------------------

#[test]
fn xml_escape_all_special_chars() {
    assert_eq!(xml_escape("a&b"), "a&amp;b");
    assert_eq!(xml_escape("<script>"), "&lt;script&gt;");
    assert_eq!(xml_escape("\"q\""), "&quot;q&quot;");
    assert_eq!(xml_escape("it's"), "it&apos;s");
    assert_eq!(xml_escape("clean"), "clean");
}

// ---------------------------------------------------------------------------
// truncate_domain
// ---------------------------------------------------------------------------

#[test]
fn truncate_domain_short_unchanged() {
    assert_eq!(truncate_domain("example.com"), "example.com");
}

#[test]
fn truncate_domain_exactly_40_unchanged() {
    let s = "a".repeat(40);
    let r = truncate_domain(&s);
    assert_eq!(r.chars().count(), 40);
    assert!(!r.contains('\u{2026}'));
}

#[test]
fn truncate_domain_41_chars_gives_38() {
    let s = "a".repeat(41);
    let r = truncate_domain(&s);
    assert!(r.ends_with('\u{2026}'));
    assert_eq!(r.chars().count(), 38); // 37 + ellipsis
}

#[test]
fn truncate_domain_50_chars_gives_38() {
    let s = "a".repeat(50);
    let r = truncate_domain(&s);
    assert_eq!(r.chars().count(), 38);
}

// ---------------------------------------------------------------------------
// format_utc_timestamp
// ---------------------------------------------------------------------------

#[test]
fn format_utc_timestamp_known_value() {
    assert_eq!(
        format_utc_timestamp(ts()),
        "May 8, 2026 \u{00b7} 17:44 UTC"
    );
}

#[test]
fn format_utc_timestamp_epoch() {
    assert_eq!(
        format_utc_timestamp(SystemTime::UNIX_EPOCH),
        "Jan 1, 1970 \u{00b7} 00:00 UTC"
    );
}

// ---------------------------------------------------------------------------
// svg_for_grade — grade letter presence
// ---------------------------------------------------------------------------

#[test]
fn svg_contains_grade_a_plus() {
    let svg = svg_for_grade("example.com", "A+", "lens", 95.0, ts());
    assert!(svg.contains(">A+<"), "A+ not found in SVG");
}

#[test]
fn svg_all_known_grades_present() {
    for grade in &["A+", "A", "B", "C", "D", "F"] {
        let svg = svg_for_grade("example.com", grade, "lens", 80.0, ts());
        assert!(
            svg.contains(&format!(">{grade}<")),
            "grade {grade} not found in SVG"
        );
    }
}

#[test]
fn svg_error_grade_renders_question_mark() {
    let svg = svg_for_grade("example.com", "error", "lens", 0.0, ts());
    assert!(svg.contains(">?<"), "error grade should render ?");
    assert!(!svg.contains(">error<"));
}

#[test]
fn svg_unknown_grade_renders_question_mark() {
    let svg = svg_for_grade("example.com", "Z", "lens", 0.0, ts());
    assert!(svg.contains(">?<"), "unknown grade should render ?");
}

// ---------------------------------------------------------------------------
// svg_for_grade — score percentage
// ---------------------------------------------------------------------------

#[test]
fn svg_score_shown_for_known_grade() {
    let svg = svg_for_grade("example.com", "A", "lens", 91.7, ts());
    assert!(svg.contains("91.7%"), "score must appear for known grade");
}

#[test]
fn svg_score_hidden_for_unknown_grade() {
    let svg = svg_for_grade("example.com", "?", "lens", 0.0, ts());
    assert!(!svg.contains('%'), "score % must not appear for unknown grade");
}

// ---------------------------------------------------------------------------
// svg_for_grade — timestamp
// ---------------------------------------------------------------------------

#[test]
fn svg_timestamp_present() {
    let svg = svg_for_grade("example.com", "A", "lens", 80.0, ts());
    assert!(svg.contains("May 8, 2026"), "timestamp date must appear");
    assert!(svg.contains("17:44 UTC"), "timestamp time must appear");
}

// ---------------------------------------------------------------------------
// svg_for_grade — escape
// ---------------------------------------------------------------------------

#[test]
fn svg_domain_with_ampersand_is_escaped() {
    let svg = svg_for_grade("a&b.com", "A", "lens", 80.0, ts());
    assert!(svg.contains("a&amp;b.com"));
    assert!(!svg.contains("a&b.com"));
}

#[test]
fn svg_label_with_angle_brackets_is_escaped() {
    let svg = svg_for_grade("example.com", "A", "<test>", 80.0, ts());
    assert!(svg.contains("&lt;test&gt;"));
    assert!(!svg.contains("<test>"));
}

// ---------------------------------------------------------------------------
// svg_for_grade — domain truncation in SVG
// ---------------------------------------------------------------------------

#[test]
fn svg_long_domain_truncated_in_text() {
    let domain = "a".repeat(50);
    let svg = svg_for_grade(&domain, "A", "lens", 80.0, ts());
    assert!(!svg.contains(&domain));
    assert!(svg.contains('\u{2026}'));
}

// ---------------------------------------------------------------------------
// svg_for_grade — canvas dimensions
// ---------------------------------------------------------------------------

#[test]
fn svg_has_correct_canvas_dimensions() {
    let svg = svg_for_grade("example.com", "A", "lens", 80.0, ts());
    assert!(svg.contains("width=\"1200\""));
    assert!(svg.contains("height=\"630\""));
}

// ---------------------------------------------------------------------------
// svg_to_png — dimensions and PNG validity
// ---------------------------------------------------------------------------

#[test]
fn png_is_1200x630() {
    let font_db = init_font_db();
    for grade in &["A+", "A", "B", "C", "D", "F", "error"] {
        let svg = svg_for_grade("example.com", grade, "lens", 80.0, ts());
        let png_bytes = svg_to_png(&svg, font_db.clone()).expect("render should succeed");

        use image::ImageReader;
        use std::io::Cursor;
        let img = ImageReader::new(Cursor::new(&png_bytes))
            .with_guessed_format()
            .expect("guessed format")
            .decode()
            .expect("decoded image");
        assert_eq!(img.width(), 1200, "width must be 1200 for grade {grade}");
        assert_eq!(img.height(), 630, "height must be 630 for grade {grade}");
    }
}

// ---------------------------------------------------------------------------
// ETag stability — same inputs produce same hash
// ---------------------------------------------------------------------------

#[test]
fn etag_is_deterministic() {
    use xxhash_rust::xxh3::xxh3_64;

    fn etag(domain: &str, grade: &str, label: &str, score_pct: f64) -> String {
        let input = format!("{domain}\x00{grade}\x00{label}\x00{score_pct:.1}");
        let hash = xxh3_64(input.as_bytes());
        format!("\"{hash:016x}\"")
    }

    let e1 = etag("example.com", "A", "lens", 80.0);
    let e2 = etag("example.com", "A", "lens", 80.0);
    assert_eq!(e1, e2, "same inputs must produce same ETag");

    let e3 = etag("example.com", "B", "lens", 80.0);
    assert_ne!(e1, e3, "different grade must produce different ETag");

    let e4 = etag("example.com", "A", "lens", 90.0);
    assert_ne!(e1, e4, "different score must produce different ETag");
}
