//! Pins beacon's `POST /inspect` wire behaviour (v2-api phase 6): the route is unchanged
//! by the move of its work into `EmailInspector`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use netray_email::config::Config;
use netray_email::routes::{ApiDoc, api_router};
use netray_email::state::AppState;
use tower::ServiceExt;
use utoipa::OpenApi;

async fn state() -> AppState {
    let config = Config::load(None).expect("default config");
    AppState::new(&config).await.expect("app state")
}

async fn post(app: &Router, peer: &str, body: &str) -> Response<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri("/inspect")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer: SocketAddr = format!("{peer}:4000").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    app.clone().oneshot(req).await.unwrap()
}

async fn text(resp: Response<Body>) -> String {
    String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}

fn is_sse(resp: &Response<Body>) -> bool {
    resp.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"))
}

#[tokio::test]
async fn inspect_post_status_matrix_is_unchanged() {
    let app = api_router(state().await);

    // The status is decided before any lookup; the body is dropped unread.
    let resp = post(&app, "198.51.100.1", r#"{"domain":"192.0.2.1"}"#).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(is_sse(&resp), "IP literal must answer text/event-stream");
    drop(resp);

    let resp = post(&app, "198.51.100.2", r#"{"domain":"not a domain"}"#).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        text(resp).await,
        r#"{"error":{"code":"INVALID_DOMAIN","message":"invalid domain: domain must have at least two labels"}}"#
    );

    let resp = post(
        &app,
        "198.51.100.3",
        r#"{"domain":"example.com","dkim_selectors":["s1"]}"#,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(is_sse(&resp));
    drop(resp);
}

#[tokio::test]
async fn inspect_post_eleventh_request_from_one_peer_is_rate_limited() {
    let app = api_router(state().await);
    for n in 1..=10 {
        let resp = post(&app, "198.51.100.9", r#"{"domain":"192.0.2.1"}"#).await;
        assert_eq!(resp.status(), StatusCode::OK, "request {n}");
        drop(resp);
    }
    let resp = post(&app, "198.51.100.9", r#"{"domain":"192.0.2.1"}"#).await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp.headers().contains_key(header::RETRY_AFTER));
    let body: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    assert_eq!(body["error"]["code"], "RATE_LIMITED");
}

#[tokio::test]
async fn inspect_post_with_all_permits_held_is_refused() {
    let state = state().await;
    let sem = Arc::clone(&state.inspect_semaphore);
    let _held = sem.acquire_many_owned(16).await.expect("16 permits");
    let app = api_router(state);

    let resp = post(&app, "198.51.100.20", r#"{"domain":"192.0.2.1"}"#).await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let body: serde_json::Value = serde_json::from_str(&text(resp).await).unwrap();
    assert_eq!(body["error"]["code"], "TOO_MANY_INSPECTIONS");
}

#[test]
fn openapi_document_names_no_sse_event_component() {
    let doc = ApiDoc::openapi().to_json().expect("openapi json");
    assert!(
        !doc.contains("SseEvent"),
        "SseEvent leaked into the document"
    );
}
