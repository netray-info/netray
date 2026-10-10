//! V2 Phase 2, requirement 8 (criterion C6): lens takes the email section from the engine
//! registry, and the DKIM selectors on a check request reach the module. With a stub email
//! module in the registry that records `ctx.options.dkim_selectors`, `POST /api/check` with
//! `dkim_selectors` runs it in-process with them; without, the module sees `None`. The other
//! backends point at a closed port and there is no `[backends.email] url`, so the section
//! cannot have come from a request.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use lens::config::Config;
use lens::routes::api_router;
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;
use tower::ServiceExt;

static CHECKS: LazyLock<Vec<CheckId>> =
    LazyLock::new(|| vec![CheckId::parse("email.email_authentication").unwrap()]);

/// An email module that records the DKIM selectors of every run and answers one passing check.
struct RecordingEmail {
    seen: Arc<Mutex<Vec<Option<Vec<String>>>>>,
}

impl Module for RecordingEmail {
    fn protocol(&self) -> Protocol {
        Protocol::Email
    }
    fn checks(&self) -> &'static [CheckId] {
        &CHECKS
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.seen
            .lock()
            .unwrap()
            .push(ctx.options.dkim_selectors.clone());
        Box::pin(async {
            SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse("email.email_authentication").unwrap(),
                    status: Status::Pass,
                    findings: vec![],
                    evidence: vec![],
                }],
                presentation: json!({}),
            }
        })
    }
}

/// A router over a registry with the recording email module; returns what it saw.
fn app() -> (Router, Arc<Mutex<Vec<Option<Vec<String>>>>>) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    // Refused connections at once: no backend answers, so only the module can supply email.
    let closed = || Some("http://127.0.0.1:1".to_string());
    config.backends.dns.url = closed();
    config.backends.tls.url = closed();
    config.backends.ip.url = closed();
    config.cache.enabled = false;
    config.snapshots.enabled = false;

    let seen = Arc::new(Mutex::new(Vec::new()));
    let registry = Registry::new().with(Box::new(RecordingEmail { seen: seen.clone() }));
    let state = AppState::with_registry(config, registry).unwrap();
    let (routes, _) = api_router().split_for_parts();
    let app = Router::new()
        .merge(routes.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    (app, seen)
}

/// POST a sync check for example.com with the given request body; the email section's check
/// names from the response.
async fn check(app: &Router, body: &str) -> Vec<String> {
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = tokio::time::timeout(Duration::from_secs(10), app.clone().oneshot(req))
        .await
        .expect("check finished")
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("sync body is JSON");
    v["sections"]["email"]["checks"]
        .as_array()
        .unwrap_or_else(|| panic!("no email section in {v}"))
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn dkim_selectors_on_a_lens_request_reach_the_email_module() {
    let (app, seen) = app();

    let names = check(
        &app,
        r#"{"domain":"example.com","stream":false,"dkim_selectors":"google,selector1"}"#,
    )
    .await;
    assert_eq!(
        names,
        vec!["email_authentication"],
        "the email section's checks are the module's"
    );
    assert_eq!(
        *seen.lock().unwrap(),
        vec![Some(vec!["google".to_string(), "selector1".to_string()])],
        "the module saw the request's selectors"
    );

    check(&app, r#"{"domain":"example.com","stream":false}"#).await;
    assert_eq!(
        seen.lock().unwrap().last(),
        Some(&None),
        "a request without selectors leaves the choice to the module"
    );
    assert_eq!(seen.lock().unwrap().len(), 2, "one module run per request");
}
