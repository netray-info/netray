//! V2 spec v2-ip-ui, requirement 14 (C6): an inspector part whose addresses carry IP badges ends
//! with the GeoIP attribution paragraph, inside the part's section; a run without badges has none.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::evidence::{Badge, BadgeTone};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

const A: &str = "192.0.2.24";

/// A stub whose part holds one address block; `enrich` answers badges only when `badges`.
struct Stub {
    badges: bool,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let block = Block {
            id: BlockId::parse("http.server").unwrap(),
            title: "Server".into(),
            shape: Shape::Fields(vec![("address".into(), Value::Addr(A.parse().unwrap()))]),
        };
        let part = JobPart {
            label: "Part".into(),
            output: Box::pin(async move { Ok(vec![block]) }),
        };
        Ok(Job::new(vec![part]))
    }
    fn enrich(&self, addrs: Vec<IpAddr>) -> BoxFuture<'static, Vec<(IpAddr, Vec<Badge>)>> {
        if !self.badges {
            return Box::pin(async { Vec::new() });
        }
        Box::pin(async move {
            addrs
                .into_iter()
                .map(|a| {
                    let tags = vec![Badge {
                        label: "Example Org".into(),
                        tone: BadgeTone::Muted,
                    }];
                    (a, tags)
                })
                .collect()
        })
    }
}

fn app(badges: bool) -> Router {
    inspect_router(InspectDeps::new(
        Arc::new(Stub { badges }),
        InspectConfig::default(),
    ))
}

/// (fragment id, fragment html) per event.
async fn run(app: &Router) -> Vec<(String, String)> {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/inspect/http/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("url=https%3A%2F%2Fexample.com%2F"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut out = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        out.push((
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    out
}

#[tokio::test]
async fn c6_a_part_with_badges_ends_with_the_geoip_attribution_inside_its_section() {
    let events = run(&app(true)).await;

    let part = events
        .iter()
        .find(|e| e.0.starts_with("inspect-part") && e.1.contains("ip-badge"))
        .unwrap_or_else(|| panic!("no part with badges in {events:?}"))
        .1
        .as_str();
    let attr = part
        .find(r#"<p class="geoip-attribution""#)
        .unwrap_or_else(|| panic!("no attribution in {part}"));
    assert!(attr > part.rfind("ip-badge").unwrap(), "{part}");
    let tail = &part[attr..];
    let close = tail.find("</p>").unwrap_or_else(|| panic!("{part}"));
    assert!(
        tail[..close].contains(r#"<a href="https://www.maxmind.com""#),
        "{part}"
    );
    assert_eq!(tail[close + "</p>".len()..].trim(), "</section>", "{part}");
    assert_eq!(part.matches("geoip-attribution").count(), 1, "{part}");
}

#[tokio::test]
async fn c6_a_run_without_badges_has_no_geoip_attribution() {
    let events = run(&app(false)).await;

    assert!(events.iter().any(|e| e.1.contains(A)), "{events:?}");
    assert!(
        !events.iter().any(|e| e.1.contains("geoip-attribution")),
        "{events:?}"
    );
}
