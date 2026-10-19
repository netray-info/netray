//! spec v2-ip-cli, Phase 2 review: a 429 from the per-target limiter also carries
//! `X-RateLimit-Remaining: 0` (requirement 10), not the caller's per-IP budget.
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

#[tokio::test]
async fn a_per_target_429_carries_remaining_zero() {
    let mut config = ApiConfig::default();
    config.ip.per_target_burst = 1;
    let state = ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    });
    let app: Router = netray_web::api::router(state);
    let mut last = None;
    for _ in 0..2 {
        let mut req = Request::get("/ip/198.51.100.1")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(
            "192.0.2.10:40000".parse::<SocketAddr>().unwrap(),
        ));
        last = Some(app.clone().oneshot(req).await.unwrap());
    }
    let resp = last.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        resp.headers()
            .get("x-ratelimit-remaining")
            .map(|v| v.to_str().unwrap().to_string()),
        Some("0".to_string())
    );
}
