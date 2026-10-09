//! `allow_system_resolvers` gates the `@system` completion of `POST /api/parse`.

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

/// (input, allow_system_resolvers, `@system` offered, a non-system server that must still be offered)
const ROWS: &[(&str, bool, bool, &str)] = &[
    ("example.com @sy", false, false, ""),
    ("example.com @sy", true, true, ""),
    ("example.com @c", false, false, "@cloudflare"),
    ("example.com ", false, false, "@cloudflare"),
    ("example.com ", true, true, "@cloudflare"),
];

#[tokio::test]
async fn parse_offers_system_completion_only_when_allowed() {
    for &(input, allow, expect_system, other) in ROWS {
        let router = test_router(state_with(allow));
        let body = serde_json::json!({ "input": input }).to_string();
        let resp = router
            .oneshot(post_json("/api/parse", &body))
            .await
            .unwrap();
        let (status, json) = json_of(resp).await;
        assert_eq!(status, StatusCode::OK, "{input:?} allow={allow}: {json}");
        let labels: Vec<&str> = json["completions"]
            .as_array()
            .unwrap_or_else(|| panic!("no completions array: {json}"))
            .iter()
            .filter_map(|c| c["label"].as_str())
            .collect();
        assert_eq!(
            labels.contains(&"@system"),
            expect_system,
            "{input:?} allow={allow}: labels {labels:?}"
        );
        if !other.is_empty() {
            assert!(
                labels.contains(&other),
                "{input:?} allow={allow}: {other} missing from {labels:?}"
            );
        }
    }
}
