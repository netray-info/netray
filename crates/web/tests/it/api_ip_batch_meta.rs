//! spec v2-ip-cli, criteria C9, C10, C34, C35: `POST /ip/batch?dns=` looks up reverse DNS per
//! address, and `GET /meta` carries the GeoLite2 attribution (paths relative to the mount).
use std::net::{IpAddr, SocketAddr};
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
        unreachable!("no run is started by these routes")
    }
}

fn app(parts: ApiParts) -> Router {
    netray_web::api::router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

fn ip_on() -> Router {
    app(ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    })
}

async fn send(app: Router, mut req: Request<Body>) -> (StatusCode, Value) {
    req.extensions_mut().insert(ConnectInfo(SocketAddr::new(
        "192.0.2.10".parse().unwrap(),
        40000,
    )));
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn batch(query: &str) -> Value {
    let req = Request::post(format!("/ip/batch{query}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"addresses": ["198.51.100.1"]}).to_string(),
        ))
        .unwrap();
    let (status, body) = send(ip_on(), req).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn batch_dns_flag_looks_up_reverse_dns_per_address_and_without_it_hostname_is_null() {
    for query in ["?dns=true", "?dns=1"] {
        let body = batch(query).await;
        assert_eq!(
            body["items"][0]["result"]["ip"]["hostname"], "stub.example.com",
            "{query}"
        );
    }
    let body = batch("").await;
    assert!(body["items"][0]["result"]["ip"]["hostname"].is_null());
}

#[tokio::test]
async fn meta_carries_the_maxmind_geolite2_attribution_with_the_ip_module_on_and_off() {
    assert!(netray_ip::GEOIP_ATTRIBUTION.contains("MaxMind"));
    assert!(netray_ip::GEOIP_ATTRIBUTION.contains("GeoLite2"));
    for (label, router) in [("on", ip_on()), ("off", app(ApiParts::default()))] {
        let (status, body) = send(router, Request::get("/meta").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK, "ip {label}");
        assert_eq!(
            body["attribution"],
            json!(netray_ip::GEOIP_ATTRIBUTION),
            "ip {label}"
        );
    }
}
