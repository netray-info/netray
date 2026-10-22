//! V2 spec v2-inspector-shell, requirements 13 and 14 (C4, C14, C15, C16): the DNS inspector shares
//! its run through the API's DNS result cache, and the caller's own address is never shared.
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
    router,
};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _: &'a str,
        _: IpAddr,
        _: RunTrigger,
        _: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn api_state() -> ApiState {
    let config: ApiConfig =
        toml::from_str("[dns]\nper_ip_burst = 200\nglobal_burst = 200").expect("[api.dns] parses");
    ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    })
}

fn shell_over(state: &ApiState) -> Router {
    inspect_router(
        InspectDeps::new(
            Arc::new(InspectJobs::new(state.clone())),
            InspectConfig::default(),
        )
        .with_dns_results(state.dns_results().clone()),
    )
}

fn with_peer(mut req: Request<Body>) -> Request<Body> {
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    req
}

async fn body_of(app: &Router, req: Request<Body>) -> (StatusCode, String) {
    let resp = app.clone().oneshot(with_peer(req)).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
    )
}

/// The (fragment id, html) pairs of a run route's SSE answer.
async fn run(app: &Router, path: &str, form: &str) -> Vec<(String, String)> {
    let req = Request::post(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_owned()))
        .unwrap();
    let (status, text) = body_of(app, req).await;
    assert_eq!(status, 200, "{text}");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<Value>(&data).unwrap_or_default();
        events.push((
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    events
}

fn attr<'a>(html: &'a str, name: &str) -> Option<&'a str> {
    let open = format!(r#"{name}=""#);
    let a = html.find(&open)? + open.len();
    Some(&html[a..a + html[a..].find('"')?])
}

fn is_key(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[tokio::test]
async fn c14_a_finished_dns_inspector_run_is_replayed_by_the_api_under_its_share_key() {
    let state = api_state();
    let shell = shell_over(&state);

    let events = run(&shell, "/inspect/dns/run", "q=example.com+MX").await;

    let share = events
        .iter()
        .rev()
        .find(|e| e.0 == "inspect-share")
        .unwrap_or_else(|| panic!("no inspect-share fragment in {events:?}"));
    let key = attr(&share.1, "data-share-key")
        .unwrap_or_else(|| panic!("no data-share-key in {}", share.1));
    assert!(is_key(key), "{key}");
    let (status, body) = body_of(
        &router(state),
        Request::get(format!("/dns/results/{key}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let replayed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(replayed["mode"], json!("query"));
    assert_eq!(replayed["query"], json!("example.com MX"));
}

#[tokio::test]
async fn c15_an_api_dns_key_opens_in_the_inspector_without_a_band_or_autostart() {
    let state = api_state();
    let api = router(state.clone());
    let req = Request::post("/dns/query")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"domain": "example.com", "record_types": ["A"]}).to_string(),
        ))
        .unwrap();
    let (status, body) = body_of(&api, req).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let key = serde_json::from_str::<Value>(&body).unwrap()["done"]["cache_key"]
        .as_str()
        .unwrap_or_else(|| panic!("done has no cache_key: {body}"))
        .to_owned();

    let (status, page) = body_of(
        &shell_over(&state),
        Request::get(format!("/inspect/dns?r={key}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains(r#"id="ev-dns."#),
        "no DNS evidence blocks in {page}"
    );
    assert!(
        page.contains("The band is not part of an API result."),
        "{page}"
    );
    assert!(!page.contains("data-stream-autostart"), "{page}");
}

struct Recording(Arc<Mutex<Vec<Option<String>>>>);

impl InspectorJobs for Recording {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, req: JobRequest) -> Result<Job, String> {
        self.0.lock().unwrap().push(req.share_key.clone());
        Ok(Job::new(vec![JobPart {
            label: "part".into(),
            output: Box::pin(async { Ok(vec![]) }),
        }]))
    }
}

/// Runs the IP inspector over a recording stub; returns the recorded share keys and fragments.
async fn ip_run(form: &str) -> (Vec<Option<String>>, Vec<(String, String)>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let app = inspect_router(InspectDeps::new(
        Arc::new(Recording(seen.clone())),
        InspectConfig::default(),
    ));
    let events = run(&app, "/inspect/ip/run", form).await;
    let keys = seen.lock().unwrap().clone();
    (keys, events)
}

#[tokio::test]
async fn c16_the_callers_own_address_gets_no_share_key_a_looked_up_one_does() {
    let (keys, events) = ip_run("ip=").await;
    assert_eq!(keys, vec![None]);
    assert!(
        events.iter().all(|e| !e.1.contains("data-share-key")),
        "{events:?}"
    );

    let (keys, events) = ip_run("ip=192.0.2.24").await;
    let key = keys[0]
        .clone()
        .expect("a looked-up address gets a share key");
    assert!(is_key(&key), "{key}");
    let share = events
        .iter()
        .rev()
        .find(|e| e.0 == "inspect-share")
        .unwrap_or_else(|| panic!("no inspect-share fragment in {events:?}"));
    assert_eq!(attr(&share.1, "data-share-key"), Some(key.as_str()));
}
