//! Example chips of the inspector shell (v2-inspector-shell, R5, C5 and C16): the idle page
//! carries two to four example links per tool; each href, percent-decoded, is the prefilled
//! page, and following it answers 200 with the input prefilled.
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

async fn get(uri: &str) -> String {
    let app: Router = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(if b[i] == b'+' { b' ' } else { b[i] });
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// `(href, data-example)` of every chip in the examples list.
fn chips(body: &str) -> Vec<(String, String)> {
    let list = body
        .split("<ul class=\"inspector-examples\"")
        .nth(1)
        .and_then(|r| r.split("</ul>").next())
        .unwrap_or_else(|| panic!("no ul.inspector-examples in {body}"));
    list.split("<a ")
        .skip(1)
        .map(|a| {
            let tag = a.split('>').next().unwrap();
            assert!(tag.contains("inspector-examples__chip"), "{tag}");
            let attr = |name: &str| {
                tag.split(&format!("{name}=\""))
                    .nth(1)
                    .and_then(|r| r.split('"').next())
                    .unwrap_or_else(|| panic!("no {name} in {tag}"))
                    .to_owned()
            };
            (attr("href"), attr("data-example"))
        })
        .collect()
}

fn allowed(example: &str) -> bool {
    let host_ok = |h: &str| {
        h == "example.com"
            || h.ends_with(".example.com")
            || h == "netray.info"
            || h.ends_with(".netray.info")
            || h.starts_with("192.0.2.")
            || h.starts_with("2001:db8:")
            || h.starts_with("[2001:db8:")
    };
    example
        .split(|c: char| c.is_whitespace() || c == '/' || c == '@')
        .filter(|w| !w.is_empty() && !w.ends_with(':'))
        .any(host_ok)
}

#[tokio::test]
async fn idle_page_offers_prefilled_example_chips() {
    for (tool, field) in [("dns", "q"), ("tls", "h"), ("http", "url"), ("ip", "ip")] {
        let body = get(&format!("/inspect/{tool}")).await;
        let found = chips(&body);
        assert!(
            (2..=4).contains(&found.len()),
            "{tool}: {} chips",
            found.len()
        );
        for (href, example) in found {
            assert_eq!(
                decode(&href),
                format!("/inspect/{tool}?{field}={example}"),
                "{tool} chip href"
            );
            assert!(allowed(&example), "{tool}: example {example:?} not allowed");
            let page = get(&href).await;
            assert!(
                page.contains(&format!("value=\"{example}\"")),
                "{tool}: {href} does not prefill {example:?}"
            );
        }
    }
}
