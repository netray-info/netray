//! v2-api requirement 34: every row of SDD section 7.1 has its path and method in the OpenAPI
//! document the API serves.
use netray_web::api::openapi;
use serde_json::Value;

/// The `(method, path)` pairs of the first cells of the table rows between `### 7.1` and
/// `### 7.2`; a code span `/path` inherits the method of the cell's last span that named one.
fn pairs(sdd: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut on = false;
    for line in sdd.lines() {
        if line.starts_with("### 7.1") {
            on = true;
            continue;
        }
        if line.starts_with("### 7.2") {
            break;
        }
        if !on || !line.starts_with('|') {
            continue;
        }
        let cell = line.split('|').nth(1).unwrap_or("");
        let mut method = String::new();
        for (i, span) in cell.split('`').enumerate() {
            if i % 2 == 0 {
                continue;
            }
            let span = span.trim();
            let path = match span.split_once(' ') {
                Some((m, p)) if m.chars().all(|c| c.is_ascii_uppercase()) => {
                    method = m.to_lowercase();
                    p.trim()
                }
                _ => span,
            };
            if path.starts_with('/') && !method.is_empty() {
                out.push((method.clone(), path.to_string()));
            }
        }
    }
    out
}

/// `"/api/v1<path> <method>"` for every pair the document lacks.
fn missing(sdd: &str, doc: &Value) -> Vec<String> {
    pairs(sdd)
        .into_iter()
        .filter(|(m, p)| doc["paths"][format!("/api/v1{p}")].get(m).is_none())
        .map(|(m, p)| format!("/api/v1{p} {m}"))
        .collect()
}

#[test]
fn every_row_of_section_7_1_is_in_the_openapi_document() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../specs/sdd/v2.md");
    let sdd = std::fs::read_to_string(path).expect("read specs/sdd/v2.md");
    let doc = serde_json::to_value(openapi()).unwrap();

    let n = pairs(&sdd).len();
    assert!(
        n >= 23,
        "only {n} (method, path) pairs parsed from section 7.1"
    );
    let absent = missing(&sdd, &doc);
    assert!(absent.is_empty(), "OpenAPI document lacks: {absent:?}");

    let planted = "### 7.1 x\n| `GET /ghost` | x |\n### 7.2 y\n";
    assert_eq!(
        missing(&planted, &doc),
        vec!["/api/v1/ghost get".to_string()]
    );
}
