use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use netray_store::HistoryStore;
use serde_json::json;

use super::{render, view::entry_view};

/// The body of every 404 under `/r/`, byte-identical to the V1 snapshot routes' body.
pub const NOT_FOUND_HTML: &str = r#"<!DOCTYPE html><html><head><title>Not found</title></head><body><h1>Snapshot not found</h1><p>This snapshot has expired or the ID is unknown.</p></body></html>"#;

const HTML: &str = "text/html; charset=utf-8";
const CACHE: &str = "public, max-age=3600";

/// V1 snapshot lookup handed in by lens: `None` when no live snapshot has the ID, `Some` with
/// lens's complete V1 answer otherwise (the page, or its 500 for a failed lookup).
pub type V1Lookup =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Option<Response>> + Send>> + Send + Sync>;

/// What an entry's OG card shows.
pub struct EntryCard {
    pub domain: String,
    pub grade: String,
    pub created_at: SystemTime,
}

/// Entry card renderer handed in by lens: the PNG bytes, or lens's render-failure response.
pub type CardRenderer = Arc<dyn Fn(&EntryCard) -> Result<Vec<u8>, Box<Response>> + Send + Sync>;

#[derive(Clone)]
pub struct EntriesDeps {
    pub history: Arc<HistoryStore>,
    pub v1: Option<V1Lookup>,
    pub card: CardRenderer,
}

/// Serves `/r/{id}` and answers every other path under `/r/` with the 404 page.
pub fn router(deps: EntriesDeps) -> axum::Router {
    axum::Router::<EntriesDeps>::new()
        .route("/r/{id}", get(entry_page))
        .route("/r/{id}/og.png", get(unmatched))
        .route("/r/", get(unmatched))
        .route("/r/{id}/", get(unmatched))
        .route("/r/{id}/{*rest}", get(unmatched))
        .with_state(deps)
}

async fn unmatched() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(CONTENT_TYPE, HTML)],
        NOT_FOUND_HTML,
    )
        .into_response()
}

fn well_formed(id: &str) -> bool {
    id.len() == 8 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn page(status: StatusCode, html: askama::Result<String>) -> Response {
    match html {
        Ok(body) => (status, [(CONTENT_TYPE, HTML), (CACHE_CONTROL, CACHE)], body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "entry page render failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn entry_page(
    State(deps): State<EntriesDeps>,
    id: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(id)) = id else {
        return unmatched().await;
    };
    if !well_formed(&id) {
        return unmatched().await;
    }
    match deps.history.get(&id).await {
        Err(e) => {
            tracing::warn!(id = %id, error = %e, "history lookup failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"code": "INTERNAL", "message": "entry unavailable"}})),
            )
                .into_response()
        }
        Ok(Some(entry)) => page(StatusCode::OK, render::entry(&entry_view(&entry))),
        Ok(None) => {
            if let Some(v1) = &deps.v1
                && let Some(resp) = v1(id).await
            {
                return resp;
            }
            page(StatusCode::GONE, render::gone())
        }
    }
}
