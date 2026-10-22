//! Guide og:image files (v2-pages requirement 20): the six PNGs shipped with the
//! web crate, each guide's V1 choice, and the static route serving them.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages::{self, guides};
use tower::ServiceExt;

const SIX: [&str; 6] = [
    "dns.png",
    "email.png",
    "http.png",
    "ip.png",
    "landing.png",
    "tls.png",
];

fn web_og() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("static/og")
}

fn site() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1-site")
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    v.sort();
    v
}

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .merge(pages::router())
        .fallback(pages::not_found)
}

#[test]
fn static_og_holds_the_six_pngs_identical_to_the_site_copies() {
    assert_eq!(names(&web_og()), SIX, "crates/web/static/og/ listing");
    for name in SIX {
        assert_eq!(
            std::fs::read(web_og().join(name)).unwrap(),
            std::fs::read(site().join("og").join(name)).unwrap(),
            "{name}: bytes differ from site/og"
        );
    }
    let mut seven: Vec<&str> = SIX.to_vec();
    seven.push("lens.png");
    seven.sort();
    assert_eq!(names(&site().join("og")), seven, "site/og/ listing");
}

#[test]
fn each_guide_keeps_its_v1_og_image_with_lens_mapped_to_landing() {
    assert_eq!(guides::GUIDES.len(), 32);
    for g in guides::GUIDES {
        let v1 = if g.slug == "scoring" { "lens" } else { g.slug };
        let path = site().join("guide").join(format!("{v1}.html"));
        let html = std::fs::read_to_string(&path).unwrap();
        let marker = r#"<meta property="og:image" content=""#;
        let start = html
            .find(marker)
            .unwrap_or_else(|| panic!("{}: no og:image", path.display()))
            + marker.len();
        let url = &html[start..start + html[start..].find('"').unwrap()];
        let file = url.rsplit_once("/og/").expect("og path").1;
        let expected = if file == "lens.png" {
            "landing.png"
        } else {
            file
        };
        assert_eq!(g.og_image, expected, "{}: og_image", g.slug);
    }
}

#[tokio::test]
async fn static_og_route_serves_the_six_pngs_and_404s_lens() {
    for name in SIX {
        let res = app()
            .oneshot(
                Request::builder()
                    .uri(format!("/static/og/{name}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{name}: status");
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/png", "{name}");
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            body.as_ref(),
            std::fs::read(web_og().join(name)).unwrap().as_slice(),
            "{name}: body"
        );
    }
    let res = app()
        .oneshot(
            Request::builder()
                .uri("/static/og/lens.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND, "lens.png: status");
    let ct = res.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/html"), "lens.png: content-type {ct}");
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body.contains(r#"<html lang="en">"#),
        "lens.png: 404 page frame"
    );
}
