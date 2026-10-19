//! V2 catalogue Phase 3, C8 (spec Requirement 3(f)): lens scores through `netray_engine::score`
//! with profile v3, its check lists carry the v3 names with the catalogue's texts, and a stored
//! 0.24.0 snapshot still renders.
//!
//! The run is the module goldens through `POST /api/check` (as `engine_run.rs`); the HTTP module
//! is wrapped so its twelve v3 checks have known statuses (a weight-5 warn, a failing `csp`).
//! The expected grade is `netray_engine::score` over the events of a second run on the same
//! registry.

use crate::common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, facts_golden, http_module, ip_golden, registry_with, tls_golden,
};
use lens::config::{Config, SiteConfig};
use lens::routes::api_router;
use lens::snapshot::{Snapshot, render_snapshot_html};
use lens::state::AppState;
use netray_engine::profile::Profiles;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, Module, RunContext, RunOptions, SectionOutcome,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tower::ServiceExt;

const HTTP_IDS: [&str; 12] = [
    "https_redirect",
    "hsts",
    "cors",
    "cookie_secure",
    "csp",
    "x_frame_options",
    "x_content_type_options",
    "referrer_policy",
    "permissions_policy",
    "deprecated_headers",
    "info_leakage",
    "caching",
];

/// The golden HTTP module with its checks replaced by the twelve v3 HTTP checks:
/// `https_redirect` (weight 5) warns, `csp` fails, the rest pass.
struct FixedHttp(Box<dyn Module>);

impl Module for FixedHttp {
    fn protocol(&self) -> Protocol {
        self.0.protocol()
    }
    fn checks(&self) -> &'static [CheckId] {
        self.0.checks()
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        self.0.volatile()
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            let SectionOutcome::Measured { evidence, .. } = self.0.run(ctx, facts).await else {
                panic!("the http golden must be measured");
            };
            let checks = HTTP_IDS
                .iter()
                .map(|n| CheckResult {
                    id: CheckId::parse(&format!("http.{n}")).unwrap(),
                    status: match *n {
                        "https_redirect" => Status::Warn,
                        "csp" => Status::Fail,
                        _ => Status::Pass,
                    },
                    findings: vec![],
                    evidence: vec![],
                })
                .collect();
            SectionOutcome::Measured { checks, evidence }
        })
    }
}

fn state() -> AppState {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = false;
    config.snapshots.enabled = false;
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden("prism.sse"),
        Box::new(FixedHttp(http_module(Some("spectra-inspect.json")))),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    AppState::with_registry(config, registry).unwrap()
}

