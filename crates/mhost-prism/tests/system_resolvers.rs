//! `allow_system_resolvers`: reported by `/api/config`, enforced on `@system` queries.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use prism::api::{AppState, QUERY_SEMAPHORE_PERMITS, api_router, health_router};
use prism::circuit_breaker::CircuitBreakerRegistry;
use prism::config::Config;
use prism::reload::HotState;
use prism::result_cache::ResultCache;
use prism::security::IpExtractor;

fn state_with(allow_system_resolvers: bool) -> AppState {
    let mut config = Config::load(None).expect("default config must be valid");
    config.dns.allow_system_resolvers = allow_system_resolvers;
    let hot_state = HotState::new(&config);
    AppState {
        circuit_breakers: Arc::new(CircuitBreakerRegistry::new(&config.circuit_breaker)),
        ip_extractor: Arc::new(IpExtractor::new(&config.server.trusted_proxies)),
        result_cache: Arc::new(ResultCache::new()),
        hot_state,
        ip_enrichment: None,
        query_semaphore: Arc::new(tokio::sync::Semaphore::new(QUERY_SEMAPHORE_PERMITS)),
        config: Arc::new(config),
    }
}

fn test_router(state: AppState) -> axum::Router {
    health_router(state.clone())
        .merge(api_router(state))
        .layer(axum::middleware::from_fn(prism::request_id_middleware))
}

fn peer() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .extension(ConnectInfo::<SocketAddr>(peer()))
        .body(Body::empty())
        .unwrap()
}

fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .extension(ConnectInfo::<SocketAddr>(peer()))
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn json_of(resp: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let json = serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid JSON ({e}): {text}"));
    (status, json)
}

#[tokio::test]
async fn config_reports_allow_system_resolvers() {
    for allow in [false, true] {
        let router = test_router(state_with(allow));
        let resp = router.oneshot(get("/api/config")).await.unwrap();
        let (status, json) = json_of(resp).await;
        assert_eq!(status, StatusCode::OK, "body: {json}");
        assert_eq!(
            json["allow_system_resolvers"],
            serde_json::Value::Bool(allow),
            "body: {json}"
        );
    }
}

#[tokio::test]
async fn system_server_is_refused_when_system_resolvers_disabled() {
    // (label, request) pairs; the policy refuses before any resolver is built.
    let requests = [
        ("GET", get("/api/query?q=example.com%20A%20@system")),
        (
            "POST",
            post_json(
                "/api/query",
                r#"{"domain":"example.com","record_types":["A"],"servers":["system"]}"#,
            ),
        ),
    ];
    for (label, req) in requests {
        let router = test_router(state_with(false));
        let resp = router.oneshot(req).await.unwrap();
        let (status, json) = json_of(resp).await;
        assert!(
            status.is_client_error(),
            "{label}: status {status}, body: {json}"
        );
        assert_eq!(
            json["error"]["code"], "SYSTEM_RESOLVERS_DISABLED",
            "{label}: body: {json}"
        );
    }
}
