//! The shell's help dialog (V2 spec v2-inspector-shell, requirement 17; C1, C3, C4): every
//! inspector page carries a closed `dialog#inspector-help` with its name, syntax, examples,
//! the keyboard table and a guide link; the DNS one also lists its resolvers, flags and types.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
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

async fn send(app: Router, uri: &str) -> (StatusCode, String) {
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn page(uri: &str) -> String {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let (status, body) = send(app, uri).await;
    assert_eq!(status, StatusCode::OK, "GET {uri}");
    body
}

/// The text between the first `open` and the next `close` after it.
fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    s.split(open).nth(1).and_then(|r| r.split(close).next())
}

fn dialog(body: &str) -> &str {
    between(body, "<dialog", "</dialog>").unwrap_or_else(|| panic!("no dialog in {body}"))
}

fn attr(tag: &str, name: &str) -> Option<String> {
    tag.split(&format!(" {name}=\""))
        .nth(1)
        .and_then(|r| r.split('"').next())
        .map(str::to_owned)
}

#[tokio::test]
async fn every_inspector_carries_a_closed_help_dialog() {
    let cases = [
        ("dns", "DNS inspector", "/guide/record-types"),
        ("tls", "TLS inspector", "/guide/tls-protocol"),
        ("http", "HTTP inspector", "/guide/security-headers"),
        ("ip", "IP inspector", "/guide/ip-enrichment"),
    ];
    for (tool, title, guide) in cases {
        let body = page(&format!("/inspect/{tool}")).await;
        let open_tag = body
            .split("<dialog")
            .nth(1)
            .and_then(|r| r.split('>').next())
            .unwrap_or_else(|| panic!("{tool}: no dialog"));
        assert_eq!(
            attr(open_tag, "id").as_deref(),
            Some("inspector-help"),
            "{tool}"
        );
        let classes = attr(open_tag, "class").unwrap_or_default();
        assert!(
            classes.split(' ').any(|c| c == "inspector-help"),
            "{tool}: {open_tag}"
        );
        assert!(
            !open_tag
                .split_whitespace()
                .any(|w| w == "open" || w.starts_with("open=")),
            "{tool}: dialog is open: {open_tag}"
        );
        let d = dialog(&body);
        let h2 = between(d, "<h2", "</h2>").unwrap_or_else(|| panic!("{tool}: no h2"));
        assert!(h2.ends_with(title), "{tool}: h2 {h2:?}");
        assert!(d.contains("class=\"inspector-help__what\""), "{tool}: what");
        assert!(
            d.contains("class=\"inspector-help__syntax\""),
            "{tool}: syntax"
        );

        // Examples: the same values as the page's chips.
        let list = between(d, "<ul class=\"inspector-help__examples\"", "</ul>")
            .unwrap_or_else(|| panic!("{tool}: no examples list"));
        let listed: Vec<String> = list
            .split("<code>")
            .skip(1)
            .map(|c| c.split("</code>").next().unwrap().to_owned())
            .collect();
        let chips: Vec<String> = between(&body, "<ul class=\"inspector-examples\"", "</ul>")
            .unwrap()
            .split("data-example=\"")
            .skip(1)
            .map(|c| c.split('"').next().unwrap().to_owned())
            .collect();
        assert!(!chips.is_empty(), "{tool}: no chips");
        assert_eq!(listed, chips, "{tool}: help examples vs chips");

        // Keyboard table.
        let keys = between(d, "<table class=\"inspector-help__keys\"", "</table>")
            .unwrap_or_else(|| panic!("{tool}: no keys table"));
        for k in ["/", "r", "j", "k", "Enter", "h", "l", "?", "Esc"] {
            assert!(keys.contains(&format!("<kbd>{k}</kbd>")), "{tool}: key {k}");
        }
        assert!(keys.contains("L</kbd>"), "{tool}: Ctrl/Cmd L");
        assert!(
            keys.contains("Ctrl") || keys.contains('⌘'),
            "{tool}: Ctrl/Cmd"
        );

        // Guide link, answering 200 through the pages router.
        let guide_p = between(d, "class=\"inspector-help__guide\"", "</p>")
            .unwrap_or_else(|| panic!("{tool}: no guide paragraph"));
        let href = guide_p
            .split("<a ")
            .nth(1)
            .and_then(|a| attr(&format!(" {a}"), "href"))
            .unwrap_or_else(|| panic!("{tool}: no guide link"));
        assert_eq!(href, guide, "{tool}: guide href");
        let app = Router::new()
            .merge(pages::router())
            .fallback(get(pages::not_found));
        let (status, _) = send(app, &href).await;
        assert_eq!(status, StatusCode::OK, "{tool}: GET {href}");

        // Open button in the tab row.
        let nav = between(&body, "<nav class=\"inspector-tabs", "</nav>")
            .unwrap_or_else(|| panic!("{tool}: no nav.inspector-tabs"));
        let btn = nav
            .split("<button")
            .find(|b| b.contains("inspector-help__open"))
            .and_then(|b| b.split('>').next())
            .unwrap_or_else(|| panic!("{tool}: no help button in the tab row"));
        assert_eq!(
            attr(btn, "aria-controls").as_deref(),
            Some("inspector-help"),
            "{tool}"
        );
        assert!(
            btn.split_whitespace().any(|w| w == "hidden"),
            "{tool}: button not hidden"
        );
    }
}

#[tokio::test]
async fn dns_help_lists_resolvers_flags_and_types_and_tls_has_no_resolvers() {
    let body = page("/inspect/dns").await;
    let d = dialog(&body);
    let catalogue = serde_json::to_value(netray_dns::server_catalogue()).unwrap();
    let mut names = Vec::new();
    for key in ["providers", "groups"] {
        for e in catalogue[key].as_array().unwrap() {
            names.push(e["name"].as_str().unwrap().to_owned());
        }
    }
    assert!(!names.is_empty());
    for n in names {
        assert!(d.contains(&format!("<code>@{n}</code>")), "resolver @{n}");
    }
    for f in netray_dns::parser::FLAGS {
        assert!(d.contains(&format!("<code>+{f}</code>")), "flag +{f}");
    }
    let types = serde_json::to_value(netray_dns::record_types()).unwrap();
    for t in types["record_types"].as_array().unwrap() {
        let n = t["name"].as_str().unwrap();
        assert!(d.contains(&format!("<code>{n}</code>")), "type {n}");
    }

    let tls = page("/inspect/tls").await;
    assert!(
        !tls.contains("inspector-help__resolvers"),
        "tls help lists resolvers"
    );
}
