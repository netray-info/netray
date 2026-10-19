//! Old V1 site URLs answer 301 to their V2 page (spec v2-dkim-legacy-urls, Phase 1,
//! requirements 9 and 10; criteria C1-C10).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use netray_web::pages::{self, guides};
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn get_uri(uri: &str) -> (StatusCode, Option<String>) {
    let res = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let location = res
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_string());
    (res.status(), location)
}

async fn assert_moved(from: &str, to: &str) {
    let (status, location) = get_uri(from).await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY, "{from}");
    assert_eq!(location.as_deref(), Some(to), "{from}");
}

#[tokio::test]
async fn guide_html_suffix_moves_to_the_slug() {
    assert_moved("/guide/asn.html", "/guide/asn").await;
}

#[tokio::test]
async fn every_guide_slug_with_html_suffix_moves() {
    for g in guides::GUIDES {
        assert_moved(
            &format!("/guide/{}.html", g.slug),
            &format!("/guide/{}", g.slug),
        )
        .await;
    }
}

#[tokio::test]
async fn guide_index_html_moves_to_the_index() {
    assert_moved("/guide/index.html", "/guide/").await;
}

#[tokio::test]
async fn guide_lens_moves_to_scoring() {
    assert_moved("/guide/lens.html", "/guide/scoring").await;
    assert_moved("/guide/lens", "/guide/scoring").await;
}

#[tokio::test]
async fn unknown_html_stem_is_404() {
    let (status, location) = get_uri("/guide/nonesuch.html").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(location, None);
}

#[tokio::test]
async fn trailing_slash_pages_move_to_canonical() {
    assert_moved("/compare/", "/compare").await;
    assert_moved("/status/", "/status").await;
}

#[tokio::test]
async fn tools_moves_to_home() {
    assert_moved("/tools", "/").await;
    assert_moved("/tools/", "/").await;
}

#[tokio::test]
async fn guide_slug_still_answers_200() {
    let (status, location) = get_uri("/guide/asn").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(location, None);
}
