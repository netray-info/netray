//! The inspector tab row (V2 spec v2-inspector-shell, requirement 3, C3 and C10-C13): five tabs
//! DNS, TLS, HTTP, EMAIL, IP in that order, the current one marked, the target carried over
//! where the other inspector takes it, a non-ASCII host in its UTS #46 form; and the tab's style.
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

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!(" {name}=\"");
    let rest = tag.split(&needle).nth(1)?;
    Some(rest.split('"').next()?.to_string())
}

/// (text, href decoded, current) of every tab in the `nav.inspector-tabs`.
fn tabs(body: &str) -> Vec<(String, String, bool)> {
    let nav = body
        .split("<nav class=\"inspector-tabs\"")
        .nth(1)
        .expect("a nav.inspector-tabs on the page")
        .split("</nav>")
        .next()
        .unwrap();
    nav.split("<a ")
        .skip(1)
        .map(|a| {
            let (tag, rest) = a.split_once('>').unwrap();
            let tag = format!(" {tag}");
            assert!(
                attr(&tag, "class")
                    .is_some_and(|c| c.split(' ').any(|c| c == "inspector-tabs__tab")),
                "tab class in {tag}"
            );
            let href = attr(&tag, "href").expect("tab href").replace("&amp;", "&");
            let text = rest.split("</a>").next().unwrap().trim().to_string();
            (
                text,
                percent_decode(&href),
                attr(&tag, "aria-current").as_deref() == Some("page"),
            )
        })
        .collect()
}

#[tokio::test]
async fn inspector_tabs_carry_the_target_and_mark_the_current_inspector() {
    // (request uri, current tab index, hrefs of the other tabs as (index, href or prefix))
    let rows: &[(&str, usize, &[(usize, &str)])] = &[
        (
            "/inspect/tls?h=example.com:8443",
            1,
            &[
                (0, "/inspect/dns?q=example.com"),
                (2, "/inspect/http?url=https://example.com/"),
                (3, "/inspect/email?d=example.com"),
            ],
        ),
        (
            "/inspect/http?url=https://www.example.com/path",
            2,
            &[
                (0, "/inspect/dns?q=www.example.com"),
                (1, "/inspect/tls?h=www.example.com:443"),
                (3, "/inspect/email?d=example.com"),
            ],
        ),
        (
            "/inspect/dns?q=b%C3%BCcher.example.com",
            0,
            &[(1, "/inspect/tls?h=xn--bcher-kva.example.com:443")],
        ),
        (
            "/inspect/ip?ip=192.0.2.24",
            4,
            &[
                (0, "/inspect/dns"),
                (1, "/inspect/tls"),
                (2, "/inspect/http"),
                (3, "/inspect/email"),
            ],
        ),
    ];
    for (uri, current, want) in rows {
        let got = tabs(&page(uri).await);
        let texts: Vec<&str> = got.iter().map(|t| t.0.as_str()).collect();
        assert_eq!(texts, ["DNS", "TLS", "HTTP", "EMAIL", "IP"], "GET {uri}");
        let marked: Vec<usize> = got
            .iter()
            .enumerate()
            .filter(|(_, t)| t.2)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(marked, [*current], "current tab of GET {uri}");
        for (i, href) in *want {
            assert_eq!(&got[*i].1, href, "tab {} of GET {uri}", got[*i].0);
        }
    }
}

#[test]
fn inspector_tab_style_is_mono_12px_uppercase() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut css = String::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_some_and(|x| x == "css") {
            css.push_str(&std::fs::read_to_string(e.path()).unwrap());
            css.push('\n');
        }
    }
    let rule = css
        .split("}")
        .find(|r| {
            r.split('{')
                .next()
                .is_some_and(|s| s.split(',').any(|s| s.trim() == ".inspector-tabs__tab"))
        })
        .expect("a .inspector-tabs__tab rule")
        .split('{')
        .nth(1)
        .unwrap()
        .replace(char::is_whitespace, "");
    assert!(rule.contains("font-family:var(--font-mono)"), "{rule}");
    assert!(
        rule.contains("font-size:12px") || rule.contains("font-size:var(--font-size-label)"),
        "{rule}"
    );
    assert!(rule.contains("text-transform:uppercase"), "{rule}");
}
