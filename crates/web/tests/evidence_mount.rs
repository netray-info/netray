//! Mount rules for `/r/<id>/<protocol>` in `lens::app` (spec v2-inspectors, Phase 4, C8).
//! Copied from crates/lens/tests/evidence_mount.rs (spec v2-cutover, Phase 6); nothing dropped.
mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{RETENTION_SECS, Recorded};
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;
use tower::ServiceExt;

const ENTRY_DOMAIN: &str = "example.com";
const NOW: i64 = RETENTION_SECS + 5000;

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

fn a_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn get(app: &Router, uri: &str) -> (StatusCode, String, String) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, ct, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn entry_card_and_evidence_view_are_answered_by_their_own_handlers() {
    let (history, _dir) = common::history_store().await;
    let id = match history
        .record(ENTRY_DOMAIN, &a_run(), &[], NOW, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };

    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    let mut state = common::state_with(base_config(), registry, plan);
    state.history_store = Some(Arc::clone(&history));
    let app = netray_web::host::apex::app(state);

    let (status, ct, body) = get(&app, &format!("/r/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/html"), "entry page content-type: {ct}");
    assert!(body.contains(ENTRY_DOMAIN));

    let (status, ct, _) = get(&app, &format!("/r/{id}/og.png")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct, "image/png");

    let (status, ct, body) = get(&app, &format!("/r/{id}/tls")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        ct.starts_with("text/html"),
        "evidence view content-type: {ct}"
    );
    assert!(body.contains(r#"data-protocol="tls""#), "body: {body}");
    assert!(body.contains(&format!("entry {id}")), "body: {body}");

    let (status, _, _) = get(&app, &format!("/r/{id}/tls/x")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
