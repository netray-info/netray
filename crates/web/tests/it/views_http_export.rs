//! v2-http-ui Phase 3 (C4, C12): the exports keep the raw http.caching keys and values.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::export::{ExportInput, csv, json};

fn caching() -> Block {
    Block {
        id: BlockId::parse("http.caching").unwrap(),
        title: "Caching".into(),
        shape: Shape::Fields(vec![
            ("max_age".into(), Value::Int(3600)),
            ("etag_weak".into(), Value::Bool(true)),
            ("etag".into(), Value::Text("W/\"abc\"".into())),
        ]),
    }
}

#[test]
fn c4_the_json_export_keeps_max_age_and_etag_weak() {
    let blocks = [caching()];
    let title = Vec::new();
    let input = ExportInput {
        target: "example.com",
        title: &title,
        band: None,
        blocks: &blocks,
        errors: &[],
        ports: &[],
    };
    let doc: serde_json::Value = serde_json::from_str(&json(&input)).expect("json");
    let text = doc.to_string();
    assert!(text.contains(r#""max_age""#), "{text}");
    assert!(text.contains(r#""etag_weak""#), "{text}");
    let fields = &doc["blocks"][0]["shape"];
    let flat = fields.to_string();
    assert!(flat.contains("3600"), "{flat}");
    assert!(flat.contains("true"), "{flat}");
}

#[test]
fn c12_the_csv_export_keeps_max_age_and_its_value() {
    let blocks = [caching()];
    let title = Vec::new();
    let input = ExportInput {
        target: "example.com",
        title: &title,
        band: None,
        blocks: &blocks,
        errors: &[],
        ports: &[],
    };
    let out = csv(&input);
    assert!(out.contains("max_age"), "{out}");
    assert!(out.contains("3600"), "{out}");
}
