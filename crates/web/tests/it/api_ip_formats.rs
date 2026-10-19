//! spec v2-ip-cli, Phase 1: criteria C5 R5, C6 R6, C19-C27 (`?format=`, `Accept` negotiation and
//! `?fields=` on `/ip` and `/ip/{address}`), bodies compared with netray-ip's renderers over `StubIpLookup`.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::IpLookup;
use netray_ip::LookupOptions;
use netray_ip::backend::Ifconfig;
use netray_ip::format::OutputFormat;
use netray_ip::handlers;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
};
use serde_json::Value;
use tower::ServiceExt;

const CALLER: &str = "192.0.2.10:40000";
const TARGET: &str = "198.51.100.1";

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: std::net::IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn app() -> Router {
    let parts = ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    };
    netray_web::api::router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

struct Reply {
    status: StatusCode,
    content_type: String,
    body: String,
}

async fn get(path: &str, accept: Option<&str>) -> Reply {
    let mut req = Request::get(path);
    if let Some(a) = accept {
        req = req.header(header::ACCEPT, a);
    }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    let resp = app().oneshot(req).await.unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        content_type,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

fn opts() -> LookupOptions {
    LookupOptions {
        reverse_dns: true,
        lang: None,
    }
}

async fn target() -> Ifconfig {
    StubIpLookup::new()
        .lookup_with(TARGET.parse().unwrap(), opts())
        .await
        .unwrap()
}

async fn caller() -> Ifconfig {
    StubIpLookup::new()
        .lookup_caller_with(CALLER.parse().unwrap(), None, opts())
        .await
        .unwrap()
}

fn body_of(format: OutputFormat, value: &Value) -> String {
    format.serialize_body(value).unwrap()
}

fn assert_ok(r: &Reply, content_type: &str, label: &str) {
    assert_eq!(r.status, StatusCode::OK, "{label}: {}", r.body);
    assert!(
        r.content_type.starts_with(content_type),
        "{label}: content type {}",
        r.content_type
    );
}

fn error_of(r: &Reply, label: &str) -> (String, String) {
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{label}: {}", r.body);
    let v: Value = serde_json::from_str(&r.body).unwrap_or_else(|_| panic!("{label}: not JSON"));
    (
        v["error"]["code"].as_str().unwrap_or_default().to_string(),
        v["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

#[tokio::test]
async fn c19_address_format_yaml_is_the_yaml_document() {
    let doc = handlers::root::to_json(&target().await).unwrap();
    let r = get(&format!("/ip/{TARGET}?format=yaml"), None).await;
    assert_ok(&r, "application/yaml", "yaml");
    assert_eq!(r.body, body_of(OutputFormat::Yaml, &doc));
}

#[tokio::test]
async fn c20_caller_format_toml_and_csv_are_the_document() {
    let doc = handlers::root::to_json(&caller().await).unwrap();
    let r = get("/ip?format=toml", None).await;
    assert_ok(&r, "application/toml", "toml");
    assert_eq!(r.body, body_of(OutputFormat::Toml, &doc));
    let r = get("/ip?format=csv", None).await;
    assert_ok(&r, "text/csv", "csv");
    assert_eq!(r.body, body_of(OutputFormat::Csv, &doc));
}

#[tokio::test]
async fn r5_every_format_renders_the_document_on_both_paths() {
    for (path, ifc) in [
        (format!("/ip/{TARGET}"), target().await),
        ("/ip".to_string(), caller().await),
    ] {
        let doc = handlers::root::to_json(&ifc).unwrap();
        let r = get(&format!("{path}?format=json"), None).await;
        assert_ok(&r, "application/json", &path);
        assert_eq!(
            serde_json::from_str::<Value>(&r.body).unwrap(),
            doc,
            "{path} json"
        );
        for (name, fmt, ct) in [
            ("yaml", OutputFormat::Yaml, "application/yaml"),
            ("toml", OutputFormat::Toml, "application/toml"),
            ("csv", OutputFormat::Csv, "text/csv"),
        ] {
            let r = get(&format!("{path}?format={name}"), None).await;
            assert_ok(&r, ct, &format!("{path} {name}"));
            assert_eq!(r.body, body_of(fmt, &doc), "{path} {name}");
        }
        let r = get(&format!("{path}?format=plain"), None).await;
        assert_ok(&r, "text/plain", &format!("{path} plain"));
        assert_eq!(r.body, handlers::root::to_plain(&ifc), "{path} plain");
    }
}

#[tokio::test]
async fn r5_an_object_field_renders_in_each_format_and_a_scalar_in_json_and_plain() {
    let ifc = target().await;
    let loc = handlers::location::to_json(&ifc).unwrap();
    let r = get(&format!("/ip/{TARGET}?field=location&format=yaml"), None).await;
    assert_ok(&r, "application/yaml", "location yaml");
    assert_eq!(r.body, body_of(OutputFormat::Yaml, &loc));
    let r = get(&format!("/ip/{TARGET}?field=location&format=toml"), None).await;
    assert_ok(&r, "application/toml", "location toml");
    assert_eq!(r.body, body_of(OutputFormat::Toml, &loc));
    let r = get(&format!("/ip/{TARGET}?field=location&format=plain"), None).await;
    assert_ok(&r, "text/plain", "location plain");
    assert_eq!(r.body, handlers::location::to_plain(&ifc));
    let r = get(&format!("/ip/{TARGET}?field=country&format=plain"), None).await;
    assert_ok(&r, "text/plain", "country plain");
    assert_eq!(r.body, handlers::country::to_plain(&ifc));
    let r = get(&format!("/ip/{TARGET}?field=country&format=json"), None).await;
    assert_ok(&r, "application/json", "country json");
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        handlers::country::to_json(&ifc).unwrap()
    );
}

#[tokio::test]
async fn c21_accept_yaml_toml_and_csv_pick_the_form() {
    let doc = handlers::root::to_json(&caller().await).unwrap();
    for (accept, fmt, ct) in [
        ("application/yaml", OutputFormat::Yaml, "application/yaml"),
        ("application/toml", OutputFormat::Toml, "application/toml"),
        ("text/csv", OutputFormat::Csv, "text/csv"),
    ] {
        let r = get("/ip", Some(accept)).await;
        assert_ok(&r, ct, accept);
        assert_eq!(r.body, body_of(fmt, &doc), "{accept}");
    }
}

#[tokio::test]
async fn c22_accept_text_plain_keeps_the_json_document() {
    let doc = handlers::root::to_json(&caller().await).unwrap();
    let r = get("/ip", Some("text/plain")).await;
    assert_ok(&r, "application/json", "text/plain");
    assert_eq!(serde_json::from_str::<Value>(&r.body).unwrap(), doc);
}

#[tokio::test]
async fn c23_field_location_format_json_is_the_location_json() {
    let ifc = target().await;
    let r = get(&format!("/ip/{TARGET}?field=location&format=json"), None).await;
    assert_ok(&r, "application/json", "location json");
    assert_eq!(
        serde_json::from_str::<Value>(&r.body).unwrap(),
        handlers::location::to_json(&ifc).unwrap()
    );
}

#[tokio::test]
async fn c24_a_scalar_field_as_toml_is_refused_naming_field_and_format() {
    let r = get(&format!("/ip/{TARGET}?field=country&format=toml"), None).await;
    let (code, message) = error_of(&r, "country toml");
    assert_eq!(code, "INVALID_INPUT");
    assert!(message.contains("country"), "{message}");
    assert!(message.contains("toml"), "{message}");
}

#[tokio::test]
async fn c25_an_unknown_format_is_refused() {
    for path in [
        "/ip?format=xml".to_string(),
        format!("/ip/{TARGET}?format=xml"),
    ] {
        let r = get(&path, None).await;
        let (code, message) = error_of(&r, &path);
        assert_eq!(code, "INVALID_INPUT", "{path}");
        assert!(message.contains("xml"), "{path}: {message}");
    }
}

#[tokio::test]
async fn c26_fields_keeps_exactly_the_named_keys() {
    let doc = handlers::root::to_json(&caller().await).unwrap();
    let r = get("/ip?fields=ip,location&format=json", None).await;
    assert_ok(&r, "application/json", "fields");
    let v: Value = serde_json::from_str(&r.body).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["ip", "location"]);
    assert_eq!(v["ip"], doc["ip"]);
    assert_eq!(v["location"], doc["location"]);
}

#[tokio::test]
async fn r6_unknown_fields_names_are_dropped_silently() {
    let r = get("/ip?fields=ip,nope&format=json", None).await;
    assert_ok(&r, "application/json", "fields unknown");
    let v: Value = serde_json::from_str(&r.body).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["ip"]);
}

#[tokio::test]
async fn r6_fields_filters_an_object_field_and_not_plain_text() {
    let ifc = target().await;
    let r = get(
        &format!("/ip/{TARGET}?field=location&fields=country&format=json"),
        None,
    )
    .await;
    assert_ok(&r, "application/json", "object field");
    let v: Value = serde_json::from_str(&r.body).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["country"]);

    let r = get(&format!("/ip/{TARGET}?fields=ip&format=plain"), None).await;
    assert_ok(&r, "text/plain", "document plain");
    assert_eq!(r.body, handlers::root::to_plain(&ifc));
}

#[tokio::test]
async fn c27_fields_does_not_change_a_plain_field() {
    let r = get("/ip?fields=ip&field=country", None).await;
    assert_ok(&r, "text/plain", "plain field");
    assert_eq!(r.body, "Germany\n");
}
