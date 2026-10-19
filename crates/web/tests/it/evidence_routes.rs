//! The evidence routes (V2 spec v2-inspectors, requirement 6): five literal routes
//! `/{domain}/{dns,tls,http,email,ip}` and `/r/{id}/{segment}`, composed with `entries::router`.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use netray_store::HistoryStore;
use netray_web::entries::{EntriesDeps, EntryCard, NOT_FOUND_HTML, V1Lookup};
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

fn v1_some() -> V1Lookup {
    Arc::new(|_id: String| {
        Box::pin(async {
            let resp: Response = (StatusCode::OK, "V1-PAGE-BODY").into_response();
            Some(resp)
        })
    })
}

fn evidence(store: Arc<HistoryStore>, v1: Option<V1Lookup>) -> Router {
    evidence_router(EvidenceDeps { history: store, v1 })
}

fn entries(store: Arc<HistoryStore>, v1: Option<V1Lookup>) -> Router {
    netray_web::entries::router(EntriesDeps {
        history: store,
        v1,
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
async fn live_v1_snapshot_id_answers_404_not_its_v1_page() {
    let (store, _dir) = open().await;
    let (status, body) = get(&evidence(store, Some(v1_some())), "/r/AAAAAAAA/tls").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("V1-PAGE-BODY"), "{body}");
}

#[tokio::test]
async fn evidence_router_routes_only_the_five_literal_segments() {
    let (store, _dir) = open().await;
    let app = evidence(store, None);

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
    let app = entries(store, None);

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
        .merge(entries(store.clone(), None))
        .merge(evidence(store, None));
    let (status, _) = get(&app, "/r/AAAAAAAA/tls").await;
    assert_eq!(status, StatusCode::GONE);
}
