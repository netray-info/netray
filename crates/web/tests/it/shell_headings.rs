//! V2 spec v2-inspector-shell, requirement 7 (C7, C20): every result section is a card with an h2
//! at 20 px weight 650; the idle page's h1 is visually hidden and names the inspector; a finished
//! run leaves exactly one h1 and an h2 per part.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let chain = Block {
            id: BlockId::parse("tls.chain").unwrap(),
            title: "Chain".into(),
            shape: Shape::Chain(vec![vec![(
                "subject".into(),
                Value::Text("leaf.example.com".into()),
            )]]),
        };
        let handshake = Block {
            id: BlockId::parse("tls.handshake").unwrap(),
            title: "Handshake".into(),
            shape: Shape::Fields(vec![("protocol".into(), Value::Text("TLSv1.3".into()))]),
        };
        let part = |label: &str, block: Block| JobPart {
            label: label.into(),
            output: Box::pin(async move { Ok(vec![block]) }),
        };
        Ok(Job::new(vec![
            part("chain", chain),
            part("handshake", handshake),
        ]))
    }
}

fn app() -> Router {
    inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()))
}

async fn get(app: &Router, uri: &str) -> String {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// (fragment id, html) per fragment event of the run.
async fn run(app: &Router, body: &str) -> Vec<(String, String)> {
    let res = app
        .clone()
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut out = Vec::new();
    for raw in text.split("\n\n") {
        let mut name = "";
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        if name != "fragment" {
            continue;
        }
        let j: serde_json::Value = serde_json::from_str(&data).unwrap();
        out.push((
            j["id"].as_str().unwrap().to_owned(),
            j["html"].as_str().unwrap().to_owned(),
        ));
    }
    out
}

/// Headings h1-h6 in document order as (level, visible text).
fn headings(body: &str) -> Vec<(u8, String)> {
    let re = regex::Regex::new(r"(?s)<h([1-6])[^>]*>(.*?)</h([1-6])>").unwrap();
    let tag = regex::Regex::new(r"(?s)<[^>]*>").unwrap();
    re.captures_iter(body)
        .map(|c| {
            let text = tag.replace_all(&c[2], " ");
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            (c[1].parse().unwrap(), text)
        })
        .collect()
}

fn assert_no_skip(hs: &[(u8, String)]) {
    let mut prev = 0u8;
    for (level, text) in hs {
        assert!(
            *level <= prev + 1,
            "h{level} {text:?} after h{prev}: {hs:?}"
        );
        prev = *level;
    }
}

/// Declaration blocks of every rule in static/*.css whose selector list matches `pred`.
fn rules(pred: impl Fn(&str) -> bool) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    let re = regex::Regex::new(r"([^{}]+)\{([^{}]*)\}").unwrap();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "css") {
            let css = std::fs::read_to_string(&path).unwrap();
            for c in re.captures_iter(&css) {
                if c[1].split(',').any(|s| pred(s.trim())) {
                    out.push(c[2].split_whitespace().collect::<Vec<_>>().join(" "));
                }
            }
        }
    }
    out
}

#[tokio::test]
async fn c7_c20_sections_are_cards_with_h2_and_the_idle_h1_is_hidden_and_named() {
    let app = app();

    // C7: the idle page.
    let page = get(&app, "/inspect/tls").await;
    let hs = headings(&page);
    let h1: Vec<&(u8, String)> = hs.iter().filter(|h| h.0 == 1).collect();
    assert_eq!(h1.len(), 1, "{hs:?}");
    assert_eq!(h1[0].1, "TLS inspector");
    assert!(
        page.contains(r#"<h1 class="visually-hidden">TLS inspector</h1>"#),
        "{page}"
    );
    let hidden = rules(|s| s == ".visually-hidden");
    assert!(
        hidden.iter().any(|d| d.contains("position: absolute")
            && (d.contains("clip") || d.contains("1px"))
            && !d.contains("display: none")
            && !d.contains("visibility: hidden")),
        "no accessible .visually-hidden rule: {hidden:?}"
    );
    let h2css = rules(|s| s.contains(".inspector-section") && s.ends_with("h2"));
    assert!(
        h2css.iter().any(|d| (d.contains("font-size: 20px")
            || d.contains("font-size: var(--font-size-h2)"))
            && d.contains("font-weight: 650")
            && netray_web::tokens::CSS.contains("--font-size-h2: 20px")),
        "no 20px/650 section h2 rule: {h2css:?}"
    );

    // C20: apply the stream to the page.
    let frags = run(&app, "h=example.com").await;
    let mut result = String::new();
    for (id, html) in &frags {
        if id == "inspect-result" {
            result = html.clone();
        }
    }
    assert!(!result.is_empty(), "no start fragment: {frags:?}");
    for (id, html) in &frags {
        if id.starts_with("inspect-part-") {
            let marker = format!(r#"id="{id}""#);
            assert!(result.contains(&marker), "no placeholder {id}: {result}");
            let start = result.find(&marker).unwrap();
            let open = result[..start].rfind('<').unwrap();
            let end = result[start..]
                .find("</section>")
                .map(|i| start + i + "</section>".len())
                .unwrap_or(result.len());
            result.replace_range(open..end, html);
        }
    }
    let after = page.replacen(
        r#"<div id="inspect-result"><h1 class="visually-hidden">TLS inspector</h1></div>"#,
        &result,
        1,
    );
    assert_ne!(after, page, "idle result region not found");
    let hs = headings(&after);
    assert_eq!(hs.iter().filter(|h| h.0 == 1).count(), 1, "{hs:?}");
    let h2: Vec<&str> = hs
        .iter()
        .filter(|h| h.0 == 2)
        .map(|h| h.1.as_str())
        .collect();
    assert_eq!(h2, ["chain", "handshake"], "{hs:?}");
    assert!(
        after
            .matches(r#"<section class="inspector-section""#)
            .count()
            >= 2,
        "{after}"
    );
    assert_no_skip(&hs);
}
