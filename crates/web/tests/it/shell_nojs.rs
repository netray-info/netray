//! The inspector shell without JavaScript (V2 spec v2-inspector-shell, requirement 26): the
//! server HTML alone carries the header name, the five tabs as plain links, the GET form,
//! the example links and the `<noscript>` notice.
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
    fn admit(&self, _tool: Tool, _client: IpAddr, _input: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _req: JobRequest) -> Result<Job, String> {
        Err("stub".into())
    }
}

async fn page(tool: &str) -> String {
    let app: Router = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(
            Request::get(format!("/inspect/{tool}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "{tool}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Every `<tag ...>` opening whose attributes contain `class_marker`.
fn tags<'a>(html: &'a str, tag: &str, class_marker: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i..];
        let end = after.find('>').unwrap_or(after.len());
        let t = &after[..end];
        if t.contains(class_marker) {
            out.push(t);
        }
        rest = &after[open.len()..];
    }
    out
}

#[tokio::test]
async fn the_idle_inspector_page_renders_its_shell_in_the_server_html() {
    let rows = [("dns", "q"), ("tls", "h"), ("http", "url"), ("ip", "ip")];
    for (tool, field) in rows {
        let html = page(tool).await;

        assert!(
            html.contains("page-inspector__name"),
            "{tool}: no header name"
        );

        let nav_start = html
            .find(r#"<nav class="inspector-tabs""#)
            .unwrap_or_else(|| panic!("{tool}: no nav.inspector-tabs"));
        let nav = &html[nav_start..];
        let nav = &nav[..nav.find("</nav>").expect("nav closes")];
        let links: Vec<_> = tags(nav, "a", "href=\"");
        assert_eq!(links.len(), 5, "{tool}: tabs {nav}");

        let forms = tags(&html, "form", "query-input");
        assert_eq!(forms.len(), 1, "{tool}: forms");
        assert!(forms[0].contains(r#"method="get""#), "{tool}: {}", forms[0]);
        assert!(
            forms[0].contains(&format!(r#"action="/inspect/{tool}""#)),
            "{tool}: {}",
            forms[0]
        );
        assert!(
            html.contains(&format!(r#"name="{field}""#)),
            "{tool}: field"
        );

        let chips = tags(&html, "a", "inspector-examples__chip");
        assert!(chips.len() >= 2, "{tool}: chips {chips:?}");
        assert!(
            chips.iter().all(|c| c.contains("href=\"")),
            "{tool}: chip without href"
        );

        let ns = html
            .find("<noscript>")
            .unwrap_or_else(|| panic!("{tool}: no noscript"));
        let ns = &html[ns..];
        let ns = &ns[..ns.find("</noscript>").expect("noscript closes")];
        assert!(ns.contains("JavaScript"), "{tool}: notice {ns}");
    }
}
