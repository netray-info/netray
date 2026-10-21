//! IP attribution (spec v2-ip-ui, requirement 14): the result page's IP tab opens on the compact
//! address table, followed in the same panel by the MaxMind attribution; the email panel has none.
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

/// Asserts the needles occur in `hay` in the given order.
fn in_order(hay: &str, needles: &[&str]) {
    let mut at = 0;
    for n in needles {
        at += hay[at..]
            .find(n)
            .unwrap_or_else(|| panic!("`{n}` missing or out of order in: {hay}"))
            + n.len();
    }
}

#[tokio::test]
async fn r14_the_ip_panel_follows_its_compact_table_with_the_maxmind_attribution() {
    let body = page(&run_of(fixture_json())).await;
    let ip = panel(&body, "ip");
    in_order(
        ip,
        &[
            r#"<table class="evidence-tabs__compact">"#,
            "</table>",
            r#"<p class="geoip-attribution">"#,
            r#"<a href="https://www.maxmind.com" rel="noopener noreferrer">MaxMind</a>"#,
            "</p>",
        ],
    );
    let email = panel(&body, "email");
    assert!(
        email.contains(r#"<table class="evidence-tabs__compact">"#),
        "{email}"
    );
    assert!(
        !email.contains("geoip-attribution"),
        "attribution in the email panel: {email}"
    );
}
