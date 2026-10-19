//! The privacy page route (V2 spec v2-entries, requirements 16 and 20): its own router, and
//! every router module netray-web has merges into one app without a path conflict.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_store::HistoryStore;
use netray_web::entries::{EntriesDeps, EntryCard, privacy_router, render, router};
use tower::ServiceExt;

async fn get(app: &Router, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn header_of(h: &axum::http::HeaderMap, name: header::HeaderName) -> &str {
    h.get(name).map(|v| v.to_str().unwrap()).unwrap_or("")
}

#[tokio::test]
async fn c1_privacy_router_answers_the_privacy_page_with_cache_headers() {
    let (status, headers, body) = get(&privacy_router(), "/privacy").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header_of(&headers, header::CONTENT_TYPE),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        header_of(&headers, header::CACHE_CONTROL),
        "public, max-age=3600"
    );
    assert_eq!(body, render::privacy().unwrap());
}

#[tokio::test]
async fn c7_all_web_routers_merge_without_a_conflict_and_both_answer() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let entries = router(EntriesDeps {
        history: Arc::new(store),
        v1: None,
        card: Arc::new(|_: &EntryCard| Ok(Vec::new())),
    });

    let app: Router = Router::new().merge(entries).merge(privacy_router());

    let (status, _, body) = get(&app, "/privacy").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, render::privacy().unwrap());

    let (status, _, _) = get(&app, "/r/AAAAAAAA").await;
    assert_eq!(status, StatusCode::GONE);
}
