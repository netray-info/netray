//! `/guide/scoring` describes `dkim_selectors` as the code implements it (spec
//! v2-dkim-legacy-urls requirement 8; Phase 3 criteria C4, C13).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages;
use netray_web::selectors::MAX_DKIM_SELECTORS;
use tower::ServiceExt;

async fn scoring() -> String {
    let res = Router::new()
        .merge(pages::router())
        .oneshot(
            Request::builder()
                .uri("/guide/scoring")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn the_selector_count_is_the_codes_limit() {
    let page = scoring().await;
    assert_eq!(MAX_DKIM_SELECTORS, 5);
    assert!(
        page.contains("1&ndash;5 selectors per request")
            || page.contains("1–5 selectors per request"),
        "the guide states 1-5 selectors"
    );
    assert!(!page.contains("1&ndash;10"), "the guide still says 1-10");
}

#[tokio::test]
async fn the_api_sample_reads_the_run_answer() {
    let page = scoring().await;
    assert!(
        page.contains(".run.checks[]"),
        "the jq sample reads the run answer's checks"
    );
    assert!(
        !page.contains("&#39;.checks[]") && !page.contains("'.checks[]"),
        "the jq sample reads a top-level .checks the answer does not have"
    );
}

#[tokio::test]
async fn selector_runs_are_described_as_not_stored() {
    let page = scoring().await;
    assert!(
        !page.contains("Results are cached per domain for 5 minutes"),
        "V1 cache wording"
    );
    assert!(
        page.contains("never stored in the history"),
        "the guide says a selector run is never stored"
    );
}
