//! spec v2-input-edges, Phase 1 (C14): the latest-evidence route refuses a full-width digit
//! address form like any invalid domain: the bare 404, never the no-entry page.
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_store::HistoryStore;
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

#[tokio::test]
async fn c14_a_full_width_digit_address_answers_404() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let app = evidence_router(EvidenceDeps {
        history: Arc::new(store),
        v1: None,
    });
    // U+FF11 four times, dots ASCII, UTF-8 percent-encoded.
    let uri = "/%EF%BC%91.%EF%BC%91.%EF%BC%91.%EF%BC%91/dns";
    let res = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND, "{uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(
        !body.contains("No entry is stored for this domain."),
        "{uri}: answers the no-entry page for an address form: {body}"
    );
}
