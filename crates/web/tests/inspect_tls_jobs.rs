//! V2 spec v2-inspectors, requirement 15 (C1, C9): the TLS inspector job takes tlsight's inputs,
//! runs SMTP STARTTLS ports 25 and 587, refuses 143 and 21 before the job, and streams
//! `netray_tls::module::translate` of the job's response. Driven through `inspect_router` over
//! `InspectJobs` and `StubTlsBackend`.
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

const REFUSAL: &str = "STARTTLS for IMAP and FTP is not supported";

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

#[tokio::test]
async fn c1_tls_inputs_run_and_stream_the_module_translation() {
    // (input, ports the backend must have been called on)
    let rows: [(&str, &[u16]); 6] = [
        ("example.com", &[443]),
        ("example.com:8443", &[8443]),
        ("example.com:443,8443", &[443, 8443]),
        ("93.184.215.14", &[443]),
        ("example.com:25", &[25]),
        ("example.com:587", &[587]),
    ];
    for (input, ports) in rows {
        let (app, backend) = app();

        let stream = run(&app, input).await;

        let mut called: Vec<u16> = backend.calls().iter().map(|c| c.1).collect();
        called.sort_unstable();
        called.dedup();
        assert_eq!(called, ports, "{input}: {stream}");
        assert!(
            stream.contains("id=\\\"ev-tls.") || stream.contains("id=\"ev-tls."),
            "{input}: no evidence blocks: {stream}"
        );
        assert!(
            stream.contains("inspect-band"),
            "{input}: no band: {stream}"
        );
        assert!(
            stream.contains("data-code=\\\"tls.") || stream.contains("data-code=\"tls."),
            "{input}: no tls lines in band: {stream}"
        );
        assert!(!stream.contains(REFUSAL), "{input}: {stream}");
    }
}

#[tokio::test]
async fn c9_imap_and_ftp_ports_are_refused_before_the_job() {
    for input in ["mail.example.com:143", "mail.example.com:21"] {
        let (app, backend) = app();

        let stream = run(&app, input).await;

        assert!(stream.contains(REFUSAL), "{input}: {stream}");
        assert!(backend.calls().is_empty(), "{input}: the job ran");
    }
}
