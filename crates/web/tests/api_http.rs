//! API v1 `POST /http/inspect` (spec v2-api Phase 5, C13 and the `[api.http]` half of C1/C3):
//! driven through `netray_web::api::router` over `StubHttpInspect`, paths relative to the mount.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use netray_http::testing::StubHttpInspect;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, openapi, router,
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
    ) -> futures::future::BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

const CALLER: &str = "198.51.100.7";

fn stub_parts() -> ApiParts {
    ApiParts {
        http: Some(Arc::new(StubHttpInspect::new())),
        ..Default::default()
    }
}

fn state(config: ApiConfig, parts: ApiParts) -> ApiState {
    ApiState::new(config, Arc::new(NoRuns)).with_parts(parts)
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    json: Value,
}

async fn send(app: &Router, mut req: Request<Body>) -> Reply {
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(CALLER.parse().unwrap(), 40000)));
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("body is not JSON: {e}: {bytes:?}"));
    Reply {
        status,
        headers,
        json,
    }
}

async fn inspect(app: &Router, body: Value) -> Reply {
    let req = Request::post("/http/inspect")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

fn validate_inspect_response(body: &Value) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let schema = &doc["paths"]["/api/v1/http/inspect"]["post"]["responses"]["200"]["content"]["application/json"]
        ["schema"];
    assert!(schema.is_object(), "200 schema not documented: {schema}");
    let mut root = schema.clone();
    root.as_object_mut()
        .unwrap()
        .insert("components".into(), doc["components"].clone());
    let validator = jsonschema::validator_for(&root).expect("200 schema compiles");
    let errors: Vec<String> = validator.iter_errors(body).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:?}\nbody: {body}");
}

#[tokio::test]
async fn inspect_answers_the_spectra_response_refuses_blocked_and_plain_http() {
    let app = router(state(ApiConfig::default(), stub_parts()));

    let ok = inspect(&app, json!({"url": "https://example.com/"})).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.json["url"], "https://example.com/");
    validate_inspect_response(&ok.json);

    let blocked = inspect(&app, json!({"url": "https://127.0.0.1/"})).await;
    assert_eq!(blocked.status, StatusCode::FORBIDDEN);
    assert_eq!(blocked.json["error"]["code"], "TARGET_BLOCKED");

    let plain = inspect(&app, json!({"url": "http://example.com/"})).await;
    assert_eq!(plain.status, StatusCode::BAD_REQUEST);
    assert_eq!(plain.json["error"]["code"], "INVALID_INPUT");
}

#[tokio::test]
async fn unknown_field_is_invalid_input() {
    let app = router(state(ApiConfig::default(), stub_parts()));
    let reply = inspect(&app, json!({"url": "https://example.com/", "x": 1})).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.json["error"]["code"], "INVALID_INPUT");
}

#[tokio::test]
async fn no_http_part_answers_unavailable() {
    let app = router(state(ApiConfig::default(), ApiParts::default()));
    let reply = inspect(&app, json!({"url": "https://example.com/"})).await;
    assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(reply.json["error"]["code"], "UNAVAILABLE");
}

#[tokio::test]
async fn spent_per_ip_burst_answers_rate_limited_with_retry_after() {
    let mut config = ApiConfig::default();
    config.http.per_ip_burst = 2;
    let app = router(state(config, stub_parts()));
    let body = json!({"url": "https://example.com/"});
    for _ in 0..2 {
        assert_eq!(inspect(&app, body.clone()).await.status, StatusCode::OK);
    }
    let refused = inspect(&app, body).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refused.json["error"]["code"], "RATE_LIMITED");
    assert!(refused.headers.contains_key("retry-after"));
}

#[tokio::test]
async fn a_second_holder_of_the_limiters_spends_the_callers_budget() {
    let mut config = ApiConfig::default();
    config.http.per_ip_burst = 2;
    let st = state(config, stub_parts());
    let app = router(st.clone());
    let other = Arc::clone(st.http_limiters());
    let caller: IpAddr = CALLER.parse().unwrap();
    other.check(caller, "one.example.com").unwrap();
    other.check(caller, "two.example.com").unwrap();
    let refused = inspect(&app, json!({"url": "https://example.com/"})).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refused.json["error"]["code"], "RATE_LIMITED");
    assert!(refused.headers.contains_key("retry-after"));
}

#[test]
fn validate_refuses_each_zero_http_key_and_defaults_are_10_5_30_10() {
    let defaults = ApiConfig::default().http;
    assert_eq!(
        (
            defaults.per_ip_per_minute,
            defaults.per_ip_burst,
            defaults.per_target_per_minute,
            defaults.per_target_burst
        ),
        (10, 5, 30, 10)
    );
    ApiConfig::default().validate().unwrap();
    for key in [
        "per_ip_per_minute",
        "per_ip_burst",
        "per_target_per_minute",
        "per_target_burst",
    ] {
        let mut config = ApiConfig::default();
        match key {
            "per_ip_per_minute" => config.http.per_ip_per_minute = 0,
            "per_ip_burst" => config.http.per_ip_burst = 0,
            "per_target_per_minute" => config.http.per_target_per_minute = 0,
            _ => config.http.per_target_burst = 0,
        }
        let err = config.validate().unwrap_err();
        assert!(err.starts_with(&format!("api.http.{key}")), "{key}: {err}");
    }
}

#[tokio::test]
async fn meta_shows_the_http_limits() {
    let app = router(state(ApiConfig::default(), stub_parts()));
    let req = Request::get("/meta").body(Body::empty()).unwrap();
    let reply = send(&app, req).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json["limits"]["http"],
        json!({
            "per_ip_per_minute": 10,
            "per_ip_burst": 5,
            "per_target_per_minute": 30,
            "per_target_burst": 10
        })
    );
}
