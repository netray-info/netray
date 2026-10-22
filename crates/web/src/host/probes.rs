//! `/health` and `/ready`, with no state: what answers before any host is looked at. Copied from
//! lens's `routes.rs` (spec v2-cutover, Phase 6).

use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct ReadyResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    down: Vec<String>,
}

async fn health_handler() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}

async fn ready_handler() -> impl IntoResponse {
    Json(ReadyResponse {
        status: "ok",
        down: vec![],
    })
}

/// The probe routes; [`super::HostRouters::probes`] takes them.
pub fn probes() -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
}
