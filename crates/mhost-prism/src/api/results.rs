//! Results endpoint: retrieve cached query results by key.
//!
//! - `GET /api/results/:key` — return a previously cached result as JSON.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::api::AppState;
use crate::result_cache::CachedResult;

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
