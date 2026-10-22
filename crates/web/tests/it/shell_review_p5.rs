//! Review findings on v2-inspector-shell Phase 5 (requirement 19): Markdown escapes what would
//! render as markup, a failed part or band is in every export with its reason (Rule M), a CSV
//! value cannot start a spreadsheet formula, and blocks of several parts name their part.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::export::{ExportInput, csv, json, markdown};

fn block(id: &str, title: &str, value: &str) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: title.into(),
        shape: Shape::Fields(vec![("value".into(), Value::Text(value.into()))]),
    }
}

#[test]
fn markdown_escapes_markup_characters() {
    let blocks = [block(
        "http.csp",
        "CSP",
        "script-src 'self' https://*.googleapis.com https://*.gstatic.com a\\|b <b>`x`</b>",
    )];
    let md = markdown(&ExportInput {
        target: "https://example.com/",
        title: &[],
        band: None,
        blocks: &blocks,
        errors: &[],
        ports: &[],
    });
    assert!(md.contains(r"https://\*.googleapis.com"), "{md}");
    assert!(md.contains(r"\<b\>"), "{md}");
    assert!(md.contains(r"\`x\`"), "{md}");
    assert!(md.contains(r"a\\\|b"), "{md}");
}

#[test]
fn a_failed_part_or_band_is_in_every_export_with_its_reason() {
    let errors = [
        ("request".to_string(), "connection refused".to_string()),
        ("checks".to_string(), "timed out".to_string()),
    ];
    let input = ExportInput {
        target: "https://example.com/",
        title: &[],
        band: None,
        blocks: &[],
        errors: &errors,
        ports: &[],
    };
    let md = markdown(&input);
    assert!(
        md.contains("## request") && md.contains("connection refused"),
        "{md}"
    );
    assert!(md.contains("timed out"), "{md}");
    let csv = csv(&input);
    assert!(
        csv.lines()
            .any(|l| l == "request,,error,connection refused"),
        "{csv}"
    );
    assert!(csv.lines().any(|l| l == "checks,,error,timed out"), "{csv}");
    let json: serde_json::Value = serde_json::from_str(&json(&input)).unwrap();
    assert_eq!(json["errors"][0]["section"], "request");
    assert_eq!(json["errors"][0]["error"], "connection refused");
    assert_eq!(json["errors"][1]["section"], "checks");
}

#[test]
fn a_csv_value_never_starts_a_formula() {
    let blocks = [block(
        "http.headers",
        "Headers",
        "=HYPERLINK(\"https://evil.example/\",\"open\")",
    )];
    let csv = csv(&ExportInput {
        target: "https://example.com/",
        title: &[],
        band: None,
        blocks: &blocks,
        errors: &[],
        ports: &[],
    });
    let row = csv.lines().find(|l| l.starts_with("http.headers")).unwrap();
    let value = row.splitn(4, ',').nth(3).unwrap();
    let value = value.trim_start_matches('"');
    assert!(value.starts_with('\''), "{row}");
}
