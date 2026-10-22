//! The inspector page links back to the registrable domain of a prefilled host name (V2 spec
//! v2-inspectors, requirement 12, C14 and C4): a run of `<field>=<value>` carries
//! `<a class="inspector-backlink" href="/<registrable>">` in its start fragment for a name with a
//! registrable domain (Public Suffix List), and no back-link for an IP literal or a public suffix
//! (v2-inspector-shell requirement 6 moved the link from the page into the run's head).
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
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

/// The back-link targets of a posted run: the `inspector-backlink` hrefs of its start fragment
/// (v2-inspector-shell requirement 6 moved the link from the page into the run's head).
async fn added_links(tool: &str, query: &str) -> BTreeSet<String> {
    let app: Router = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(
            Request::post(format!("/inspect/{tool}/run"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(query.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "POST {tool} {query}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let data = text
        .lines()
        .find_map(|l| l.strip_prefix("data:"))
        .expect("a first fragment");
    let j: serde_json::Value = serde_json::from_str(data.trim()).unwrap();
    let html = j["html"].as_str().unwrap_or_default();
    html.split(r#"class="inspector-backlink" href=""#)
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn prefilled_target_links_back_to_its_registrable_domain() {
    // (tool, query string, expected added links)
    let rows: &[(&str, &str, &[&str])] = &[
        ("tls", "h=www.example.com", &["/example.com"]),
        ("tls", "h=192.0.2.1", &[]),
        ("tls", "h=www.example.com:443", &["/example.com"]),
        (
            "http",
            "url=https://www.example.com/path",
            &["/example.com"],
        ),
        ("dns", "q=_dmarc.example.com%20TXT", &["/example.com"]),
        ("dns", "q=a.b.example.co.uk%20A", &["/example.co.uk"]),
        ("tls", "h=%5B2001%3Adb8%3A%3A1%5D%3A443", &[]),
        ("dns", "q=com%20NS", &[]),
        ("ip", "ip=192.0.2.1", &[]),
    ];
    for (tool, query, want) in rows {
        let want: BTreeSet<String> = want.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            added_links(tool, query).await,
            want,
            "POST /inspect/{tool}/run {query}"
        );
    }
}
