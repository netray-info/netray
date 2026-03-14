//! Results endpoint: retrieve cached query results by key.
//!
//! - `GET /api/results/:key` — return a previously cached result as JSON.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::api::AppState;
use crate::error::ErrorResponse;
use crate::result_cache::CachedResult;

/// Retrieve a cached query result by its permalink key.
///
/// Results are stored for up to 1 hour after the originating query completes.
/// The key is the 12-character hex prefix of the UUID v7 assigned at query time
/// and is returned in the `cache_key` field of the `done` SSE event.
#[utoipa::path(
    get,
    path = "/api/results/{key}",
    tag = "Query",
    params(
        ("key" = String, Path, description = "12-character hex permalink key from the `cache_key` field of a `done` SSE event"),
    ),
    responses(
        (status = 200, description = "Cached result found", body = CachedResult),
        (status = 400, description = "Key format invalid (must be 12 hex characters)", body = ErrorResponse),
        (status = 404, description = "No cached result found for this key (expired or never existed)", body = ErrorResponse),
    ),
)]
pub async fn get_handler(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<CachedResult>, impl IntoResponse> {
    // Validate key format: must be 12 hex characters.
    if key.len() != 12 || !key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    match state.result_cache.get(&key).await {
        Some(result) => Ok(Json(result)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

