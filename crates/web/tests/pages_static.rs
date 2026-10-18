//! Contract test for the static routes (V2 spec v2-pages, requirement 10; criteria C3, C20).
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use tower::ServiceExt;

const BASE: &str = include_str!("../static/base.css");
const PAGES: &str = include_str!("../static/pages.css");
const ISLANDS: &str = include_str!("../islands/dist/islands.js");
const LOGO: &str = include_str!("../static/logo.svg");
const FAVICON: &str = include_str!("../static/favicon.svg");

/// R44 order of SDD 8.3: stylesheet content and its root class rule.
const COMPONENTS: [(&str, &str, &str); 10] = [
    ("grade", include_str!("../static/grade.css"), ".grade"),
    (
        "run_line",
        include_str!("../static/run_line.css"),
        ".run-line",
    ),
    (
        "progress_ray",
        include_str!("../static/progress_ray.css"),
        ".progress-ray",
    ),
    (
        "spectrum",
        include_str!("../static/spectrum.css"),
        ".spectrum",
    ),
    ("trace", include_str!("../static/trace.css"), ".trace"),
    (
        "check_row",
        include_str!("../static/check_row.css"),
        ".check-row",
    ),
    (
        "evidence_block",
        include_str!("../static/evidence_block.css"),
        ".evidence-block",
    ),
    (
        "diff_block",
        include_str!("../static/diff_block.css"),
        ".diff-block",
    ),
    (
        "grouping_switch",
        include_str!("../static/grouping_switch.css"),
        ".grouping-switch",
    ),
    (
        "query_input",
        include_str!("../static/query_input.css"),
        ".query-input",
    ),
];

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn fetch(path: &str) -> (Response, String) {
    let res = app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (Response::from_parts(parts, Body::empty()), text)
}

fn header(res: &Response, name: &str) -> String {
    res.headers()
        .get(name)
        .unwrap_or_else(|| panic!("missing header {name}"))
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn app_css_is_tokens_base_components_pages_in_order() {
    let (res, body) = fetch("/static/app.css").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(header(&res, "content-type").starts_with("text/css"));
    assert_eq!(header(&res, "cache-control"), "public, max-age=3600");
    assert!(
        body.starts_with(netray_web::tokens::CSS),
        "body must begin with tokens::CSS"
    );

    let mut at = netray_web::tokens::CSS.len();
    let mut chain: Vec<(&str, &str)> = vec![("base.css", BASE)];
    for (name, css, _) in COMPONENTS {
        chain.push((name, css));
    }
    chain.push(("pages.css", PAGES));
    for (name, css) in chain {
        let i = body[at..]
            .find(css)
            .unwrap_or_else(|| panic!("{name} missing or out of order in app.css"));
        at += i + css.len();
    }
    for (name, _, class) in COMPONENTS {
        assert!(
            body.contains(class),
            "app.css lacks the root class {class} of {name}"
        );
    }
}

#[tokio::test]
async fn islands_js_is_the_built_bundle() {
    let (res, body) = fetch("/static/islands.js").await;
    assert_eq!(res.status(), StatusCode::OK);
    let ct = header(&res, "content-type");
    assert!(
        ct.starts_with("text/javascript") || ct.starts_with("application/javascript"),
        "{ct}"
    );
    assert_eq!(header(&res, "cache-control"), "public, max-age=3600");
    assert_eq!(body, ISLANDS);
}

#[tokio::test]
async fn svg_files_are_served_as_the_sources() {
    for (path, src) in [("/static/logo.svg", LOGO), ("/static/favicon.svg", FAVICON)] {
        let (res, body) = fetch(path).await;
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        assert!(
            header(&res, "content-type").starts_with("image/svg+xml"),
            "{path}"
        );
        assert_eq!(
            header(&res, "cache-control"),
            "public, max-age=3600",
            "{path}"
        );
        assert_eq!(body, src, "{path}");
    }
}

#[tokio::test]
async fn unknown_static_paths_answer_the_404_page() {
    for path in ["/static/missing.css", "/static/og/nope.png"] {
        let (res, body) = fetch(path).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
        assert!(body.contains("<html"), "{path} must answer the 404 page");
    }
}
