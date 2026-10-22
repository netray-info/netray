//! The evidence routes (V2 spec v2-inspectors, requirement 6): five literal routes
//! `/{domain}/{dns,tls,http,email,ip}` and `/r/{id}/{segment}`, composed with `entries::router`.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_store::HistoryStore;
use netray_web::entries::{EntriesDeps, EntryCard, NOT_FOUND_HTML};
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

const PROTOCOLS: [&str; 5] = ["dns", "tls", "http", "email", "ip"];

async fn open() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}

fn evidence(store: Arc<HistoryStore>) -> Router {
    evidence_router(EvidenceDeps { history: store })
}

fn entries(store: Arc<HistoryStore>) -> Router {
    netray_web::entries::router(EntriesDeps {
        history: store,
        card: Arc::new(|_: &EntryCard| Ok(Vec::new())),
    })
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn evidence_router_routes_only_the_five_literal_segments() {
    let (store, _dir) = open().await;
    let app = evidence(store);

    for path in ["/example.com/smtp", "/assets/index-x.js"] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(!body.contains("Nothing here"), "{path} was routed: {body}");
    }
    for p in PROTOCOLS {
        let path = format!("/example.com/{p}");
        let (status, body) = get(&app, &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(body.contains(r#"href="/example.com""#), "{path}: {body}");
    }
}

#[tokio::test]
async fn entries_router_alone_narrowed_to_id_segment_rest() {
    let (store, _dir) = open().await;
    let app = entries(store);

    let (status, body) = get(&app, "/r/AAAAAAAA/tls/x").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, NOT_FOUND_HTML);

    let (status, _) = get(&app, "/r/AAAAAAAA").await;
    assert_eq!(status, StatusCode::GONE);
    let (status, _) = get(&app, "/r/AAAAAAAA/og.png").await;
    assert_ne!(status, StatusCode::NOT_FOUND, "og.png is still routed");
}

#[tokio::test]
async fn entries_and_evidence_routers_merge_without_panicking() {
    let (store, _dir) = open().await;
    let app = Router::new()
        .merge(entries(store.clone()))
        .merge(evidence(store));
    let (status, _) = get(&app, "/r/AAAAAAAA/tls").await;
    assert_eq!(status, StatusCode::GONE);
}
