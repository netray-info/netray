//! Result polish, Phase 1 review (spec v2-result-polish, requirements 5 and 8): edge cases of the
//! compact tables the review found: no MX block, an unknown DKIM key type, repeated records, Ed25519.
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

/// The shop fixture's run as JSON, so a scenario edits blocks as the store reads them.
fn fixture_json() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/shop.example.com-3.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    v["run"].clone()
}

fn run_of(v: serde_json::Value) -> Run {
    serde_json::from_value(v).unwrap()
}

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

fn caption(text: &str) -> String {
    format!(r#"<p class="evidence-tabs__caption">{text}</p>"#)
}

/// Drops the blocks `ids` and every reference to them.
fn without(mut v: serde_json::Value, ids: &[&str]) -> serde_json::Value {
    v["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| !ids.contains(&b["id"].as_str().unwrap()));
    for c in v["checks"].as_array_mut().unwrap() {
        if let Some(ev) = c["evidence"].as_array_mut() {
            ev.retain(|e| !ids.contains(&e.as_str().unwrap()));
        }
    }
    v
}

fn block_mut<'a>(v: &'a mut serde_json::Value, id: &str) -> &'a mut serde_json::Value {
    v["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["id"] == id)
        .unwrap()
}

#[tokio::test]
async fn an_unanswered_mx_lookup_claims_no_host_count() {
    let v = without(fixture_json(), &["email.mx"]);
    let body = page(&run_of(v)).await;
    let email = panel(&body, "email");
    assert!(email.contains(&caption("Mail records")), "{email}");
    assert!(!email.contains("MX host"), "{email}");
}

#[tokio::test]
async fn a_dkim_key_without_a_type_gets_no_dangling_comma() {
    let mut v = fixture_json();
    let dkim = block_mut(&mut v, "email.dkim");
    dkim["shape"]["data"]["rows"] = serde_json::json!([[
        {"t": "text", "v": "mail#1"},
        {"t": "text", "v": "mail"},
        {"t": "text", "v": "revoked"},
        {"t": "text", "v": ""},
        {"t": "text", "v": ""}
    ]]);
    let body = page(&run_of(v)).await;
    let email = panel(&body, "email");
    assert!(
        email.contains("<tr><td>DKIM</td><td>mail: revoked</td></tr>"),
        "{email}"
    );
}

#[tokio::test]
async fn every_repeated_record_gets_its_own_row() {
    let mut v = fixture_json();
    let spf = block_mut(&mut v, "email.spf");
    spf["shape"]["data"].as_array_mut().unwrap().push(
        serde_json::json!(["record", {"t": "text", "v": "v=spf1 include:spf.example.com -all"}]),
    );
    let body = page(&run_of(v)).await;
    let email = panel(&body, "email");
    assert!(
        email.contains("<tr><td>SPF</td><td>v=spf1 mx -all</td></tr>"),
        "{email}"
    );
    assert!(
        email.contains("<tr><td>SPF</td><td>v=spf1 include:spf.example.com -all</td></tr>"),
        "{email}"
    );
}

#[tokio::test]
async fn an_ed25519_key_drops_its_fixed_size() {
    let mut v = fixture_json();
    for id in ["tls.chain@192.0.2.10:443", "tls.chain@198.51.100.10:443"] {
        let chain = block_mut(&mut v, id);
        for pair in chain["shape"]["data"][0].as_array_mut().unwrap() {
            if pair[0] == "key" {
                pair[1] = serde_json::json!({"t": "text", "v": "Ed25519 256"});
            }
        }
    }
    let body = page(&run_of(v)).await;
    let tls = panel(&body, "tls");
    assert!(tls.contains("<td>Ed25519</td>"), "{tls}");
    assert!(!tls.contains("Ed25519 256"), "{tls}");
}
