//! API v1 ip lookups (spec v2-api, Phase 4): `GET /ip`, `GET /ip/{address}` and `GET /asn/{n}`
//! driven through `netray_web::api::router` over `StubIpLookup`, paths relative to the mount.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_ip::IpLookup;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiIpConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
    openapi, router,
};
use serde_json::Value;
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> futures::future::BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

const PROXY: &str = "203.0.113.9";

fn app(internal_mode: bool, parts: ApiParts) -> Router {
    let config = ApiConfig {
        ip: ApiIpConfig {
            internal_mode,
            ..Default::default()
        },
        ..Default::default()
    };
    router(
        ApiState::new(config, Arc::new(NoRuns))
            .with_ip_extractor(Arc::new(IpExtractor::new(&[PROXY.to_string()])))
            .with_parts(parts),
    )
}

fn stub_parts() -> ApiParts {
    ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    }
}

/// Sends one GET as the trusted proxy forwarding for 192.0.2.10; returns status and parsed body.
async fn get(app: Router, uri: &str) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .uri(uri)
        .header("x-forwarded-for", "192.0.2.10")
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = format!("{PROXY}:0").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("{uri}: body is not JSON: {e}: {bytes:?}"));
    (status, body)
}

fn stub_json(ip: &str) -> Value {
    let stub = StubIpLookup::new();
    let ifconfig = futures::executor::block_on(stub.lookup(ip.parse().unwrap())).unwrap();
    serde_json::to_value(ifconfig).unwrap()
}

fn validate_ifconfig(body: &Value, label: &str) {
    let doc = serde_json::to_value(openapi()).unwrap();
    let mut root = doc["components"]["schemas"]["Ifconfig"].clone();
    assert!(root.is_object(), "{label}: Ifconfig not documented");
    root.as_object_mut()
        .unwrap()
        .insert("components".into(), doc["components"].clone());
    let validator = jsonschema::validator_for(&root).expect("Ifconfig schema compiles");
    let errors: Vec<String> = validator.iter_errors(body).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{label}: {errors:?}\nbody: {body}");
}

/// C1, C10: the caller route answers for the forwarded client, the address route for its path.
#[tokio::test]
async fn ip_routes_answer_the_stubs_ifconfig() {
    let cases = [("/ip", "192.0.2.10"), ("/ip/198.51.100.1", "198.51.100.1")];
    for (uri, addr) in cases {
        let (status, body) = get(app(false, stub_parts()), uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(body["ip"]["addr"], addr, "{uri}");
        assert_eq!(body, stub_json(addr), "{uri}");
        validate_ifconfig(&body, uri);
    }
}

/// C15: a private address is refused by default and answered in internal mode.
#[tokio::test]
async fn private_address_needs_internal_mode() {
    let (status, body) = get(app(false, stub_parts()), "/ip/10.0.0.1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "INVALID_INPUT");

    let (status, body) = get(app(true, stub_parts()), "/ip/10.0.0.1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ip"]["addr"], "10.0.0.1");
}

/// C24: without an ip lookup every ip function answers 503 naming the module.
#[tokio::test]
async fn no_ip_lookup_answers_unavailable() {
    for uri in ["/ip", "/ip/198.51.100.1", "/asn/64496"] {
        let (status, body) = get(app(false, ApiParts::default()), uri).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(body["error"]["code"], "UNAVAILABLE", "{uri}");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("IP module"), "{uri}: {message}");
    }
}

/// C9, C28: the lookup is callable without axum types and equals the route's body.
#[tokio::test]
async fn stub_lookup_equals_the_route_body() {
    let lookup: Arc<dyn IpLookup> = Arc::new(StubIpLookup::new());
    let direct = lookup
        .lookup("198.51.100.1".parse().unwrap())
        .await
        .unwrap();
    let (status, body) = get(app(false, stub_parts()), "/ip/198.51.100.1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::to_value(direct).unwrap(), body);
}
