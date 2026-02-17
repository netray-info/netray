//! API route definitions and shared application state.

pub mod meta;
pub mod query;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::config::Config;
use crate::security::IpExtractor;

/// Shared state passed to all API handlers via axum's `State` extractor.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub circuit_breakers: Arc<CircuitBreakerRegistry>,
    pub ip_extractor: Arc<IpExtractor>,
}

/// Build the API router with all endpoints.
///
/// Health is mounted separately so it can bypass rate limiting.
pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/query",
            get(query::get_handler).post(query::post_handler),
        )
        .route("/api/servers", get(meta::servers))
        .route("/api/record-types", get(meta::record_types))
        .with_state(state)
}

/// Health endpoint router. Kept separate so it can be mounted outside
/// rate-limiting layers.
pub fn health_router() -> Router {
    Router::new().route("/api/health", get(meta::health))
}
