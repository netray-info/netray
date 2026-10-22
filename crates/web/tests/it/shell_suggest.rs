//! The shell's autocomplete tables (V2 spec v2-inspector-shell, requirements 15 and 16, C6 and
//! C21): the DNS shell root carries the DNS crate's own tables in data attributes, the form box
//! holds the suggestion list, and no other inspector carries a `data-suggest*` attribute.
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

async fn page(uri: &str) -> String {
    let app: Router = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The value of `name="..."` inside the opening tag of the shell root.
fn root_attr(body: &str, name: &str) -> Option<String> {
    let tag = body
        .split("<div class=\"inspector-shell\"")
        .nth(1)?
        .split('>')
        .next()?;
    let rest = tag.split(&format!(" {name}=\"")).nth(1)?;
    Some(rest.split('"').next()?.to_string())
}

fn names(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                match (k.as_str(), x) {
                    ("name", serde_json::Value::String(s)) => out.push(s.clone()),
                    _ => names(x, out),
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| names(x, out)),
        _ => {}
    }
}

fn words(attr: Option<String>) -> Vec<String> {
    attr.unwrap_or_default()
        .split(' ')
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn dns_shell_carries_the_autocomplete_tables_and_other_inspectors_do_not() {
    let body = page("/inspect/dns").await;
    assert_eq!(root_attr(&body, "data-suggest").as_deref(), Some("dns"));

    let types = words(root_attr(&body, "data-suggest-types"));
    let mut expected = Vec::new();
    names(
        &serde_json::to_value(netray_dns::record_types()).unwrap(),
        &mut expected,
    );
    assert!(expected.len() > 5, "record_types() names: {expected:?}");
    for t in &expected {
        assert!(types.contains(t), "type {t} missing from {types:?}");
    }

    let flags = words(root_attr(&body, "data-suggest-flags"));
    for f in ["+trace", "+dnssec", "+tcp"] {
        assert!(
            flags.iter().any(|x| x == f),
            "flag {f} missing from {flags:?}"
        );
    }

    let servers = words(root_attr(&body, "data-suggest-servers"));
    assert!(
        servers.iter().any(|x| x == "cloudflare"),
        "cloudflare missing from {servers:?}"
    );

    let list = body
        .split("<ul class=\"inspector-suggest\"")
        .nth(1)
        .expect("ul.inspector-suggest in the form box");
    let tag = list.split('>').next().unwrap();
    assert!(tag.contains("id=\"inspector-suggest\""), "{tag}");
    assert!(tag.contains("role=\"listbox\""), "{tag}");
    assert!(tag.split_whitespace().any(|a| a == "hidden"), "{tag}");

    let tls = page("/inspect/tls").await;
    assert!(
        !tls.contains("data-suggest"),
        "TLS page carries data-suggest"
    );
}
