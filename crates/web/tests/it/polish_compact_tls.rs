//! Result polish, Phase 1 (spec v2-result-polish; C1, C5, C11-C14): a stored entry's TLS panel
//! holds a compact table instead of the evidence blocks.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::{Block, Shape, Value};
use netray_model::run::Run;
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";
const T: i64 = 1_800_000_000;
const SEP: &str = "\u{00B7}";

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

/// The multi-address TLS fixture: two addresses on 443, identical four-item chains.
fn fixture_run() -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/shop.example.com-3.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(v["run"].clone()).unwrap()
}

/// Returns the page body and the stored entry id.
async fn page(run: &Run) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, run, &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let id = store.latest(DOMAIN).await.unwrap().unwrap().id;
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
    (String::from_utf8_lossy(&bytes).into_owned(), id)
}

fn panel<'a>(body: &'a str, protocol: &str) -> &'a str {
    let at = body.find(r#"id="evidence""#).expect("#evidence missing");
    let ev = &body[at..];
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

fn caption(panel: &str) -> &str {
    let open = r#"<p class="evidence-tabs__caption">"#;
    let at = panel
        .find(open)
        .unwrap_or_else(|| panic!("no caption: {panel}"))
        + open.len();
    let end = panel[at..].find("</p>").unwrap();
    &panel[at..at + end]
}

fn header(panel: &str) -> &str {
    let open = r#"<table class="evidence-tabs__compact"><thead><tr>"#;
    let at = panel
        .find(open)
        .unwrap_or_else(|| panic!("no compact table: {panel}"))
        + open.len();
    let end = panel[at..].find("</tr></thead>").unwrap();
    &panel[at..at + end]
}

fn body_rows(panel: &str) -> Vec<&str> {
    let open = "</thead><tbody>";
    let at = panel.find(open).expect("no tbody") + open.len();
    let end = panel[at..].find("</tbody>").unwrap();
    panel[at..at + end]
        .split("</tr>")
        .filter(|r| !r.is_empty())
        .collect()
}

fn chain_mut<'a>(run: &'a mut Run, qualifier: &str) -> &'a mut Vec<Vec<(String, Value)>> {
    let block = run
        .blocks
        .iter_mut()
        .find(|b| b.id.to_string() == format!("tls.chain@{qualifier}"))
        .unwrap();
    match &mut block.shape {
        Shape::Chain(items) => items,
        _ => panic!("not a chain"),
    }
}

fn set(item: &mut Vec<(String, Value)>, key: &str, value: Value) {
    item.retain(|(k, _)| k != key);
    item.push((key.to_string(), value));
}

#[tokio::test]
async fn c1_c5_c11_a_stored_tls_panel_holds_caption_table_links_and_no_blocks() {
    let (body, id) = page(&fixture_run()).await;
    let tls = panel(&body, "tls");
    assert!(!tls.contains(r#"class="evidence-block""#), "{tls}");
    let cap = tls.find(r#"<p class="evidence-tabs__caption">"#).unwrap();
    let table = tls
        .find(r#"<table class="evidence-tabs__compact">"#)
        .unwrap();
    let links = tls
        .find(&format!(
            r#"<a class="evidence-tabs__all" href="/r/{id}/tls">All evidence →</a>"#
        ))
        .expect("links line lacks the all-evidence link");
    assert!(cap < table && table < links, "order: {tls}");
    assert_eq!(
        caption(tls),
        format!("Certificate chain {SEP} port 443 {SEP} 2 addresses, consistent")
    );
    assert_eq!(
        header(tls),
        "<th>#</th><th>Subject</th><th>Issuer</th><th>Key</th><th>Not after</th>"
    );
    let rows = body_rows(tls);
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert_eq!(
        rows[0],
        "<tr><td>0</td><td><strong>shop.example.com</strong></td><td>Example TLS Issuing ECC CA 1</td><td>ECDSA P-256</td><td><strong>2026-12-25</strong></td>"
    );
    assert!(rows[1].contains("<td>ECDSA P-256</td>"), "{}", rows[1]);
    assert!(!rows[1].contains("<strong>ECDSA"), "{}", rows[1]);
}

#[tokio::test]
async fn c1_a_panel_without_compact_sources_keeps_only_the_links_line() {
    let mut run = fixture_run();
    run.blocks.retain(|b| b.id.module() != "ip");
    for check in &mut run.checks {
        check.evidence.retain(|id| id.module() != "ip");
    }
    let (body, id) = page(&run).await;
    let ip = panel(&body, "ip");
    assert!(!ip.contains("evidence-tabs__caption"), "{ip}");
    assert!(!ip.contains("evidence-tabs__compact"), "{ip}");
    assert!(
        ip.contains(&format!(
            r#"<a class="evidence-tabs__all" href="/r/{id}/ip">All evidence →</a>"#
        )),
        "{ip}"
    );
}

#[tokio::test]
async fn c12_differing_chains_name_the_first_address() {
    let mut run = fixture_run();
    set(
        &mut chain_mut(&mut run, "198.51.100.10:443")[0],
        "fingerprint_sha256",
        Value::Text("AA:BB".into()),
    );
    let (body, _) = page(&run).await;
    assert_eq!(
        caption(panel(&body, "tls")),
        format!(
            "Certificate chain {SEP} port 443 {SEP} 2 addresses, chains differ {SEP} showing 192.0.2.10"
        )
    );
}

#[tokio::test]
async fn c13_self_signed_and_rsa_cells_and_a_single_address() {
    let mut run = fixture_run();
    run.blocks
        .retain(|b| b.id.to_string() != "tls.chain@198.51.100.10:443");
    for check in &mut run.checks {
        check
            .evidence
            .retain(|id| id.to_string() != "tls.chain@198.51.100.10:443");
    }
    let chain = chain_mut(&mut run, "192.0.2.10:443");
    set(&mut chain[1], "key", Value::Text("RSA 2048".into()));
    set(&mut chain[3], "self_signed", Value::Bool(true));
    let (body, _) = page(&run).await;
    let tls = panel(&body, "tls");
    assert_eq!(
        caption(tls),
        format!("Certificate chain {SEP} port 443 {SEP} 1 address, consistent")
    );
    let rows = body_rows(tls);
    assert!(rows[3].contains("<td>self-signed</td>"), "{}", rows[3]);
    assert!(rows[1].contains("<td>RSA 2048</td>"), "{}", rows[1]);
}

#[tokio::test]
async fn c14_without_a_chain_block_the_addresses_table_is_the_source() {
    let mut run = fixture_run();
    let is_chain = |b: &Block| b.id.module() == "tls" && b.id.name() == "chain";
    let gone: Vec<_> = run
        .blocks
        .iter()
        .filter(|b| is_chain(b))
        .map(|b| b.id.clone())
        .collect();
    run.blocks.retain(|b| !is_chain(b));
    for check in &mut run.checks {
        check.evidence.retain(|id| !gone.contains(id));
    }
    let (body, _) = page(&run).await;
    let tls = panel(&body, "tls");
    assert_eq!(caption(tls), format!("Addresses {SEP} 4 tested"));
    assert_eq!(
        header(tls),
        "<th>Address</th><th>Port</th><th>Outcome</th><th>Message</th>"
    );
    assert!(!tls.contains(r#"class="evidence-block""#), "{tls}");
}
