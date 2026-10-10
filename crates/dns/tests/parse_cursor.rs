//! `POST /api/parse` answers every well-formed `cursor_pos`; it is a byte offset and a mid-character one rounds down.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use netray_dns::api::{AppState, QUERY_SEMAPHORE_PERMITS, api_router, health_router};
use netray_dns::circuit_breaker::CircuitBreakerRegistry;
use netray_dns::config::Config;
use netray_dns::reload::HotState;
use netray_dns::result_cache::ResultCache;
use netray_dns::security::IpExtractor;

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
        .layer(axum::middleware::from_fn(netray_dns::request_id_middleware))
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

async fn completion_labels(input: &str, cursor_pos: usize) -> Vec<String> {
    let router = test_router(state_with(true));
    let body = serde_json::json!({ "input": input, "cursor_pos": cursor_pos }).to_string();
    let resp = router
        .oneshot(post_json("/api/parse", &body))
        .await
        .unwrap();
    let (status, json) = json_of(resp).await;
    assert_eq!(status, StatusCode::OK, "{input:?} @{cursor_pos}: {json}");
    json["completions"]
        .as_array()
        .unwrap_or_else(|| panic!("no completions array: {json}"))
        .iter()
        .filter_map(|c| c["label"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn cursor_inside_multibyte_char_on_domain_token_yields_no_completions() {
    // "exämple.com @sy": ä is bytes 2-3, cursor 3 is mid-character.
    let labels = completion_labels("exämple.com @sy", 3).await;
    assert!(labels.is_empty(), "expected no completions, got {labels:?}");
}

#[tokio::test]
async fn cursor_inside_multibyte_char_rounds_down_to_prefix_completions() {
    // "example.com @sÿ": ÿ is bytes 14-15, cursor 15 is mid-character -> prefix "@s".
    let labels = completion_labels("example.com @sÿ", 15).await;
    assert!(labels.contains(&"@system".to_owned()), "{labels:?}");
}

#[tokio::test]
async fn cursor_at_end_of_ascii_input_completes_as_before() {
    let labels = completion_labels("example.com @sy", 15).await;
    assert!(labels.contains(&"@system".to_owned()), "{labels:?}");
}

#[tokio::test]
async fn cursor_beyond_input_is_clamped() {
    // Clamped to the end: the same completions as a cursor at byte 15.
    let beyond = completion_labels("example.com @sy", 999).await;
    let at_end = completion_labels("example.com @sy", 15).await;
    assert_eq!(beyond, at_end);
}
