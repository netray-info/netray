/// Unit tests for OG card rendering (SVG template and PNG rasterization).
use lens::og::fonts::init_font_db;
use lens::og::render::{svg_for_grade, svg_to_png, truncate_domain, xml_escape};

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
// svg_for_grade — grade letter presence
// ---------------------------------------------------------------------------

#[test]
fn svg_contains_grade_a_plus() {
    let svg = svg_for_grade("example.com", "A+", "lens");
    assert!(svg.contains(">A+<"), "A+ not found in SVG");
}

#[test]
fn svg_all_known_grades_present() {
    for grade in &["A+", "A", "B", "C", "D", "F"] {
        let svg = svg_for_grade("example.com", grade, "lens");
        assert!(
            svg.contains(&format!(">{grade}<")),
            "grade {grade} not found in SVG"
        );
    }
}

#[test]
fn svg_error_grade_renders_question_mark() {
    let svg = svg_for_grade("example.com", "error", "lens");
    assert!(svg.contains(">?<"), "error grade should render ?");
    assert!(!svg.contains(">error<"));
}

#[test]
fn svg_unknown_grade_renders_question_mark() {
    let svg = svg_for_grade("example.com", "Z", "lens");
    assert!(svg.contains(">?<"), "unknown grade should render ?");
}

// ---------------------------------------------------------------------------
// svg_for_grade — escape
// ---------------------------------------------------------------------------

#[test]
fn svg_domain_with_ampersand_is_escaped() {
    let svg = svg_for_grade("a&b.com", "A", "lens");
    assert!(svg.contains("a&amp;b.com"));
    assert!(!svg.contains("a&b.com"));
}

#[test]
fn svg_label_with_angle_brackets_is_escaped() {
    let svg = svg_for_grade("example.com", "A", "<test>");
    assert!(svg.contains("&lt;test&gt;"));
    assert!(!svg.contains("<test>"));
}

// ---------------------------------------------------------------------------
// svg_for_grade — domain truncation in SVG
// ---------------------------------------------------------------------------

#[test]
fn svg_long_domain_truncated_in_text() {
    let domain = "a".repeat(50);
    let svg = svg_for_grade(&domain, "A", "lens");
    // Full 50-char domain must NOT appear verbatim in SVG
    assert!(!svg.contains(&domain));
    // Ellipsis must be present
    assert!(svg.contains('\u{2026}'));
}

// ---------------------------------------------------------------------------
// svg_for_grade — canvas dimensions
// ---------------------------------------------------------------------------

#[test]
fn svg_has_correct_canvas_dimensions() {
    let svg = svg_for_grade("example.com", "A", "lens");
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
        let svg = svg_for_grade("example.com", grade, "lens");
        let png_bytes = svg_to_png(&svg, font_db.clone()).expect("render should succeed");

        // Decode using the image crate to check dimensions.
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

    fn etag(domain: &str, grade: &str, label: &str) -> String {
        let input = format!("{domain}\x00{grade}\x00{label}");
        let hash = xxh3_64(input.as_bytes());
        format!("\"{hash:016x}\"")
    }

    let e1 = etag("example.com", "A", "lens");
    let e2 = etag("example.com", "A", "lens");
    assert_eq!(e1, e2, "same inputs must produce same ETag");

    let e3 = etag("example.com", "B", "lens");
    assert_ne!(e1, e3, "different grade must produce different ETag");
}
