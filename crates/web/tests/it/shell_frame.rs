//! One shell builds every inspector page (V2 spec v2-inspector-shell, requirement 1, C1 and C14):
//! the `inspector_shell` macro and the `shell` island are the only frame; no template branches
//! on a tool name or stream URL, and the DNS builder sits inside the shell root.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
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

async fn page(tool: &str) -> String {
    let app: Router = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let uri = format!("/inspect/{tool}");
    let res = app
        .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn web_file(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The markup of the first `<div ...>` opened by `needle`, up to its matching `</div>`.
fn div_at(body: &str, needle: &str) -> Option<String> {
    let start = body.find(needle)?;
    let mut depth = 0usize;
    let mut i = start;
    while i < body.len() {
        let rest = &body[i..];
        if rest.starts_with("<div") {
            depth += 1;
        } else if rest.starts_with("</div>") {
            depth -= 1;
            if depth == 0 {
                return Some(body[start..i + 6].to_owned());
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    None
}

#[tokio::test]
async fn one_shell_builds_every_inspector_page() {
    // the builder sits in the shell root for dns; the other tools have none
    let dns = page("dns").await;
    let root = div_at(&dns, "<div class=\"inspector-shell\"")
        .expect("dns page has an .inspector-shell root");
    assert!(
        root.contains("data-island=\"shell\""),
        "shell root mounts the shell island"
    );
    assert!(
        root.contains("data-tool=\"dns\""),
        "shell root names its tool"
    );
    assert_eq!(
        dns.matches("data-island=\"dns\"").count(),
        1,
        "one dns builder"
    );
    assert!(
        root.contains("<fieldset data-island=\"dns\""),
        "the dns builder is inside the shell root"
    );

    for tool in ["dns", "tls", "http", "ip"] {
        let body = page(tool).await;
        assert_eq!(
            body.matches("class=\"inspector-shell\"").count(),
            1,
            "{tool}: exactly one shell root"
        );
        assert_eq!(
            body.matches("data-island=\"shell\"").count(),
            1,
            "{tool}: exactly one shell island"
        );
        assert!(
            body.contains(&format!("data-tool=\"{tool}\"")),
            "{tool}: data-tool equals the tool"
        );
        if tool != "dns" {
            assert!(
                !body.contains("data-island=\"dns\""),
                "{tool}: no dns fieldset"
            );
        }
    }
}

#[test]
fn the_frame_is_one_macro_one_stylesheet_one_island_with_no_tool_branch() {
    let macro_src = web_file("templates/components/inspector_shell.html");
    assert!(
        macro_src.contains("macro inspector_shell("),
        "inspector_shell macro is defined"
    );
    web_file("static/inspector_shell.css");

    let page_src = web_file("templates/pages/inspector.html");
    assert!(
        page_src.contains("inspector_shell"),
        "pages/inspector.html builds the page with the macro"
    );

    for (name, src) in [
        ("pages/inspector.html", &page_src),
        ("components/inspector_shell.html", &macro_src),
    ] {
        assert!(
            !src.contains("/inspect/dns/run"),
            "{name}: no stream URL branch"
        );
        for tool in ["dns", "tls", "http", "email", "ip"] {
            for form in [
                format!("== \"{tool}\""),
                format!("== '{tool}'"),
                format!("\"{tool}\" =="),
            ] {
                assert!(!src.contains(&form), "{name}: no comparison `{form}`");
            }
        }
        assert!(
            !src.contains("stream_url =="),
            "{name}: no branch on stream_url"
        );
    }

    web_file("islands/src/shell.ts");
    let main = web_file("islands/src/main.ts");
    assert!(
        main.contains("register(\"shell\""),
        "main.ts registers the shell island"
    );
}
