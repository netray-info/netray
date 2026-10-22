//! Result polish, requirement 6 (spec v2-result-polish; C6, C15): the DNS panel's compact
//! "Key records" table.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::run::Run;
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

use netray_model::evidence::Shape;

const DOMAIN: &str = "example.com";
const T: i64 = 1_800_000_000;

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

/// The shop.example.com run: dns.a with two records, an empty dns.caa, dns.txt and dns.soa filled.
fn fixture_run() -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/shop.example.com-3.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(v["run"].clone()).unwrap()
}

/// Returns the page body and the stored entry id.
async fn page(run: &Run) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, run, &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn evidence(body: &str) -> &str {
    let at = body.find(r#"id="evidence""#).expect("#evidence missing");
    &body[at..]
}

/// The panel of `protocol`, from its opening tag to the next panel or the end.
fn panel<'a>(body: &'a str, protocol: &str) -> &'a str {
    let ev = evidence(body);
    let open = format!(
        r#"<div id="evidence-{protocol}" class="evidence-tabs__panel" data-panel="{protocol}""#
    );
    let at = ev
        .find(&open)
        .unwrap_or_else(|| panic!("panel `{open}` missing: {ev}"));
    let rest = &ev[at + open.len()..];
    let end = rest
        .find(r#"class="evidence-tabs__panel""#)
        .unwrap_or(rest.len());
    &rest[..end]
}

#[tokio::test]
async fn c15_the_dns_panel_lists_key_records_in_order() {
    let run = fixture_run();
    let answered = run
        .blocks
        .iter()
        .filter(|b| b.id.module() == "dns")
        .filter(|b| matches!(&b.shape, Shape::Records(r) if !r.is_empty()))
        .count();
    let body = page(&run).await;
    let dns = panel(&body, "dns");

    let caption = format!(
        r#"<p class="evidence-tabs__caption">Key records · {answered} record types answered</p>"#
    );
    assert!(dns.contains(&caption), "caption `{caption}` missing: {dns}");

    let start = dns
        .find(r#"<table class="evidence-tabs__compact">"#)
        .unwrap_or_else(|| panic!("no compact table: {dns}"));
    let table = &dns[start..start + dns[start..].find("</table>").expect("table end")];
    assert!(
        table.starts_with(
            r#"<table class="evidence-tabs__compact"><thead><tr><th>Type</th><th>Data</th><th>TTL</th></tr></thead><tbody>"#
        ),
        "{table}"
    );

    let rows = [
        "<tr><td>A</td><td>192.0.2.10</td><td>290</td></tr>",
        "<tr><td>A</td><td>198.51.100.10</td><td>290</td></tr>",
        "<tr><td>AAAA</td><td>2001:db8::10</td><td>160</td></tr>",
        "<tr><td>NS</td><td>ns1.shop.example.com.</td><td>77532</td></tr>",
        "<tr><td>CAA</td><td>no records</td><td></td></tr>",
        "<tr><td>DS</td>",
    ];
    let mut at = 0;
    for row in rows {
        at += table[at..]
            .find(row)
            .unwrap_or_else(|| panic!("row `{row}` missing or out of order: {table}"))
            + row.len();
    }
    for absent in ["<td>TXT</td>", "<td>SOA</td>", "<td>MX</td>"] {
        assert!(!table.contains(absent), "unexpected {absent}: {table}");
    }
}
