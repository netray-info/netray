//! The generated netray column of /compare (v2-pages requirements 23, 24, 25).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_model::CheckId;
use netray_web::pages::{self, compare};
use tower::ServiceExt;

fn row(src: &str) -> compare::Row {
    toml::from_str::<compare::Row>(src).expect("row parses")
}

#[test]
fn netray_cell_is_generated_from_checks_depth_and_product() {
    let full =
        row("label = \"x\"\nchecks = [\"dns.dnssec\", \"dns.caa\"]\ndepth = \"full\"\ncells = []");
    let cell = full.netray();
    assert_eq!(cell.symbol, "✓");
    assert_eq!(cell.note, "DNSSEC signed, CAA records");

    // the literal titles are the catalogue's titles
    let cat = netray_engine::catalogue();
    for (id, title) in [("dns.dnssec", "DNSSEC signed"), ("dns.caa", "CAA records")] {
        let spec = cat
            .get(&CheckId::parse(id).unwrap())
            .expect("id in catalogue");
        assert_eq!(spec.title, title, "catalogue title of {id}");
    }

    let partial = row(
        "label = \"x\"\nchecks = [\"dns.dnssec\", \"dns.caa\"]\ndepth = \"partial\"\ncells = []",
    );
    assert_eq!(partial.netray().symbol, "~");

    let none = row("label = \"x\"\nchecks = []\ncells = []");
    let cell = none.netray();
    assert_eq!(cell.symbol, "✗");
    assert_eq!(cell.note, "");

    for (product, symbol) in [("yes", "✓"), ("partial", "~"), ("no", "✗")] {
        let r = row(&format!(
            "label = \"x\"\nproduct = \"{product}\"\nnote = \"API access\"\ncells = []"
        ));
        let cell = r.netray();
        assert_eq!(cell.symbol, symbol, "product {product}");
        assert_eq!(cell.note, "API access", "product {product}");
    }
}

#[test]
fn real_rows_have_known_ids_and_exactly_one_basis() {
    let rows = compare::rows();
    assert!(!rows.is_empty(), "compare.toml has rows");
    let unknown = compare::unknown_ids(&rows);
    assert!(unknown.is_empty(), "unknown check ids: {unknown:?}");

    for r in &rows {
        let by_checks = r.checks.is_some();
        let by_product = r.product.is_some();
        assert!(
            by_checks != by_product,
            "{}: needs checks XOR product (checks: {by_checks}, product: {by_product})",
            r.label
        );
        if let Some(checks) = &r.checks {
            if checks.is_empty() {
                assert!(
                    r.note.is_none(),
                    "{}: empty checks carries no note",
                    r.label
                );
            } else {
                assert!(
                    matches!(r.depth.as_deref(), Some("full" | "partial")),
                    "{}: non-empty checks needs depth full|partial",
                    r.label
                );
            }
        } else {
            assert!(r.note.is_some(), "{}: product row needs a note", r.label);
        }
    }
}

#[test]
fn unknown_ids_names_a_planted_unknown_check() {
    let planted =
        row("label = \"planted\"\nchecks = [\"dns.nonexistent\"]\ndepth = \"full\"\ncells = []");
    let mut rows = compare::rows();
    rows.push(&planted);
    assert_eq!(
        compare::unknown_ids(&rows),
        vec!["planted: dns.nonexistent".to_string()]
    );
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[tokio::test]
async fn rendered_netray_cell_equals_the_generated_cell() {
    let rows = compare::rows();
    let r = rows
        .iter()
        .find(|r| r.checks.as_ref().is_some_and(|c| !c.is_empty()))
        .expect("a row with checks");
    let cell = r.netray();

    let res = pages::router()
        .oneshot(
            Request::builder()
                .uri("/compare")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body =
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();

    let at = body
        .find(&escape(&r.label))
        .unwrap_or_else(|| panic!("label {:?} not on the page", r.label));
    let rest = &body[at..];
    let row_html = &rest[..rest.find("</tr>").unwrap_or(rest.len())];
    assert!(
        row_html.contains(&cell.symbol),
        "symbol {:?} in row: {row_html}",
        cell.symbol
    );
    assert!(
        row_html.contains(&escape(&cell.note)),
        "note {:?} in row: {row_html}",
        cell.note
    );
}
