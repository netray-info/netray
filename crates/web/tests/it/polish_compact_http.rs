//! Result polish, Phase 1 (spec v2-result-polish, requirement 7 and 4; C7, C16, C4, C20): the
//! HTTP panel of a stored entry shows a compact Response table of six security headers.
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
use serde_json::json;
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

/// The shop fixture with `http.headers` replaced by `headers` (name, value).
fn run_with_headers(headers: &[(&str, &str)]) -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/shop.example.com-3.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let rows: Vec<_> = headers
        .iter()
        .map(|(n, val)| json!([{"t": "text", "v": n}, {"t": "text", "v": val}]))
        .collect();
    let blocks = v["run"]["blocks"].as_array_mut().unwrap();
    let block = blocks
        .iter_mut()
        .find(|b| b["id"] == "http.headers")
        .expect("fixture has http.headers");
    block["shape"]["data"]["rows"] = json!(rows);
    serde_json::from_value(v["run"].clone()).unwrap()
}

async fn http_panel(run: &Run) -> String {
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
    let body = String::from_utf8_lossy(&bytes).into_owned();
    let ev = &body[body.find(r#"id="evidence""#).expect("#evidence missing")..];
    let open = r#"<div id="evidence-http" class="evidence-tabs__panel" data-panel="http""#;
    let rest = &ev[ev.find(open).expect("http panel missing") + open.len()..];
    let end = rest
        .find(r#"class="evidence-tabs__panel""#)
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

#[tokio::test]
async fn c7_c16_http_compact_lists_six_security_headers_in_order() {
    let run = run_with_headers(&[
        ("content-security-policy", "default-src self"),
        ("x-frame-options", "DENY"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("server", "example-edge"),
    ]);
    let panel = http_panel(&run).await;
    assert!(
        panel.contains(
            r#"<p class="evidence-tabs__caption">Response · 200 · h2 · https://shop.example.com/</p>"#
        ),
        "caption: {panel}"
    );
    let expected = concat!(
        r#"<table class="evidence-tabs__compact"><thead><tr><th>Header</th><th>Value</th></tr></thead><tbody>"#,
        "<tr><td>strict-transport-security</td><td>not sent</td></tr>",
        "<tr><td>content-security-policy</td><td>default-src self</td></tr>",
        "<tr><td>x-frame-options</td><td>DENY</td></tr>",
        "<tr><td>x-content-type-options</td><td>nosniff</td></tr>",
        "<tr><td>referrer-policy</td><td>no-referrer</td></tr>",
        "<tr><td>permissions-policy</td><td>not sent</td></tr>",
        "</tbody></table>",
    );
    assert!(panel.contains(expected), "table: {panel}");
}

#[tokio::test]
async fn c4_c20_a_long_header_value_is_truncated_with_a_title() {
    let mut csp = String::from("default-src self ");
    while csp.chars().count() < 200 {
        csp.push_str("img-src example.com ");
    }
    csp.truncate(200);
    assert_eq!(csp.chars().count(), 200);
    let exact: String = "a".repeat(64);
    let run = run_with_headers(&[
        ("content-security-policy", &csp),
        ("referrer-policy", &exact),
    ]);
    let panel = http_panel(&run).await;
    let short: String = csp.chars().take(63).collect();
    let long_cell = format!(r#"<td title="{csp}">{short}…</td>"#);
    assert!(
        panel.contains(&format!(
            "<tr><td>content-security-policy</td>{long_cell}</tr>"
        )),
        "long cell: {panel}"
    );
    assert!(
        panel.contains(&format!(
            "<tr><td>referrer-policy</td><td>{exact}</td></tr>"
        )),
        "64-char cell shown whole: {panel}"
    );
}
