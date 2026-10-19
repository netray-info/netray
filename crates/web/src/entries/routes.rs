use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use netray_store::HistoryStore;
use serde_json::json;

use crate::plate::{card_layout, card_plate};

use super::{
    render,
    view::{entry_view, grade_letter},
};

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
    /// The `plate::card_plate` of the entry's run over its predecessor's.
    pub plate: String,
}

/// Entry card renderer handed in by lens: the PNG bytes, or lens's render-failure response.
pub type CardRenderer = Arc<dyn Fn(&EntryCard) -> Result<Vec<u8>, Box<Response>> + Send + Sync>;

#[derive(Clone)]
pub struct EntriesDeps {
    pub history: Arc<HistoryStore>,
    pub v1: Option<V1Lookup>,
    pub card: CardRenderer,
}

/// Serves `/r/{id}` and `/r/{id}/og.png`; `/r/`, `/r/{id}/`, `/r/{id}/{segment}/` and every path
/// of three or more segments under `/r/` answer the 404 page. The two-segment
/// `/r/{id}/{inspector}` is left to the evidence router.
pub fn router(deps: EntriesDeps) -> axum::Router {
    axum::Router::<EntriesDeps>::new()
        .route("/r/{id}", get(entry_page))
        .route("/r/{id}/og.png", get(entry_card))
        .route("/r/", get(unmatched))
        .route("/r/{id}/", get(unmatched))
        .route("/r/{id}/{segment}/", get(unmatched))
        .route("/r/{id}/{segment}/{*rest}", get(unmatched))
        .with_state(deps)
}

/// Serves `/privacy` (SDD §4.7); lens mounts it next to [`router`] until V5.1.
pub fn privacy_router() -> axum::Router {
    axum::Router::new().route("/privacy", get(privacy_page))
}

async fn privacy_page() -> Response {
    page(StatusCode::OK, render::privacy())
}

async fn unmatched() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(CONTENT_TYPE, HTML)],
        NOT_FOUND_HTML,
    )
        .into_response()
}

pub(crate) fn well_formed(id: &str) -> bool {
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
        Ok(Some(entry)) => {
            metrics::counter!("netray_entry_views_total").increment(1);
            page(StatusCode::OK, render::entry(&entry_view(&entry)))
        }
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

async fn entry_card(
    State(deps): State<EntriesDeps>,
    id: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
) -> Response {
    let Ok(Path(id)) = id else {
        return unmatched().await;
    };
    if !well_formed(&id) {
        return unmatched().await;
    }
    let entry = match deps.history.get(&id).await {
        Err(e) => {
            tracing::warn!(id = %id, error = %e, "history lookup failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"code": "INTERNAL", "message": "entry unavailable"}})),
            )
                .into_response();
        }
        Ok(None) => return StatusCode::GONE.into_response(),
        Ok(Some(entry)) => entry,
    };
    let letter = grade_letter(entry.grade);
    let etag = format!("\"{id}-{letter}\"");
    if headers
        .get(IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
    {
        return (StatusCode::NOT_MODIFIED, [(ETAG, etag)]).into_response();
    }
    let predecessor = match &entry.predecessor_id {
        None => None,
        Some(pid) => match deps.history.get(pid).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(id = %id, error = %e, "history lookup failed");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": {"code": "INTERNAL", "message": "entry unavailable"}})),
                )
                    .into_response();
            }
        },
    };
    let card = EntryCard {
        domain: entry.domain,
        grade: letter.to_string(),
        created_at: UNIX_EPOCH + Duration::from_secs(entry.created_at.max(0) as u64),
        plate: card_plate(&card_layout(
            &entry.run,
            predecessor.as_ref().map(|p| &p.run),
        )),
    };
    match (deps.card)(&card) {
        Ok(png) => (
            StatusCode::OK,
            [
                (CONTENT_TYPE.as_str(), "image/png"),
                (CACHE_CONTROL.as_str(), CACHE),
                (ETAG.as_str(), etag.as_str()),
            ],
            png,
        )
            .into_response(),
        Err(resp) => *resp,
    }
}
