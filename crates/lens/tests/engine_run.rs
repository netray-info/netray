//! V2 Phase 6, requirements 18 and 20: lens's check path runs through `netray_engine::run` with
//! one resolve stage. A request calls the registry's `FactsProvider` once; a failed resolve
//! leaves the sections that need addresses (HTTP, TLS, IP) errored and the result `incomplete`
//! while DNS and email keep their results; the IP section samples the provider's A/AAAA, not
//! the DNS section's presentation.

mod common;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    FailingFacts, dns_golden_raw, email_golden, facts_golden, facts_with_ips, http_module,
    ip_golden, registry_with, tls_golden,
};
use lens::check::{SectionError, run_check};
use lens::config::Config;
use lens::modules::BackendExtra;
use lens::routes::api_router;
use lens::state::AppState;
use netray_engine::{BoxFuture, Domain, Facts, FactsProvider, ResolveError, RunContext};
use serde_json::Value;
use tower::ServiceExt;

/// A resolve stage that counts its calls and delegates.
struct Counting {
    inner: Box<dyn FactsProvider>,
    calls: Arc<AtomicUsize>,
}

impl FactsProvider for Counting {
    fn resolve<'a>(
        &'a self,
        ctx: &'a RunContext,
        domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.resolve(ctx, domain)
    }
}

fn state_with(facts: Box<dyn FactsProvider>) -> AppState {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    config.cache.enabled = false;
    config.snapshots.enabled = false;
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts,
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    AppState::with_registry(config, registry).unwrap()
}

/// `POST /api/check` (sync) for example.com; the JSON body.
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

#[tokio::test]
async fn one_check_request_calls_the_facts_provider_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let state = state_with(Box::new(Counting {
        inner: facts_golden("prism.sse"),
        calls: calls.clone(),
    }));

    let body = sync_check(state).await;

    assert_eq!(body["summary"]["complete"], true, "control: {body}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the resolve stage runs once per check request"
    );
}

#[tokio::test]
async fn a_failed_resolve_errors_the_address_sections_and_keeps_dns_and_email() {
    let state = state_with(Box::new(FailingFacts));

    let body = sync_check(state).await;

    let summary = &body["summary"];
    assert_eq!(summary["grade"], "incomplete", "summary: {summary}");
    assert_eq!(summary["complete"], false, "summary: {summary}");
    for section in ["http", "tls", "ip"] {
        assert_eq!(
            summary["sections"][section], "error",
            "{section} needs addresses and the resolve failed: {summary}"
        );
    }
    for section in ["dns", "email"] {
        assert_ne!(
            summary["sections"][section], "error",
            "{section} needs no addresses: {summary}"
        );
        assert!(
            body[section]["checks"]
                .as_array()
                .is_some_and(|c| !c.is_empty()),
            "{section} keeps its checks: {body}"
        );
    }
}

#[tokio::test]
async fn the_ip_section_samples_the_facts_not_the_dns_presentation() {
    // The DNS golden presents 192.0.2.10; the resolve stage answers other public addresses.
    let state = state_with(facts_with_ips(&["8.8.8.8", "8.8.4.4", "2606:4700::2"]));

    let out = run_check(&state, "example.com").await;

    let ip = out.sections["ip"]
        .as_ref()
        .unwrap_or_else(|e: &SectionError| panic!("ip section failed: {e:?}"));
    let BackendExtra::Ip { addresses, .. } = &ip.extra else {
        panic!("the IP section must carry the IP extras");
    };
    let mut got: Vec<IpAddr> = addresses.iter().map(|a| a.ip).collect();
    got.sort();
    let mut want: Vec<IpAddr> = ["8.8.8.8", "8.8.4.4", "2606:4700::2"]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
    want.sort();
    assert_eq!(
        got, want,
        "the IP section's addresses are the facts' A/AAAA"
    );

    let Ok(dns) = &out.sections["dns"] else {
        panic!("dns section failed");
    };
    let BackendExtra::Dns { resolved_ips, .. } = &dns.extra else {
        panic!("the DNS section must carry the DNS extras");
    };
    assert!(
        resolved_ips.iter().all(|ip| !got.contains(ip)),
        "control: the DNS presentation {resolved_ips:?} differs from the facts {got:?}"
    );
}
