//! pustina-findings C4/C11: the header logo is inline SVG painted from tokens, never an `<img>`.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::entries::render;
use netray_web::pages::{
    self, InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Ok(Job::new(vec![]))
    }
}

async fn get_on(app: Router, path: &str) -> String {
    let res = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "{path}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn header_of(html: &str) -> &str {
    let from = html.find("<header").expect("page has a header");
    let to = html[from..].find("</header>").expect("header closes");
    &html[from..from + to]
}

#[tokio::test]
async fn every_page_header_carries_an_inline_svg_logo_and_no_logo_img() {
    let inspect = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let pages = vec![
        ("/guide/csp", get_on(pages::router(), "/guide/csp").await),
        ("/tools", get_on(pages::router(), "/tools").await),
        ("/inspect/dns", get_on(inspect, "/inspect/dns").await),
        ("privacy", render::privacy().unwrap()),
    ];
    for (ctx, html) in pages {
        let header = header_of(&html);
        assert!(header.contains("<svg"), "{ctx}: no inline <svg> in header");
        assert!(
            !html.contains(r#"src="/static/logo.svg""#),
            "{ctx}: page references /static/logo.svg through <img>"
        );
        let svg = &header[header.find("<svg").unwrap_or(0)..];
        let svg = &svg[..svg.find("</svg>").unwrap_or(svg.len())];
        assert!(
            svg.contains("currentColor") || svg.contains("var(--"),
            "{ctx}: logo colour is not token-driven"
        );
        assert!(
            !svg.contains("#") || svg.contains("url(#"),
            "{ctx}: logo hard-codes a hex colour"
        );
    }
}
