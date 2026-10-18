//! API v1 `POST /dns/query` refusals and limits (spec v2-api, Phase 7, requirements 7, 30, 31):
//! the policy refusals, the `[api.dns]` per-IP, per-target and stream budgets, the shared limiter
//! `Arc`, the off answer and the config keys, driven through `netray_web::api::router` over the
//! DNS stub jobs.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_dns::DnsJobs;
use netray_dns::config::Config;
use netray_dns::testing::{stub_jobs, stub_jobs_with};
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
        unreachable!("no run is started by the DNS route")
    }
}

const PROXY: &str = "203.0.113.9";

fn api_config(dns_table: &str) -> ApiConfig {
    toml::from_str(&format!("[dns]\n{dns_table}")).expect("[api.dns] keys parse")
}

fn state_with(config: ApiConfig, parts: ApiParts) -> (Router, ApiState) {
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(Arc::new(IpExtractor::new(&[PROXY.to_string()])))
        .with_parts(parts);
    (netray_web::api::router(state.clone()), state)
}

fn setup_with(config: ApiConfig, jobs: DnsJobs) -> (Router, ApiState) {
    state_with(
        config,
        ApiParts {
            dns: Some(jobs),
            ..Default::default()
        },
    )
}

fn setup(config: ApiConfig) -> (Router, ApiState) {
    setup_with(config, stub_jobs())
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

/// Posts `body` as the trusted proxy forwarding for `caller`, asking for JSON.
async fn post(app: &Router, caller: &str, body: Value) -> Reply {
    let mut req = Request::post("/dns/query")
        .header("content-type", "application/json")
        .header("x-forwarded-for", caller)
        .body(Body::from(body.to_string()))
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

async fn query(app: &Router, caller: &str) -> Reply {
    post(
        app,
        caller,
        json!({ "domain": "example.com", "record_types": ["A"] }),
    )
    .await
}

fn assert_rate_limited(r: &Reply) {
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "RATE_LIMITED");
    let retry: u64 = r.headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1);
}

/// C6: an invalid record type and a server address while arbitrary servers are off are 400.
#[tokio::test]
async fn policy_refusals_answer_400_invalid_input() {
    let (app, _) = setup(ApiConfig::default());
    for body in [
        json!({ "domain": "example.com", "record_types": ["NOPE"] }),
        json!({ "domain": "example.com", "record_types": ["A"], "servers": ["192.0.2.1"] }),
    ] {
        let r = post(&app, "198.51.100.1", body.clone()).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{body}: {}", r.json);
        assert_eq!(r.json["error"]["code"], "INVALID_INPUT", "{body}");
        assert!(r.json["error"]["message"].is_string(), "{body}");
    }
}

/// C6: with arbitrary servers allowed, a private server address is 403.
#[tokio::test]
async fn a_blocked_server_address_answers_403_target_blocked() {
    let mut config = Config::load(None).expect("default config loads");
    config.dns.allow_arbitrary_servers = true;
    let (app, _) = setup_with(ApiConfig::default(), stub_jobs_with(config));
    let r = post(
        &app,
        "198.51.100.1",
        json!({ "domain": "example.com", "record_types": ["A"], "servers": ["10.0.0.1"] }),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "TARGET_BLOCKED");
}

/// C7: `per_target_burst = 1`: a second caller querying the same default target is refused.
#[tokio::test]
async fn a_spent_per_target_burst_refuses_another_caller_with_retry_after() {
    let (app, _) = setup(api_config("per_target_burst = 1"));
    let first = query(&app, "198.51.100.1").await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.json);
    let second = query(&app, "198.51.100.2").await;
    assert_rate_limited(&second);
}

/// C7 / requirement 31: a second holder of the limiter's Arc spends the caller's budget.
#[tokio::test]
async fn a_second_holder_of_the_dns_limiter_spends_the_callers_budget() {
    let (app, state) = setup(ApiConfig::default());
    let caller: IpAddr = "192.0.2.10".parse().unwrap();
    let other = Arc::clone(state.dns_limiter());
    drop(
        other
            .check_query_cost(caller, &[], 64, 1)
            .expect("the second holder spends the whole per-IP burst"),
    );
    let r = query(&app, "192.0.2.10").await;
    assert_rate_limited(&r);
}

/// C7: a held stream guard at `per_ip_max_streams = 1` refuses the caller with `Retry-After: 1`.
#[tokio::test]
async fn a_held_stream_guard_refuses_the_caller_with_retry_after_1() {
    let (app, state) = setup(api_config("per_ip_max_streams = 1"));
    let caller: IpAddr = "192.0.2.20".parse().unwrap();
    let guard = state
        .dns_limiter()
        .check_query_cost(caller, &[], 1, 1)
        .expect("the first stream is admitted");
    let r = query(&app, "192.0.2.20").await;
    assert_rate_limited(&r);
    assert_eq!(r.headers["retry-after"], "1");
    drop(guard);
}

/// An unknown body field is ignored (prism's behaviour).
#[tokio::test]
async fn an_unknown_body_field_is_ignored() {
    let (app, _) = setup(ApiConfig::default());
    let r = post(
        &app,
        "198.51.100.1",
        json!({ "domain": "example.com", "record_types": ["A"], "bogus": 1 }),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
}

/// No DNS part: 503 with the off message.
#[tokio::test]
async fn an_absent_dns_part_answers_503_with_the_off_message() {
    let (app, _) = state_with(ApiConfig::default(), ApiParts::default());
    let r = query(&app, "198.51.100.1").await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "UNAVAILABLE");
    assert_eq!(
        r.json["error"]["message"],
        "the DNS inspector is off: no DNS module was built"
    );
}

/// `[api.dns]`: each zero is refused under its own name, and the two bursts below 18 too.
#[test]
fn dns_limits_are_validated_with_their_names() {
    for key in [
        "per_ip_per_minute",
        "per_ip_burst",
        "per_target_per_minute",
        "per_target_burst",
        "global_per_minute",
        "global_burst",
        "per_ip_max_streams",
    ] {
        let err = api_config(&format!("{key} = 0")).validate().unwrap_err();
        assert_eq!(
            err,
            format!("api.dns.{key} must be greater than 0"),
            "{key}"
        );
    }
    for key in ["per_ip_burst", "global_burst"] {
        let err = api_config(&format!("{key} = 17")).validate().unwrap_err();
        assert!(
            err.starts_with(&format!("api.dns.{key} (17)")),
            "{key}: {err}"
        );
        assert_eq!(api_config(&format!("{key} = 18")).validate(), Ok(()));
    }
    assert_eq!(api_config("per_target_burst = 1").validate(), Ok(()));
}

/// `[api.dns]` defaults reach `/meta` `limits.dns`.
#[tokio::test]
async fn meta_reports_the_dns_limit_defaults() {
    let (app, _) = setup(ApiConfig::default());
    let resp = app
        .oneshot(Request::get("/meta").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body["limits"]["dns"],
        json!({
            "per_ip_per_minute": 120,
            "per_ip_burst": 64,
            "per_target_per_minute": 60,
            "per_target_burst": 20,
            "global_per_minute": 1000,
            "global_burst": 50,
            "per_ip_max_streams": 10
        })
    );
}
