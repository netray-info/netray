use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use std::net::SocketAddr;
use tokio::net::TcpListener;

use netray_ip::Config;
use serde_json::{Value, json};

fn offline_config() -> Config {
    let mut config = Config::load(None).expect("default config");
    config.batch.enabled = true;
    config
}

async fn spawn(config: &Config) -> SocketAddr {
    let app = netray_ip::build_app(config).await.app;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    });
    addr
}

struct Reply {
    status: u16,
    content_type: String,
    retry_after: Option<String>,
    body: String,
}

async fn send(addr: SocketAddr, method: &str, path: &str, ua: &str, accept: &str, body: &str) -> Reply {
    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new()).build_http();
    let req = Request::builder()
        .method(method)
        .uri(format!("http://{addr}{path}"))
        .header("user-agent", ua)
        .header("accept", accept)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = client.request(req).await.unwrap();
    let h = |n: &str| resp.headers().get(n).map(|v| v.to_str().unwrap().to_string());
    let status = resp.status().as_u16();
    let content_type = h("content-type").unwrap_or_default();
    let retry_after = h("retry-after");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        content_type,
        retry_after,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

const CURL: &str = "curl/8.5.0";
const BROWSER: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

#[tokio::test]
async fn curl_root_answers_the_caller_as_plain_text() {
    let addr = spawn(&offline_config()).await;
    let r = send(addr, "GET", "/", CURL, "*/*", "").await;
    assert_eq!(r.status, 200);
    assert!(
        r.content_type.starts_with("text/plain"),
        "content-type: {}",
        r.content_type
    );
    assert_eq!(r.body, "127.0.0.1\n");
}

#[tokio::test]
async fn browser_root_is_not_plain_text() {
    let addr = spawn(&offline_config()).await;
    let r = send(addr, "GET", "/", BROWSER, "text/html,application/xhtml+xml", "").await;
    assert!(
        !r.content_type.starts_with("text/plain"),
        "content-type: {}",
        r.content_type
    );
}

#[tokio::test]
async fn own_batch_over_the_burst_is_429_with_retry_after_1() {
    let addr = spawn(&offline_config()).await;
    let ips: Vec<String> = (1..=11).map(|i| format!("8.8.8.{i}")).collect();
    let r = send(
        addr,
        "POST",
        "/batch?dns=false",
        CURL,
        "*/*",
        &serde_json::to_string(&ips).unwrap(),
    )
    .await;
    assert_eq!(r.status, 429);
    assert_eq!(r.retry_after.as_deref(), Some("1"));
}

fn ifconfig_8888() -> Value {
    json!({
        "ip": {"addr": "8.8.8.8", "hostname": null, "version": "4"},
        "location": {
            "accuracy_radius_km": null, "city": null, "continent": null, "continent_code": null,
            "country": null, "country_iso": null, "is_eu": null, "latitude": null, "longitude": null,
            "postal_code": null, "region": null, "region_code": null, "registered_country": null,
            "registered_country_iso": null, "timezone": null
        },
        "network": {
            "asn": null, "asn_category": null, "asn_registered": null, "bot": null, "cloud": null,
            "infra_type": "residential", "is_anycast": false, "is_bot": false, "is_c2": false,
            "is_cins": false, "is_datacenter": false, "is_internal": false, "is_spamhaus": false,
            "is_tor": false, "is_vpn": false, "network_role": null, "org": null, "prefix": null,
            "type": "residential", "vpn": null
        },
        "tcp": null,
        "user_agent": null
    })
}

/// Pins captured from the code before the v2-api Phase 4 refactor; they must pass before and after.
#[tokio::test]
async fn own_json_batch_and_range_answers_are_unchanged() {
    let addr = spawn(&offline_config()).await;

    let r = send(addr, "GET", "/json?ip=8.8.8.8&dns=false", CURL, "*/*", "").await;
    assert_eq!(r.status, 200);
    assert_eq!(serde_json::from_str::<Value>(&r.body).unwrap(), ifconfig_8888());

    let r = send(
        addr,
        "POST",
        "/batch?dns=false",
        CURL,
        "*/*",
        r#"["8.8.8.8","10.0.0.1","nope"]"#,
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        json!([
            ifconfig_8888(),
            {"error": {"code": "INVALID_IP", "message": "private/loopback IP not allowed"}, "index": 1},
            {"error": {"code": "INVALID_IP", "message": "invalid IP address"}, "index": 2}
        ])
    );

    let r = send(addr, "GET", "/range?cidr=8.8.8.0/24", CURL, "*/*", "").await;
    assert_eq!(r.status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        json!({
            "cidr": "8.8.8.0/24", "asn": null, "org": null, "network_type": "residential",
            "is_cloud": false, "is_vpn": false, "is_datacenter": false, "is_spamhaus": false,
            "is_c2": false
        })
    );
}
