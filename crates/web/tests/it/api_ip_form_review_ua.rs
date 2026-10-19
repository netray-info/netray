//! spec v2-ip-cli, second final reading: `user_agent` of a caller who sends no User-Agent has no
//! TOML form and is refused before any charge or lookup, as the seven scalar fields are.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
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
    let mut req = Request::get(path).body(Body::empty()).unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "192.0.2.10:40000".parse::<SocketAddr>().unwrap(),
    ));
    req
}

#[tokio::test]
async fn user_agent_as_toml_without_an_agent_costs_no_rate_limit_budget() {
    let app: Router = netray_web::api::router(state());
    let config = ApiConfig::default().ip;
    for n in 0..config.per_ip_burst.max(config.per_target_burst) + 1 {
        let resp = app
            .clone()
            .oneshot(request("/ip?field=user_agent&format=toml"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "request {n}");
    }
    let resp = app
        .clone()
        .oneshot(request("/ip?field=country"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
