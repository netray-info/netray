//! `/api/v1/domains/{domain}` in `lens::app` (spec v2-api, requirements 10, 15): lens's real
//! `validate_domain`, the history store from `[history]`, 503 with history disabled.
//! Copied from crates/lens/tests/api_v1_domains.rs (spec v2-cutover, Phase 6); nothing dropped.
use crate::common;

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;
use serde_json::Value;
use tower::ServiceExt;

/// lens's `base_config`: the cache off, 60 per minute and a burst of 10 per IP, 1000 and 100
/// globally.
fn base_config() -> ApexConfig {
    let defaults = ApexConfig::default();
    ApexConfig {
        cache_enabled: false,
        cache_window_seconds: 300,
        limits: AdmissionLimits {
            rate_limit_per_ip_per_minute: 60,
            rate_limit_per_ip_burst: 10,
            rate_limit_global_per_minute: 1000,
            rate_limit_global_burst: 100,
            ..defaults.limits
        },
        ..defaults
    }
}

async fn app(history_on: bool) -> (Router, tempfile::TempDir) {
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    let mut state = common::state_with(base_config(), registry, plan);
    let (store, dir) = common::history_store().await;
    if history_on {
        let run = Run {
            profile_version: 3,
            engine_version: "0.26.0".into(),
            evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
            grade: Grade::A,
            complete: true,
            sections: vec![RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            }],
            checks: vec![RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            }],
            blocks: vec![],
        };
        store
            .record("example.com", &run, &[], 1_800_000_000, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
        state.history_store = Some(store);
    }
    (netray_web::host::apex::app(state), dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let mut req = Request::get(path).body(Body::empty()).unwrap();
    let peer: SocketAddr = "203.0.113.9:4000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn domain_read_normalises_with_the_apex_validation() {
    let (app, _dir) = app(true).await;
    let (status, body) = get(&app, "/api/v1/domains/Example.COM.").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["domain"], "example.com");
}

#[tokio::test]
async fn domain_read_refuses_a_wildcard_with_domain_invalid() {
    let (app, _dir) = app(true).await;
    let (status, body) = get(&app, "/api/v1/domains/%2A.example.com").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "DOMAIN_INVALID");
}

#[tokio::test]
async fn domain_read_with_history_disabled_is_503_unavailable() {
    let (app, _dir) = app(false).await;
    let (status, body) = get(&app, "/api/v1/domains/example.com").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "UNAVAILABLE");
}
