//! V2 spec v2-ip-ui, requirement 15 (C8, C21): without JavaScript the caller's card renders from
//! the GET alone; the copy controls stay hidden, never broken. Driven through `inspect_router`
//! over the `StubIpLookup`.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_ip::IpLookup;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the inspector")
    }
}

const PROXY: &str = "203.0.113.9";

fn app() -> Router {
    let config: ApiConfig = toml::from_str("[ip]\ninternal_mode = true\n").unwrap();
    let extractor = Arc::new(IpExtractor::new(&[PROXY.to_string()]));
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(extractor.clone())
        .with_parts(ApiParts {
            ip: Some(Arc::new(StubIpLookup::default()) as Arc<dyn IpLookup>),
            ..Default::default()
        });
    inspect_router(
        InspectDeps::new(Arc::new(InspectJobs::new(state)), InspectConfig::default())
            .with_ip_extractor(extractor),
    )
}

/// Every `<button ...>` opening tag in the markup.
fn button_tags(html: &str) -> Vec<&str> {
    html.split("<button")
        .skip(1)
        .map(|s| s.split('>').next().unwrap())
        .collect()
}

/// Whether the tag text carries the attribute `name` as a whole word.
fn has_attr(tag: &str, name: &str) -> bool {
    tag.split(|c: char| c.is_whitespace() || c == '=' || c == '"')
        .any(|w| w == name)
}

#[tokio::test]
async fn ip_get_renders_the_callers_card_without_javascript_and_keeps_copy_hidden() {
    let mut req = Request::get("/inspect/ip")
        .header(header::USER_AGENT, "nojs-test/1.0")
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = "192.0.2.10:54321".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes).into_owned();

    let card_at = html
        .find(r#"id="ev-ip.lookup""#)
        .unwrap_or_else(|| panic!("no lookup card in the GET body: {html}"));
    let card = &html[card_at..];
    assert!(card.contains("192.0.2.10"), "{html}");
    assert!(card.contains("stub.example.com"), "{html}");

    assert!(
        !html.contains("data-stream-autostart"),
        "the form must not autostart: {html}"
    );

    let copies: Vec<&str> = button_tags(&html)
        .into_iter()
        .filter(|t| t.contains("inspector-copy"))
        .collect();
    assert!(!copies.is_empty(), "no copy button in the GET body: {html}");
    for tag in copies {
        assert!(has_attr(tag, "hidden"), "copy button not hidden: <button{tag}>");
    }
}
