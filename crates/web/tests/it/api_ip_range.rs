//! API v1 ip functions (spec v2-api, requirements 22-24): `/ip/range/{*cidr}`, `POST /ip/diff`
//! and `/asn/{n}` driven through `netray_web::api::router` over `StubIpLookup`.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::testing::StubIpLookup;
use netray_ip::{
    IpLookup,
    lookup::{AsnLookupResponse, DiffRequest, DiffResponse, RangeResponse},
};
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
        unreachable!("no run is started by an ip test")
    }
}

fn app(config: ApiConfig) -> Router {
    let parts = ApiParts {
        ip: Some(Arc::new(StubIpLookup::new()) as Arc<dyn IpLookup>),
        ..Default::default()
    };
    netray_web::api::router(ApiState::new(config, Arc::new(NoRuns)).with_parts(parts))
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    send(app, Request::get(path).body(Body::empty()).unwrap()).await
}

async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::post(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

fn stub_range() -> Value {
    json!({
        "cidr": "192.0.2.0/24",
        "asn": 64496,
        "org": "Example Cloud GmbH",
        "network_type": "cloud",
        "is_cloud": true,
        "is_vpn": false,
        "is_datacenter": true,
        "is_spamhaus": false,
        "is_c2": false,
    })
}

#[test]
fn the_phase_4_types_are_named_where_the_interface_puts_them() {
    fn names<T>() {}
    names::<RangeResponse>();
    names::<AsnLookupResponse>();
    names::<DiffRequest>();
    names::<DiffResponse>();
}

#[tokio::test]
async fn range_answers_the_stub_for_a_slash_and_for_an_encoded_slash() {
    let app = app(ApiConfig::default());
    let (s1, plain) = get(&app, "/ip/range/192.0.2.0/24").await;
    let (s2, encoded) = get(&app, "/ip/range/192.0.2.0%2F24").await;
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    assert_eq!(plain, stub_range());
    assert_eq!(encoded, stub_range());
}

#[tokio::test]
async fn range_refuses_a_special_prefix_and_a_malformed_cidr() {
    let app = app(ApiConfig::default());
    for path in [
        "/ip/range/10.0.0.0/8",
        "/ip/range/not-a-cidr",
        "/ip/range/192.0.2.0/99",
    ] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(body["error"]["code"], "INVALID_INPUT", "{path}");
    }
}

#[tokio::test]
async fn range_without_a_prefix_is_invalid_input_not_a_lookup_of_range() {
    let app = app(ApiConfig::default());
    let (status, body) = get(&app, "/ip/range").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "INVALID_INPUT");
}

#[tokio::test]
async fn diff_answers_a_typed_response_with_todays_fields() {
    let app = app(ApiConfig::default());
    let (status, body) = post(
        &app,
        "/ip/diff",
        json!({"a": "198.51.100.1", "b": "198.51.100.2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["ip", "location", "network", "tcp", "user_agent"]);
    for part in ["ip", "location", "network", "tcp", "user_agent"] {
        let mut k: Vec<&str> = body[part]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        k.sort();
        assert_eq!(k, ["a", "b", "equal"], "{part}");
    }
    assert_eq!(body["ip"]["a"]["addr"], "198.51.100.1");
    assert_eq!(body["ip"]["b"]["addr"], "198.51.100.2");
    assert_eq!(body["ip"]["equal"], false);
    assert_eq!(body["location"]["equal"], true);
    assert_eq!(body["network"]["equal"], true);
    assert_eq!(body["tcp"]["equal"], true);
    assert_eq!(body["user_agent"]["equal"], true);
}

#[tokio::test]
async fn diff_charges_two_tokens_against_the_ip_per_ip_limiter() {
    let mut config = ApiConfig::default();
    config.ip.per_ip_burst = 3;
    let app = app(config);
    let body = json!({"a": "198.51.100.1", "b": "198.51.100.2"});
    let (first, _) = post(&app, "/ip/diff", body.clone()).await;
    let (second, refused) = post(&app, "/ip/diff", body).await;
    assert_eq!(first, StatusCode::OK);
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refused["error"]["code"], "RATE_LIMITED");
}

#[tokio::test]
async fn asn_answers_the_stub_echo_and_refuses_a_non_number() {
    let app = app(ApiConfig::default());
    let (status, body) = get(&app, "/asn/64496").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["asn"], 64496);
    assert_eq!(body["category"], "hosting");
    assert_eq!(body["network_role"], "stub");
    assert_eq!(body["is_anycast"], false);
    assert!(body["org"].is_null());
    let (status, body) = get(&app, "/asn/abc").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "INVALID_INPUT");
}