async fn sync_check(state: AppState) -> Value {
    let (routes, _) = api_router().split_for_parts();
    let app = Router::new()
        .merge(routes.with_state(state))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    let req = Request::builder()
        .method("POST")
        .uri("/api/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"domain":"example.com","stream":false}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).expect("sync body is JSON")
}

fn names(body: &Value, section: &str) -> Vec<String> {
    body[section]["checks"]
        .as_array()
        .unwrap_or_else(|| panic!("{section} has a check list: {body}"))
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn check_lists_hold_v3_names_with_catalogue_texts() {
    let body = sync_check(state()).await;

    let http = names(&body, "http");
    for n in HTTP_IDS {
        assert!(http.contains(&n.to_string()), "http lacks {n}: {http:?}");
    }
    assert!(
        !http.contains(&"security_headers".to_string()),
        "V1 bucket name must be gone: {http:?}"
    );
    let email = names(&body, "email");
    for n in ["spf", "dkim", "dmarc"] {
        assert!(email.contains(&n.to_string()), "email lacks {n}: {email:?}");
    }
    assert!(
        !email.contains(&"email_authentication".to_string()),
        "V1 bucket name must be gone: {email:?}"
    );

    let spec = netray_engine::catalogue()
        .get(&CheckId::parse("http.csp").unwrap())
        .expect("catalogue has http.csp");
    let csp = body["http"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "csp")
        .expect("csp check");
    assert_eq!(csp["fix_hint"], spec.fix_hint.as_str(), "csp: {csp}");
    assert_eq!(
        csp["guide_url"],
        format!("https://netray.info{}", spec.guide),
        "csp: {csp}"
    );
}

#[tokio::test]
async fn the_grade_is_the_engine_score_with_a_warn_earning_half() {
    let state = state();
    let body = sync_check(state.clone()).await;

    // The same registry, run directly: the events `netray_engine::score` takes.
    let plan: Vec<(Protocol, Duration)> = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ]
    .iter()
    .map(|p| (*p, Duration::from_secs(15)))
    .collect();
    let base = RunContext {
        deadline: Instant::now() + Duration::from_secs(20),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let (tx, mut rx) = mpsc::channel(8);
    let (_, events) = tokio::join!(
        netray_engine::run(&state.registry, base, &plan, Duration::from_secs(5), tx),
        async {
            let mut events = Vec::new();
            while let Some(e) = rx.recv().await {
                events.push(e);
            }
            events
        }
    );
    let want = netray_engine::score(Profiles::current(), &events);

    let summary = &body["summary"];
    assert_eq!(
        summary["grade"],
        serde_json::to_value(want.grade).unwrap(),
        "summary: {summary}"
    );
    assert_eq!(
        summary["score"].as_f64().unwrap(),
        (want.overall_percentage * 10.0).round() / 10.0,
        "summary: {summary}"
    );
    // The weight-5 warn earns 2.5 of 5, not V1's integer 2.
    let http = &want.sections[&Protocol::Http];
    let weights = &Profiles::current().sections[&Protocol::Http].checks;
    let possible: u32 = weights.values().sum();
    assert_eq!(
        http.possible, possible,
        "control: every http check is weighted"
    );
    // Everything passes but the warning (5, earns 2.5) and csp (1, fails).
    assert_eq!(http.earned, (possible - 5 - 1) as f64 + 2.5);
}

fn old_snapshot(earned: Value) -> String {
    json!({
        "shortid": "ABCD1234",
        "domain": "example.com",
        "grade": "B",
        "score": 80.0,
        "sections": [{
            "name": "Email",
            "grade": "B",
            "passes": 0, "warns": 1, "fails": 0, "skips": 0,
            "findings": [{
                "check_name": "email_authentication",
                "verdict": "warn",
                "message": "DMARC policy is none",
                "earned": earned,
                "possible": 5
            }]
        }],
        "server_addresses": [],
        "created_at": "2026-10-01T12:00:00Z",
        "lens_version": "0.24.0"
    })
    .to_string()
}

#[test]
fn a_0_24_snapshot_renders_its_v1_label_and_guide_link() {
    let snap: Snapshot = serde_json::from_str(&old_snapshot(json!(2)))
        .expect("an integer `earned` still deserialises");
    let html = render_snapshot_html(&snap, &SiteConfig::default());
    assert!(
        html.contains("Authentication"),
        "V1 label for email_authentication must render"
    );
    assert!(
        html.contains("https://netray.info/guide/email-auth"),
        "V1 guide link must render"
    );
}

#[test]
fn a_decimal_earned_round_trips() {
    let snap: Snapshot =
        serde_json::from_str(&old_snapshot(json!(2.5))).expect("a decimal `earned` deserialises");
    let back = serde_json::to_value(&snap).unwrap();
    assert_eq!(back["sections"][0]["findings"][0]["earned"], json!(2.5));
    let html = render_snapshot_html(&snap, &SiteConfig::default());
    assert!(html.contains("example.com"));
}

#[test]
fn a_snapshot_with_a_v3_name_renders_the_catalogue_title() {
    let raw = old_snapshot(json!(2.5)).replace("email_authentication", "csp");
    let snap: Snapshot = serde_json::from_str(&raw).expect("snapshot deserialises");
    let spec = netray_engine::catalogue()
        .get(&CheckId::parse("http.csp").unwrap())
        .unwrap();
    let html = render_snapshot_html(&snap, &SiteConfig::default());
    assert!(
        html.contains(spec.title.as_str()),
        "the catalogue title {:?} must render",
        spec.title
    );
}
