//! spec v2-ip-cli, final review: a field with no form in the asked format is refused before any
//! charge or lookup, as the decision log's "validation runs before any charge" says.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, ip_host,
};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn state() -> ApiState {
    ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    })
}

fn request(path: &str) -> Request<Body> {
    let mut req = Request::get(path)
        .header("user-agent", "curl/8.5.0")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "192.0.2.10:40000".parse::<SocketAddr>().unwrap(),
    ));
    req
}

#[tokio::test]
async fn a_scalar_field_as_toml_costs_no_rate_limit_budget() {
    let state = state();
    let app: Router = netray_web::api::router(state.clone());
    let burst = ApiConfig::default().ip.per_ip_burst;
    for path in [
        "/ip/198.51.100.1?field=country&format=toml",
        "/ip?field=city&format=toml",
    ] {
        for n in 0..burst + 1 {
            let resp = app.clone().oneshot(request(path)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{path} request {n}");
        }
    }
    for n in 0..burst + 1 {
        let resp = ip_host(&state, request("/country/toml")).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "ip_host request {n}"
        );
    }
    let resp = app
        .clone()
        .oneshot(request("/ip/198.51.100.1?field=country"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
