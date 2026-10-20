//! V2 spec v2-inspector-shell, requirement 2 (C2, C9): the inspector page's site header carries
//! the inspector's name and tagline, the nav marks Inspectors current, and the idle page has no
//! visible h1 tool title.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use netray_web::tokens::CSS;
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
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn between<'a>(html: &'a str, open: &str, close: &str) -> &'a str {
    let start = html
        .find(open)
        .unwrap_or_else(|| panic!("missing `{open}`"));
    let end = start
        + html[start..]
            .find(close)
            .unwrap_or_else(|| panic!("missing `{close}`"));
    &html[start..end + close.len()]
}

#[tokio::test]
async fn inspector_header_names_the_inspector_and_marks_inspectors_current() {
    // (tool, name, tagline); email is a later phase.
    let rows = [
        ("dns", "DNS inspector", "DNS, resolved"),
        ("tls", "TLS inspector", "TLS, illuminated"),
        ("http", "HTTP inspector", "HTTP, inspected"),
        ("ip", "IP inspector", "IP, located"),
    ];
    for (tool, name, tagline) in rows {
        let html = page(&format!("/inspect/{tool}")).await;
        let header = between(&html, "<header", "</header>");

        let want_name = format!(r#"<span class="page-inspector__name">{name}</span>"#);
        assert!(
            header.contains(&want_name),
            "{tool}: no `{want_name}` in {header}"
        );
        assert!(
            header.contains(r#"<span class="page-inspector__sep" aria-hidden="true">·</span>"#),
            "{tool}: no separator in {header}"
        );
        let want_tag = format!(r#"<span class="page-tagline">{tagline}</span>"#);
        assert!(
            header.contains(&want_tag),
            "{tool}: no `{want_tag}` in {header}"
        );
        assert!(
            !header.contains("your domain, in full spectrum"),
            "{tool}: site tagline must be replaced: {header}"
        );

        let nav = between(header, r#"<nav class="page-nav">"#, "</nav>");
        let link = nav
            .split("<a ")
            .find(|a| a.contains(">Inspectors</a>"))
            .unwrap_or_else(|| panic!("{tool}: no Inspectors link in {nav}"));
        assert!(
            link.contains(r#"aria-current="page""#),
            "{tool}: Inspectors not current: {link}"
        );

        // The only h1 on the idle page is the visually hidden one.
        let h1s: Vec<&str> = html.split("<h1").skip(1).collect();
        assert_eq!(h1s.len(), 1, "{tool}: exactly one h1 expected");
        assert!(
            h1s[0].starts_with(r#" class="visually-hidden">"#),
            "{tool}: the idle h1 must be visually hidden: <h1{}",
            &h1s[0][..h1s[0].len().min(60)]
        );
    }
}

/// The last declaration of `prop` in the rule whose selector is exactly `selector`, across
/// every static stylesheet.
fn declared(selector: &str, prop: &str) -> Option<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    let mut found = None;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_none_or(|x| x != "css") {
            continue;
        }
        let css = std::fs::read_to_string(e.path()).unwrap();
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            if rest[..open].trim().rsplit('}').next().map(str::trim) == Some(selector) {
                for d in rest[open + 1..open + close].split(';') {
                    if let Some((k, v)) = d.split_once(':')
                        && k.trim() == prop
                    {
                        found = Some(v.trim().to_string());
                    }
                }
            }
            rest = &rest[open + close + 1..];
        }
    }
    found
}

/// `value`, with a `var(--x)` replaced by its `:root` token value.
fn resolve(value: &str) -> String {
    let Some(name) = value.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) else {
        return value.to_string();
    };
    let needle = format!("{name}:");
    let at = CSS
        .find(&needle)
        .unwrap_or_else(|| panic!("unknown token {name}"));
    CSS[at + needle.len()..]
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn inspector_name_and_tagline_have_the_board_sizes() {
    let size = |sel: &str| {
        resolve(&declared(sel, "font-size").unwrap_or_else(|| format!("<{sel} missing>")))
    };
    assert_eq!(size(".page-inspector__name"), "21px");
    assert_eq!(size(".page-tagline"), "13px");
    assert_eq!(
        declared(".page-inspector__name", "font-weight").as_deref(),
        Some("650")
    );
}
