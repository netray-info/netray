//! API v1 `POST /tls/inspect` limits (spec v2-api, Phase 5, requirements 7 and 31): the `[api.tls]`
//! per-target budget, the shared `Arc` of per-IP limiters, and the `[api.tls]` config keys, driven
//! through `netray_web::api::router` over `StubTlsBackend`.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_tls::testing::{StubTlsBackend, stub_state};
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
        unreachable!("no run is started by the TLS routes")
    }
}

const PROXY: &str = "203.0.113.9";

fn api_config(tls_table: &str) -> ApiConfig {
    toml::from_str(&format!("[tls]\n{tls_table}")).expect("[api.tls] keys parse")
}

fn setup(config: ApiConfig) -> (Router, ApiState) {
    let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
    let state = ApiState::new(config, Arc::new(NoRuns))
        .with_ip_extractor(Arc::new(IpExtractor::new(&[PROXY.to_string()])))
        .with_parts(ApiParts {
            tls: Some(inspector),
            ..Default::default()
        });
    (netray_web::api::router(state.clone()), state)
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

/// Posts as the trusted proxy forwarding for `caller`.
async fn inspect(app: &Router, caller: &str, host: &str) -> Reply {
    let mut req = Request::post("/tls/inspect")
        .header("content-type", "application/json")
        .header("x-forwarded-for", caller)
        .body(Body::from(json!({ "host": host }).to_string()))
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

/// C10: the per-target budget for example.com is spent by two callers; a third is refused.
#[tokio::test]
async fn exhausted_per_target_budget_refuses_a_fresh_address_with_retry_after() {
    let (app, _) = setup(api_config("per_target_burst = 2"));
    for caller in ["198.51.100.1", "198.51.100.2"] {
        let r = inspect(&app, caller, "example.com").await;
        assert_eq!(r.status, StatusCode::OK, "{caller}: {}", r.json);
    }
    let r = inspect(&app, "198.51.100.3", "example.com").await;
    assert_rate_limited(&r);
}

/// C11, C3: a second holder of the limiters' Arc spends the caller's per-IP budget.
#[tokio::test]
async fn a_second_holder_of_the_tls_limiters_spends_the_callers_budget() {
    let (app, state) = setup(ApiConfig::default());
    let caller: IpAddr = "192.0.2.10".parse().unwrap();
    let other = Arc::clone(state.tls_limiters());
    // default per-IP burst is 10
    other
        .check_cost(caller, "other.example.com", 10)
        .expect("the second holder spends the whole per-IP burst");
    let r = inspect(&app, "192.0.2.10", "example.com").await;
    assert_rate_limited(&r);
}

/// `[api.tls]`: each zero is refused under its own name.
#[test]
fn zero_tls_limits_are_refused_with_their_names() {
    for key in [
        "per_ip_per_minute",
        "per_ip_burst",
        "per_target_per_minute",
        "per_target_burst",
    ] {
        let err = api_config(&format!("{key} = 0")).validate().unwrap_err();
        assert!(err.starts_with(&format!("api.tls.{key}")), "{key}: {err}");
    }
    assert_eq!(api_config("per_target_burst = 1").validate(), Ok(()));
}

/// `[api.tls]` defaults reach `/meta` `limits.tls`.
#[tokio::test]
async fn meta_reports_the_tls_limit_defaults() {
    let (app, _) = setup(ApiConfig::default());
    let resp = app
        .oneshot(Request::get("/meta").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body["limits"]["tls"],
        json!({
            "per_ip_per_minute": 30,
            "per_ip_burst": 10,
            "per_target_per_minute": 60,
            "per_target_burst": 20
        })
    );
}
