//! API v1 `POST /email/inspect` limits (spec v2-api, Phase 6, requirements 7, 29 and 31): the
//! `[api.email]` slots and per-IP budget, the shared limiter `Arc`, the off answer and the config
//! keys, driven through `netray_web::api::router` over the email stub inspector.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_email::EmailInspectRequest;
use netray_email::testing::{example_dns, stub_inspector};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
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
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the email route")
    }
}

const PROXY: &str = "203.0.113.9";

fn api_config(email_table: &str) -> ApiConfig {
    toml::from_str(&format!("[email]\n{email_table}")).expect("[api.email] keys parse")
}

fn state_with(config: ApiConfig, parts: ApiParts) -> (Router, ApiState) {
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(Arc::new(IpExtractor::new(&[PROXY.to_string()])))
        .with_parts(parts);
    (netray_web::api::router(state.clone()), state)
}

fn setup(config: ApiConfig) -> (Router, ApiState) {
    state_with(
        config,
        ApiParts {
            email: Some(Arc::new(stub_inspector(example_dns()))),
            ..Default::default()
        },
    )
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

/// Posts as the trusted proxy forwarding for `caller`, asking for JSON.
async fn inspect(app: &Router, caller: &str) -> Reply {
    let mut req = Request::post("/email/inspect")
        .header("content-type", "application/json")
        .header("x-forwarded-for", caller)
        .body(Body::from(json!({ "domain": "example.com" }).to_string()))
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

fn assert_rate_limited(r: &Reply) {
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "RATE_LIMITED");
    let retry: u64 = r.headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1);
}

/// C4: one slot, held by a job started through the shared parts; the route is refused until the
/// job is dropped.
#[tokio::test]
async fn a_held_slot_refuses_the_route_with_503_until_dropped() {
    let (app, state) = setup(api_config("max_concurrent_inspections = 1"));
    let req = EmailInspectRequest {
        domain: "example.com".to_string(),
        dkim_selectors: Vec::new(),
    };
    let held = state
        .email_inspect()
        .unwrap()
        .inspect(
            &req,
            "192.0.2.77".parse().unwrap(),
            state.email_limiter(),
            state.email_slots(),
        )
        .unwrap();

    let r = inspect(&app, "198.51.100.1").await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "UNAVAILABLE");
    assert!(
        r.json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("slot"),
        "{}",
        r.json
    );

    drop(held);
    let r = inspect(&app, "198.51.100.2").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
}

/// `per_ip_burst = 1`: the second post from one caller is refused with Retry-After.
#[tokio::test]
async fn a_spent_per_ip_burst_refuses_the_second_post_with_retry_after() {
    let (app, _) = setup(api_config("per_ip_burst = 1"));
    let first = inspect(&app, "198.51.100.1").await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.json);
    let second = inspect(&app, "198.51.100.1").await;
    assert_rate_limited(&second);
}

/// C3: a second holder of the limiter's Arc spends the caller's budget.
#[tokio::test]
async fn a_second_holder_of_the_email_limiter_spends_the_callers_budget() {
    let (app, state) = setup(api_config("per_ip_burst = 1"));
    let caller: IpAddr = "192.0.2.10".parse().unwrap();
    let other = Arc::clone(state.email_limiter());
    other
        .check(caller)
        .expect("the second holder spends the whole per-IP burst");
    let r = inspect(&app, "192.0.2.10").await;
    assert_rate_limited(&r);
}

/// No email part: 503 with the off message.
#[tokio::test]
async fn an_absent_email_part_answers_503_with_the_off_message() {
    let (app, _) = state_with(ApiConfig::default(), ApiParts::default());
    let r = inspect(&app, "198.51.100.1").await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "UNAVAILABLE");
    assert_eq!(
        r.json["error"]["message"],
        "the email inspector is off: no email module was built"
    );
}

/// `[api.email]`: each zero is refused under its own name.
#[test]
fn zero_email_limits_are_refused_with_their_names() {
    for key in [
        "per_ip_per_minute",
        "per_ip_burst",
        "max_concurrent_inspections",
    ] {
        let err = api_config(&format!("{key} = 0")).validate().unwrap_err();
        assert!(err.starts_with(&format!("api.email.{key}")), "{key}: {err}");
    }
    assert_eq!(api_config("per_ip_burst = 1").validate(), Ok(()));
}

/// `[api.email]` defaults reach `/meta` `limits.email`.
#[tokio::test]
async fn meta_reports_the_email_limit_defaults() {
    let (app, _) = setup(ApiConfig::default());
    let resp = app
        .oneshot(Request::get("/meta").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body["limits"]["email"],
        json!({
            "per_ip_per_minute": 10,
            "per_ip_burst": 10,
            "max_concurrent_inspections": 16
        })
    );
}
