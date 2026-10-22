//! v2-ui-polish requirement 10 (C2, C8, C9): a fact cell with long values spans the grid's row.
use std::fs;
use std::path::PathBuf;

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::components::{Cell, FACT_WIDE_CHARS, FactCell, FactsGridView, facts_grid};
use netray_web::pages::views::{ViewContext, render_with};

fn cell(label: &str, values: &[&str], chips: bool) -> FactCell {
    FactCell {
        label: label.into(),
        values: values.iter().map(|v| Cell::from(*v)).collect(),
        chips,
        tone: None,
    }
}

/// The markup of the cell labelled `label`, from its opening tag to the next cell.
fn cell_markup<'a>(html: &'a str, label: &str) -> &'a str {
    html.split("<div class=\"facts-grid__cell")
        .skip(1)
        .find(|part| part.contains(&format!(">{label}<")))
        .unwrap_or_else(|| panic!("no cell `{label}` in: {html}"))
}

#[test]
fn wide_threshold_is_sixty_four_chars() {
    assert_eq!(FACT_WIDE_CHARS, 64);
}

#[test]
fn wide_counts_chars_of_the_joined_texts() {
    let n = FACT_WIDE_CHARS;
    assert!(!cell("L", &["a".repeat(n).as_str()], false).wide());
    assert!(cell("L", &["a".repeat(n + 1).as_str()], false).wide());
    // n + 1 two-byte chars: wide by chars; n two-byte chars (2n bytes): not wide
    assert!(cell("L", &["é".repeat(n + 1).as_str()], false).wide());
    assert!(!cell("L", &["é".repeat(n).as_str()], false).wide());
    // chips: three names of 21 chars joined with ", " = 63 + 4 = 67 > 64
    let names = ["a".repeat(21), "b".repeat(21), "c".repeat(21)];
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    assert!(cell("L", &refs, true).wide());
    // three names of 20 chars: 60 + 4 = 64, not wide
    let names = ["a".repeat(20), "b".repeat(20), "c".repeat(20)];
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    assert!(!cell("L", &refs, true).wide());
}

#[test]
fn facts_grid_marks_only_wide_cells() {
    let long = "x".repeat(FACT_WIDE_CHARS + 1);
    let html = facts_grid(&FactsGridView {
        cells: vec![
            cell("Short", &["200"], false),
            cell("Long", &[&long], false),
        ],
    })
    .unwrap();
    assert_eq!(html.matches("data-wide").count(), 1, "in: {html}");
    assert!(
        cell_markup(&html, "Long").contains("data-wide"),
        "in: {html}"
    );
    assert!(
        !cell_markup(&html, "Short").contains("data-wide"),
        "in: {html}"
    );
}

#[test]
fn facts_grid_css_spans_the_wide_cell() {
    let css =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/facts_grid.css"))
            .expect("facts_grid.css");
    let at = css
        .find(".facts-grid__cell[data-wide]")
        .expect("rule `.facts-grid__cell[data-wide]` missing");
    let body = &css[at..];
    let body = &body[..body.find('}').expect("unclosed rule")];
    assert!(body.contains("grid-column: 1 / -1"), "rule: {body}");
}

fn t(k: &str, v: Value) -> (String, Value) {
    (k.to_string(), v)
}

#[test]
fn tls_leaf_with_long_san_list_marks_alternative_names_wide() {
    let leaf = vec![
        t("position", Value::Int(0)),
        t("subject", Value::Text("CN=example.com".into())),
        t("issuer", Value::Text("CN=Example Issuing CA".into())),
        t(
            "sans",
            Value::Text(
                "example.com, www.example.com, api.example.com, mail.example.com, cdn.example.com, static.example.org"
                    .into(),
            ),
        ),
        t("not_before", Value::Time("2026-10-07T00:00:00Z".into())),
        t("not_after", Value::Time("2027-01-06T00:00:00Z".into())),
        t("days_remaining", Value::Int(89)),
        t("key", Value::Text("ECDSA P-256".into())),
        t("signature_algorithm", Value::Text("ecdsa-with-SHA256".into())),
        t("self_signed", Value::Bool(false)),
        t("expired", Value::Bool(false)),
    ];
    let addresses = Block {
        id: BlockId::parse("tls.addresses").unwrap(),
        title: "Addresses".into(),
        shape: Shape::Table {
            key: "address".into(),
            columns: ["address", "port", "outcome", "message"]
                .map(String::from)
                .to_vec(),
            rows: vec![vec![
                Value::Addr("192.0.2.1".parse().unwrap()),
                Value::Int(443),
                Value::Text("ok".into()),
                Value::Text(String::new()),
            ]],
        },
    };
    let chain = Block {
        id: BlockId::parse("tls.chain@192.0.2.1:443").unwrap(),
        title: "Certificate chain".into(),
        shape: Shape::Chain(vec![leaf]),
    };
    let handshake = Block {
        id: BlockId::parse("tls.handshake@192.0.2.1:443").unwrap(),
        title: "Handshake".into(),
        shape: Shape::Fields(vec![
            t("version", Value::Text("TLSv1.3".into())),
            t("cipher_suite", Value::Text("TLS_AES_256_GCM_SHA384".into())),
            t("handshake_ms", Value::Duration(22)),
        ]),
    };
    let all = vec![addresses, chain, handshake];
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses: &[],
        decorate: &decorate,
        feeds: &[],
    };
    let html = render_with(&all[0], &ctx, &all).expect("tls.addresses is claimed");
    assert!(
        cell_markup(&html, "Alternative names").contains("data-wide"),
        "in: {html}"
    );
}
