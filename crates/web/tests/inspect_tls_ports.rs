//! V2 spec v2-tls-backend, Phase 2, R8 and R9 (C13, C14): the TLS inspector job streams one part
//! per inspected port (label `handshake` for one port, `port <n>` for more) and deduplicates the
//! ports in first-occurrence order before charging and inspecting.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the TLS inspector")
    }
}

fn app() -> (Router, Arc<StubTlsBackend>) {
    // The stub resolves every name to a global address; documentation ranges are blocked targets.
    let backend = Arc::new(StubTlsBackend::new());
    let inspector = stub_state(backend.clone()).inspector();
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        tls: Some(inspector),
        ..Default::default()
    });
    let deps = InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default());
    (inspect_router(deps), backend)
}

/// The whole event stream as text.
async fn run(app: &Router, input: &str) -> String {
    let body = format!("h={}", input.replace(':', "%3A").replace(',', "%2C"));
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

fn count(stream: &str, needle: &str) -> usize {
    stream.matches(needle).count()
}

#[tokio::test]
async fn c13_one_part_per_port_labelled_by_port() {
    let (app, _backend) = app();
    let stream = run(&app, "example.com:443,465").await;

    assert!(stream.contains("port 443"), "{stream}");
    assert!(stream.contains("port 465"), "{stream}");
    assert!(!stream.contains("handshake"), "{stream}");
    assert_eq!(count(&stream, "inspect-part-2"), 0, "{stream}");
    assert!(stream.contains("inspect-band"), "{stream}");

    // The streamed part is the last mention of its id; its blocks follow it, before the next part.
    let p0 = stream.rfind("inspect-part-0").expect(&stream);
    let p1 = stream.rfind("inspect-part-1").expect(&stream);
    let b443 = stream
        .find("ev-tls.chain@93.184.215.14:443")
        .unwrap_or_else(|| panic!("no 443 chain block: {stream}"));
    let b465 = stream
        .find("ev-tls.chain@93.184.215.14:465")
        .unwrap_or_else(|| panic!("no 465 chain block: {stream}"));
    assert!(p0 < p1, "parts out of order: {stream}");
    assert!(p0 < b443 && b443 < p1, "443 block outside part 0: {stream}");
    assert!(b465 > p1, "465 block outside part 1: {stream}");
}

#[tokio::test]
async fn c13_a_single_port_keeps_the_handshake_label() {
    let (app, _backend) = app();
    let stream = run(&app, "example.com").await;

    assert!(stream.contains("handshake"), "{stream}");
    assert!(!stream.contains("port 443"), "{stream}");
    assert!(stream.contains("inspect-part-0"), "{stream}");
    assert_eq!(count(&stream, "inspect-part-1"), 0, "{stream}");
    assert!(stream.contains("inspect-band"), "{stream}");
}

#[tokio::test]
async fn c14_repeated_ports_are_inspected_once_in_first_occurrence_order() {
    let (app, backend) = app();
    let stream = run(&app, "example.com:443,443,465").await;

    let calls = backend.calls();
    assert_eq!(calls.len(), 2, "{calls:?}: {stream}");
    assert_eq!(calls[0].1, 443, "{calls:?}");
    assert_eq!(calls[1].1, 465, "{calls:?}");
    assert_eq!(calls[0].0, calls[1].0, "one address: {calls:?}");
    assert!(stream.contains("inspect-part-1"), "{stream}");
    assert_eq!(count(&stream, "inspect-part-2"), 0, "{stream}");
}
