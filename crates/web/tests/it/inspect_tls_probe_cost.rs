//! v2-tls-probe-close, Phase 2, requirement 8: the inspect page's TLS admission charges `[api.tls]`
//! the planned cost of one address on every port, probe connections included, not the port count.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
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
        unreachable!("no run is started by the TLS routes")
    }
}

fn refused() -> ProbeOutcome {
    ProbeOutcome {
        verdict: Verdict::Refused,
        version: None,
        cipher_suite: None,
        detail: "alert protocol_version".into(),
    }
}

fn app() -> Router {
    let config: ApiConfig = toml::from_str("[tls]\nper_ip_burst = 10\nper_ip_per_minute = 1")
        .expect("[api.tls] parses");
    let probes = ProtocolProbes {
        ssl3: refused(),
        tls1_0: refused(),
        tls1_1: refused(),
        tls1_2: refused(),
        tls1_3: refused(),
        suites_insecure: refused(),
        suites_cbc: refused(),
        suites_static_rsa: refused(),
    };
    let inspector = stub_state(Arc::new(StubTlsBackend::new().with_probes(probes))).inspector();
    let state = ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        tls: Some(inspector),
        ..Default::default()
    });
    inspect_router(InspectDeps::new(
        Arc::new(InspectJobs::new(state)),
        InspectConfig::default(),
    ))
}

async fn run(app: &Router) -> StatusCode {
    app.clone()
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("h=example.com%3A443"))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn a_probing_admission_charges_eight_so_the_second_is_refused() {
    let app = app();
    assert_ne!(
        run(&app).await,
        StatusCode::TOO_MANY_REQUESTS,
        "8 of 10 fit"
    );
    assert_eq!(
        run(&app).await,
        StatusCode::TOO_MANY_REQUESTS,
        "the first admission charged 8 of 10"
    );
}
