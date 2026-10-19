//! Error pages and the `?d=` redirect through the composed router
//! (v2-pages requirements 14 and 15).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn get_uri(uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    split(res).await
}

async fn split(res: axum::response::Response) -> (StatusCode, axum::http::HeaderMap, String) {
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn assert_frame_page(headers: &axum::http::HeaderMap, body: &str, what: &str) {
    assert_eq!(
        headers[header::CACHE_CONTROL],
        "no-store",
        "{what}: cache-control"
    );
    let ct = headers[header::CONTENT_TYPE].to_str().unwrap();
    assert!(ct.starts_with("text/html"), "{what}: content-type {ct}");
    assert!(body.contains(r#"<html lang="en">"#), "{what}: frame");
    assert!(body.contains("<h1"), "{what}: heading");
}

fn assert_query_input(body: &str, what: &str) {
    assert!(
        body.contains(r#"class="query-input""#),
        "{what}: query-input class"
    );
    assert!(body.contains(r#"action="/""#), "{what}: form action");
    assert!(body.contains(r#"name="d""#), "{what}: input name d");
}

fn assert_not_found_markers(
    status: StatusCode,
    headers: &axum::http::HeaderMap,
    body: &str,
    what: &str,
) {
    assert_eq!(status, StatusCode::NOT_FOUND, "{what}: status");
    assert_frame_page(headers, body, what);
    assert_query_input(body, what);
    assert!(body.contains(r#"href="/guide/""#), "{what}: guide link");
    assert!(body.contains(r#"href="/compare""#), "{what}: compare link");
}

#[tokio::test]
async fn unmatched_paths_and_bare_root_answer_the_404_page() {
    for uri in ["/nope", "/", "/?x=1"] {
        let (status, headers, body) = get_uri(uri).await;
        assert_not_found_markers(status, &headers, &body, uri);
    }
}

#[tokio::test]
async fn server_error_page_is_500_on_the_frame_with_a_status_link() {
    let (status, headers, body) = split(pages::server_error()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_frame_page(&headers, &body, "server_error");
    assert!(body.contains(r#"href="/status""#), "status link");
}

#[tokio::test]
async fn query_redirect_table() {
    let ok: &[(&str, &str)] = &[
        ("/?d=%20Example.COM.%20", "/example.com"),
        ("/?d=example.com", "/example.com"),
        ("/?d=b%C3%BCcher.example", "/b%C3%BCcher.example"),
        ("/?d=a.b..", "/a.b."),
    ];
    for (uri, location) in ok {
        let (status, headers, _) = get_uri(uri).await;
        assert_eq!(status, StatusCode::MOVED_PERMANENTLY, "{uri}: status");
        assert_eq!(headers[header::LOCATION], *location, "{uri}: location");
    }

    // (uri, refilled value attribute as rendered)
    let bad: &[(&str, &str)] = &[
        ("/?d=foo", r#"value="foo""#),
        ("/?d=a/b.c", r#"value="a/b.c""#),
        ("/?d=a%20b.c", r#"value="a b.c""#),
        ("/?d=a.c%3Fx", r#"value="a.c?x""#),
        ("/?d=a.c%23x", r#"value="a.c#x""#),
        ("/?d=", r#"value="""#),
    ];
    for (uri, value_attr) in bad {
        let (status, headers, body) = get_uri(uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: status");
        assert_eq!(
            headers[header::CACHE_CONTROL],
            "no-store",
            "{uri}: cache-control"
        );
        assert_query_input(&body, uri);
        assert!(
            body.contains(value_attr),
            "{uri}: expected {value_attr} in body"
        );
        assert!(
            body.lines()
                .any(|l| l.trim() == "Enter a domain name, e.g. example.com."),
            "{uri}: hint line"
        );
    }
}
