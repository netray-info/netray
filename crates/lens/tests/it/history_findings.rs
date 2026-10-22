//! Stored checks carry findings and evidence references (specs/features/v2-api/spec.md,
//! requirement 4, criteria C4 and C34).
//!
//! The TLS module is a stub: `tls.caa_compliant` warns with one message and references the
//! block `dns.caa`; the HTTP module answers not applicable. Lens runs with
//! `tests/fixtures/lens.production.toml` and a `HistoryStore` on a temp file, driven in-process;
//! the entry is read back with `store.entries`, polled because the run task records after the
//! response.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{dns_golden_raw, email_golden, facts_golden_raw, ip_golden, registry_with};
use lens::config::Config;
use lens::routes::{api_router, badge_router};
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{Block, BlockId, Shape};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use netray_store::HistoryStore;
use tempfile::NamedTempFile;
use tower::ServiceExt;

const MESSAGE: &str = "CAA record does not authorise the issuing CA";

/// A TLS module that measures one check: `tls.caa_compliant` warns and references `dns.caa`.
struct StubTls;

impl Module for StubTls {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn needs_address_records(&self) -> bool {
        false
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async {
            let caa = BlockId::parse("dns.caa").unwrap();
            SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse("tls.caa_compliant").unwrap(),
                    status: Status::Warn,
                    findings: vec![MESSAGE.to_string()],
                    evidence: vec![caa.clone()],
                }],
                evidence: vec![Block {
                    id: caa,
                    title: "CAA".to_string(),
                    shape: Shape::Text("0 issue \"example.org\"".to_string()),
                }],
            }
        })
    }
}

/// An HTTP module whose section is not applicable.
struct StubHttp;

impl Module for StubHttp {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn needs_address_records(&self) -> bool {
        false
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async {
            SectionOutcome::NotApplicable {
                reason: "no web server".to_string(),
            }
        })
    }
}

/// C4/C34: a measured check is stored with its message in `findings` and its referenced block in
/// `evidence`; the checks of a not-applicable section carry neither.
#[tokio::test]
async fn history_findings_c4_c34_stored_check_carries_findings_and_evidence() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.rate_limit.per_ip_per_minute = 100;
    config.rate_limit.per_ip_burst = 100;
    let db = NamedTempFile::new().unwrap();
    config.history.enabled = true;
    config.history.db_path = db.path().to_path_buf();
    let store = HistoryStore::open(db.path()).await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);

    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        Box::new(StubHttp),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        Box::new(StubTls),
    );
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = Some(store.clone());

    let (api, _) = api_router().split_for_parts();
    let (badge, _) = badge_router().split_for_parts();
    let app = Router::new()
        .merge(api.with_state(state.clone()))
        .merge(badge.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));

    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();

    let mut entries = Vec::new();
    for _ in 0..250 {
        entries = store.entries("example.com").await.unwrap();
        if !entries.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(entries.len(), 1, "the run is recorded");
    let run = &entries[0].run;

    let caa = run
        .checks
        .iter()
        .find(|c| c.id.to_string() == "tls.caa_compliant")
        .expect("tls.caa_compliant is stored");
    assert_eq!(caa.status, Status::Warn);
    assert_eq!(caa.findings, vec![MESSAGE.to_string()]);
    assert_eq!(caa.evidence, vec![BlockId::parse("dns.caa").unwrap()]);

    let http: Vec<_> = run
        .checks
        .iter()
        .filter(|c| c.id.to_string().starts_with("http."))
        .collect();
    assert!(
        !http.is_empty(),
        "the not-applicable section stores its catalogue checks"
    );
    for c in http {
        assert_eq!(c.status, Status::NotApplicable, "{}", c.id);
        assert!(c.findings.is_empty(), "{} carries findings", c.id);
        assert!(c.evidence.is_empty(), "{} carries evidence", c.id);
    }
}
