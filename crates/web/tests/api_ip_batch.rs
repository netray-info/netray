//! API v1 `POST /ip/batch` (spec v2-api, requirements 21 and 25): driven through
//! `netray_web::api::router` over `StubIpLookup`, plus the `[api.ip]` batch-key `validate()` texts.
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::testing::StubIpLookup;
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
        unreachable!("no run is started by the IP routes")
    }
}

fn client() -> IpAddr {
    "192.0.2.10".parse().unwrap()
}

fn setup() -> (Router, ApiState) {
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    });
    (netray_web::api::router(state.clone()), state)
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

async fn batch(app: &Router, addresses: &[String]) -> Reply {
    let mut req = Request::post("/ip/batch")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "addresses": addresses }).to_string()))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(client(), 40000)));
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

fn addrs(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("198.51.100.{i}")).collect()
}

fn cost(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

#[tokio::test]
async fn batch_of_100_answers_in_order_and_spends_one_per_ip_and_100_batch_tokens() {
    let (app, state) = setup();
    let input = addrs(100);
    let r = batch(&app, &input).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    let items = r.json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 100);
    for (item, address) in items.iter().zip(&input) {
        assert_eq!(item["address"], *address);
        assert_eq!(item["result"]["ip"]["addr"], *address);
        assert!(item.get("error").is_none());
    }
    let limiters = state.ip_limiters();
    // default per-IP burst is 10: one token spent leaves nine, not eight and not zero
    assert!(
        limiters
            .per_ip
            .check_key_n(&client(), cost(9))
            .unwrap()
            .is_ok()
    );
    assert!(limiters.per_ip.check_key(&client()).is_err());
    // default batch-address burst is 100: all spent
    assert!(limiters.batch_addresses.check_key(&client()).is_err());
}

#[tokio::test]
async fn batch_of_101_is_refused_with_batch_too_many() {
    let (app, _) = setup();
    let r = batch(&app, &addrs(101)).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json["error"]["code"], "BATCH_TOO_MANY");
    assert_eq!(r.json["error"]["message"], "batch size exceeds limit (100)");
}

#[tokio::test]
async fn one_malformed_address_carries_its_own_error_and_the_rest_results() {
    let (app, _) = setup();
    let input = vec![
        "198.51.100.1".to_string(),
        "198.51.100.2".to_string(),
        "not-an-ip".to_string(),
        "198.51.100.4".to_string(),
    ];
    let r = batch(&app, &input).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    let items = r.json["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    for i in [0, 1, 3] {
        assert_eq!(items[i]["address"], input[i]);
        assert!(items[i].get("result").is_some(), "item {i}");
        assert!(items[i].get("error").is_none(), "item {i}");
    }
    assert_eq!(items[2]["address"], "not-an-ip");
    assert!(items[2].get("result").is_none());
    assert_eq!(items[2]["error"]["code"], "INVALID_INPUT");
    assert_eq!(items[2]["error"]["message"], "invalid IP address");
}

#[tokio::test]
async fn eleven_addresses_with_ten_batch_tokens_left_answer_429_with_retry_after() {
    let (app, state) = setup();
    let first = batch(&app, &addrs(90)).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.json);
    let r = batch(&app, &addrs(11)).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "RATE_LIMITED");
    let retry: u64 = r.headers["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1);
    // the refused batch spent no batch-address token: ten are still there
    let limiters = state.ip_limiters();
    assert!(
        limiters
            .batch_addresses
            .check_key_n(&client(), cost(10))
            .unwrap()
            .is_ok()
    );
}

#[tokio::test]
async fn eleven_one_address_batches_in_a_burst_refuse_the_eleventh_by_the_per_ip_limiter() {
    let (app, state) = setup();
    for i in 1..=10 {
        let r = batch(&app, &[format!("198.51.100.{i}")]).await;
        assert_eq!(r.status, StatusCode::OK, "batch {i}: {}", r.json);
    }
    let r = batch(&app, &["198.51.100.11".to_string()]).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{}", r.json);
    assert_eq!(r.json["error"]["code"], "RATE_LIMITED");
    assert!(r.headers.contains_key("retry-after"));
    // the batch-address budget (100) was not the reason: ten of 100 were spent
    assert!(
        state
            .ip_limiters()
            .batch_addresses
            .check_key_n(&client(), cost(90))
            .unwrap()
            .is_ok()
    );
}

fn api_config(ip_table: &str) -> ApiConfig {
    toml::from_str(&format!("[ip]\n{ip_table}")).expect("[api.ip] keys parse")
}

#[test]
fn batch_keys_validate_with_their_names() {
    let burst_below_max = api_config("batch_addresses_burst = 50\nbatch_max_size = 100")
        .validate()
        .unwrap_err();
    assert!(
        burst_below_max.starts_with("api.ip.batch_addresses_burst"),
        "{burst_below_max}"
    );
    let over_cap = api_config("batch_max_size = 101").validate().unwrap_err();
    assert!(over_cap.starts_with("api.ip.batch_max_size"), "{over_cap}");
    assert_eq!(api_config("batch_max_size = 100").validate(), Ok(()));
}

#[test]
fn per_ip_burst_below_the_diff_cost_is_refused() {
    let one = api_config("per_ip_burst = 1").validate().unwrap_err();
    assert!(one.starts_with("api.ip.per_ip_burst"), "{one}");
    assert_eq!(api_config("per_ip_burst = 2").validate(), Ok(()));
}
