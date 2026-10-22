//! V2 spec v2-ui-polish, requirements 6 and 7 (C5, C6, C14, C15): the check list sits in one
//! bordered card under its heading, and the page-inspector crumb hides on a phone.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::evidence::{Block, BlockId, Shape};
use netray_model::{CheckResult, Protocol, Status};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

const BLOCK: &str = "http.headers";
const FORM: &str = "url=https%3A%2F%2Fexample.com%2F";

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let part = JobPart {
            label: "Headers".into(),
            output: Box::pin(async {
                Ok(vec![Block {
                    id: BlockId::parse(BLOCK).unwrap(),
                    title: "Response headers".into(),
                    shape: Shape::Text("server: example".into()),
                }])
            }),
        };
        let checks = catalogue()
            .entries()
            .iter()
            .filter(|e| e.protocol == Protocol::Http)
            .map(|e| CheckResult {
                id: e.id.clone(),
                status: Status::Pass,
                findings: Vec::new(),
                evidence: Vec::new(),
            })
            .collect::<Vec<_>>();
        Ok(Job::new(vec![part]).with_band(Protocol::Http, Box::pin(async move { Ok(checks) })))
    }
}

async fn body(app: &Router, req: Request<Body>) -> String {
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

/// The live band fragment and the shared page of one run.
async fn live_and_shared() -> (String, String) {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let stream = body(
        &app,
        Request::post("/inspect/http/run")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(FORM))
            .unwrap(),
    )
    .await;
    let mut band = String::new();
    let mut key = String::new();
    for raw in stream.split("\n\n") {
        let data: String = raw
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        let html = j["html"].as_str().unwrap_or_default();
        match j["id"].as_str().unwrap_or_default() {
            "inspect-band" => band = html.to_owned(),
            "inspect-share" => {
                let a = html.find("data-share-key=\"").unwrap() + 16;
                key = html[a..a + html[a..].find('"').unwrap()].to_owned();
            }
            _ => {}
        }
    }
    assert!(!band.is_empty() && !key.is_empty(), "{stream}");
    let page = body(
        &app,
        Request::get(format!("/inspect/http?r={key}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    (band, page)
}

/// Asserts the checks section of `html` holds all rows and links inside one card after the title.
fn assert_card(html: &str, what: &str) {
    let s = html
        .find("inspector-checks")
        .map(|i| html[..i].rfind("<section").expect("section tag"))
        .unwrap_or_else(|| panic!("{what}: no section.inspector-checks"));
    let section = &html[s..s + html[s..].find("</section>").unwrap()];
    let title = section
        .find(r#"<h2 class="inspector-checks__title">"#)
        .unwrap_or_else(|| panic!("{what}: no title in {section}"));
    let open = r#"<div class="inspector-checks__card">"#;
    assert_eq!(section.matches(open).count(), 1, "{what}: {section}");
    let card = section.find(open).unwrap();
    assert!(title < card, "{what}: title after card");
    let (mut depth, mut end) = (0i32, None);
    let mut i = card;
    while i < section.len() {
        if !section.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if section[i..].starts_with("<div") {
            depth += 1;
        } else if section[i..].starts_with("</div>") {
            depth -= 1;
            if depth == 0 {
                end = Some(i);
                break;
            }
        }
        i += 1;
    }
    let inner = &section[card..end.unwrap_or_else(|| panic!("{what}: card never closes"))];
    let rows = section.matches(r#"<article class="check-row""#).count();
    let links = section.matches(r#"class="evidence-section__link""#).count();
    assert!(rows > 0, "{what}: rows {rows}, links {links}");
    assert_eq!(
        inner.matches(r#"<article class="check-row""#).count(),
        rows,
        "{what}"
    );
    assert_eq!(
        inner.matches(r#"class="evidence-section__link""#).count(),
        links,
        "{what}"
    );
}

/// The declaration block of the first rule whose selector list contains `selector`.
fn rule_opt<'a>(css: &'a str, selector: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(p) = css[from..].find('{') {
        let head_start = css[..from + p].rfind(['}', '{']).map_or(0, |i| i + 1);
        let head = &css[head_start..from + p];
        let end = from + p + css[from + p..].find('}').unwrap();
        if head.split(',').any(|s| s.trim() == selector) {
            return Some(&css[from + p + 1..end]);
        }
        from = end + 1;
    }
    None
}

fn rule<'a>(css: &'a str, selector: &str) -> &'a str {
    rule_opt(css, selector).unwrap_or_else(|| panic!("no rule for {selector}"))
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tokio::test]
async fn c5_c14_check_list_sits_in_one_card_live_and_shared() {
    let (band, page) = live_and_shared().await;
    assert_card(&band, "live band");
    assert_card(&page, "shared page");
}

#[test]
fn c5_c14_card_rules_in_the_shell_stylesheet() {
    let css = include_str!("../../static/inspector_shell.css");
    let card = squash(rule(css, ".inspector-checks__card"));
    for want in [
        "border: var(--line-hair) solid var(--color-line)",
        "border-radius: var(--radius-max)",
        "background: var(--color-surface)",
    ] {
        assert!(card.contains(want), "{want} missing in {card}");
    }
    let first = squash(rule(
        css,
        // the gate keeps component classes in their own stylesheet: the first child, a check row
        ".inspector-checks__card > :first-child",
    ));
    assert!(first.contains("border-top: 0"), "{first}");
}

#[test]
fn c6_c15_page_inspector_hides_under_480px() {
    let mut found = false;
    for css in [
        include_str!("../../static/pages.css"),
        include_str!("../../static/inspector_shell.css"),
    ] {
        let mut from = 0;
        while let Some(p) = css[from..].find("@media (max-width: 480px)") {
            let open = from + p + css[from + p..].find('{').unwrap();
            let (mut depth, mut end) = (0, open);
            for (i, c) in css[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let block = &css[open + 1..end];
            let hidden = rule_opt(&format!("{block} }}"), ".page-inspector")
                .is_some_and(|r| squash(r).contains("display: none"));
            found |= hidden;
            from = end;
        }
    }
    assert!(
        found,
        ".page-inspector gets display: none in a max-width 480px block"
    );
}
