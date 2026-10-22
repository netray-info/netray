//! spec v2-input-limits, Phase 1 (C11): the latest-evidence route refuses a dotted address like
//! any invalid domain: the bare 404, never the no-entry page that links a domain page.
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_store::HistoryStore;
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

#[tokio::test]
async fn c11_a_dotted_address_answers_404() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let app = evidence_router(EvidenceDeps {
        history: Arc::new(store),
        v1: None,
    });
    for (uri, page) in [("/192.0.2.1./dns", "/192.0.2.1"), ("/1.2.3/dns", "/1.2.3")] {
        let res = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{uri}");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&bytes);
        assert!(
            !body.contains(&format!(r#"href="{page}""#)),
            "{uri}: links a domain page for an address: {body}"
        );
    }
}
