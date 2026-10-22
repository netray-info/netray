//! Mount rules for the result router in `lens::app` (spec v2-result-page, Phase 2: C1 lens half, C16, C21).
//! Copied from crates/lens/tests/result_mount.rs (spec v2-cutover, Phase 6); the API path the
//! result router must not answer is `/api/v1/domains/example.com` instead of the dropped V1
//! `/api/check/example.com`; nothing else dropped.
use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use netray_engine::Registry;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{RETENTION_SECS, Recorded};
use netray_web::admission::ApexState;
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;
use tower::ServiceExt;

const ENTRY_DOMAIN: &str = "shop.example.com";
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

fn state() -> ApexState {
    let registry = Registry::new();
    let plan = common::default_plan(&registry);
    common::state_with(base_config(), registry, plan)
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

async fn get(app: &Router, uri: &str) -> (StatusCode, String, String, Option<String>) {
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
    let loc = resp
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        ct,
        String::from_utf8_lossy(&bytes).into_owned(),
        loc,
    )
}

const SPA_SHELL: &str = "<div id=\"root\"";
const IDLE_FORM: &str = "action=\"/\"";

/// Returns the app with history enabled, and the id of the recorded shop.example.com entry.
async fn app_with_entry() -> (Router, String, tempfile::TempDir) {
    let (history, dir) = common::history_store().await;
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
    let mut state = state();
    state.history_store = Some(history);
    (netray_web::host::apex::app(state), id.to_string(), dir)
}

fn app_without_history() -> Router {
    netray_web::host::apex::app(state())
}

fn is_result_router_page(body: &str) -> bool {
    body.contains("id=\"result\"") || (body.contains(IDLE_FORM) && body.contains("/inspect/dns"))
}

#[tokio::test]
async fn apex_serves_idle_and_result_pages_with_history_enabled() {
    let (app, id, _dir) = app_with_entry().await;

    let (status, _, body, _) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains(SPA_SHELL), "GET / is still the SPA shell");
    assert!(body.contains(IDLE_FORM), "idle form missing: {body}");
    assert!(body.contains("name=\"d\""), "idle input missing: {body}");
    assert!(body.contains("/inspect/dns"), "idle links missing: {body}");

    let (status, _, body, _) = get(&app, "/shop.example.com").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains(SPA_SHELL), "result is still the SPA shell");
    assert!(
        body.contains("id=\"result\""),
        "result page missing: {body}"
    );
    assert!(body.contains(&id), "entry id {id} missing: {body}");

    let (status, _, _, loc) = get(&app, "/?d=Example.com").await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(loc.as_deref(), Some("/example.com"));
}

#[tokio::test]
async fn result_router_does_not_answer_robots_share_or_api_paths() {
    let (app, id, _dir) = app_with_entry().await;
    // Entry exists, so a stray match of the result router would show id="result".
    assert!(!id.is_empty());

    let (status, ct, body, _) = get(&app, "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/plain"), "robots content-type: {ct}");
    assert!(body.contains("User-agent"), "robots body: {body}");
    assert!(!is_result_router_page(&body));

    for uri in ["/r/abcdefgh", "/api/v1/domains/example.com"] {
        let (_, _, body, _) = get(&app, uri).await;
        assert!(
            !is_result_router_page(&body),
            "{uri} answered by result router: {body}"
        );
    }

    // Guard against the vacuous pass: the result router must exist at all.
    let (_, _, body, _) = get(&app, "/").await;
    assert!(
        is_result_router_page(&body),
        "result router not mounted at /"
    );
}

#[tokio::test]
async fn result_router_does_not_answer_static_and_well_known_paths() {
    let (app, _id, _dir) = app_with_entry().await;
    let plain = app_without_history();

    for uri in [
        "/favicon.svg",
        "/favicon.ico",
        "/bimi-logo.svg",
        "/.well-known/mta-sts.txt",
    ] {
        let (status, ct, body, _) = get(&app, uri).await;
        let (p_status, p_ct, _, _) = get(&plain, uri).await;
        assert_eq!(
            status, p_status,
            "{uri}: status differs from history-less app"
        );
        assert_eq!(
            ct, p_ct,
            "{uri}: content-type differs from history-less app"
        );
        assert!(!body.contains("id=\"result\""), "{uri}: result page served");
    }

    let (_, _, body, _) = get(&app, "/").await;
    assert!(
        is_result_router_page(&body),
        "result router not mounted at /"
    );
}
