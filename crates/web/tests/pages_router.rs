//! `pages::router()` holds the shared pages only (V2 spec v2-pages, requirement 8): no `/`, no
//! `/robots.txt`, no fallback, so a service composes its own around it.
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use tower::ServiceExt;

async fn call(app: Router, path: &str) -> (StatusCode, String) {
    let res = app
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn router_alone_holds_no_root_robots_or_fallback() {
    for path in ["/", "/robots.txt"] {
        let (status, body) = call(pages::router(), path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "GET {path}");
        assert!(
            !body.contains("<html"),
            "GET {path} answered a page: {body}"
        );
    }
}

#[tokio::test]
async fn router_merges_into_a_service_with_its_own_robots_and_fallback() {
    let app = || {
        Router::new()
            .route("/", get(pages::query_redirect))
            .route("/robots.txt", get(|| async { "own-robots" }))
            .merge(pages::router())
            .fallback(pages::not_found)
    };
    let (status, body) = call(app(), "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "own-robots");

    let (status, body) = call(app(), "/").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "`/` without `d` is the 404 page"
    );
    assert!(body.contains("<html"), "the 404 page is a page: {body}");

    let (status, body) = call(app(), "/no/such/path").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body.contains("<html"),
        "the fallback is the 404 page: {body}"
    );
}
