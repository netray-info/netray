//! API v1 `?field=` on `/ip` and `/ip/{address}` (spec v2-api, requirement 20): plain-text
//! single values rendered by netray-ip's `to_plain` over `StubIpLookup`, paths relative to the mount.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::IpLookup;
use netray_ip::backend::Ifconfig;
use netray_ip::handlers;
use netray_ip::testing::StubIpLookup;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
};
use serde_json::Value;
use tower::ServiceExt;

const CALLER: &str = "192.0.2.10:40000";
const UA: &str = "curl/8.5.0";
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

async fn get(path: &str, ua: Option<&str>) -> Reply {
    let mut req = Request::get(path);
    if let Some(ua) = ua {
        req = req.header(header::USER_AGENT, ua);
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

fn plain(name: &str, ifc: &Ifconfig) -> String {
    match name {
        "ip" => handlers::ip::to_plain(ifc),
        "asn" => handlers::asn::to_plain(ifc),
        "country" => handlers::country::to_plain(ifc),
        "city" => handlers::city::to_plain(ifc),
        "region" => handlers::region::to_plain(ifc),
        "timezone" => handlers::timezone::to_plain(ifc),
        "latitude" => handlers::latitude::to_plain(ifc),
        "longitude" => handlers::longitude::to_plain(ifc),
        "isp" => handlers::isp::to_plain(ifc),
        "location" => handlers::location::to_plain(ifc),
        "network" => handlers::network::to_plain(ifc),
        "tcp" => handlers::tcp::to_plain(ifc),
        "user_agent" => handlers::user_agent::to_plain(ifc),
        "host" => handlers::host::to_plain(ifc),
        other => panic!("no renderer for {other}"),
    }
}

const ADDRESS_FIELDS: [&str; 11] = [
    "ip",
    "asn",
    "country",
    "city",
    "region",
    "timezone",
    "latitude",
    "longitude",
    "isp",
    "location",
    "network",
];
const CALLER_ONLY: [&str; 4] = ["tcp", "user_agent", "host", "headers"];

fn assert_plain(r: &Reply, expected: &str, label: &str) {
    assert_eq!(r.status, StatusCode::OK, "{label}: {}", r.body);
    assert!(
        r.content_type.starts_with("text/plain"),
        "{label}: content type {}",
        r.content_type
    );
    assert_eq!(r.body, expected, "{label}");
    assert!(r.body.ends_with('\n'), "{label}: no trailing newline");
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
async fn address_field_country_is_the_stub_country_and_a_newline() {
    let r = get(&format!("/ip/{TARGET}?field=country"), None).await;
    assert_plain(&r, "Germany\n", "country");
}

#[tokio::test]
async fn the_eleven_address_fields_equal_to_plain_over_the_stub() {
    let stub = StubIpLookup::new();
    let target = stub.lookup(TARGET.parse().unwrap()).await.unwrap();
    let caller = stub
        .lookup_caller(CALLER.parse().unwrap(), None)
        .await
        .unwrap();
    for name in ADDRESS_FIELDS {
        let r = get(&format!("/ip/{TARGET}?field={name}"), None).await;
        assert_plain(&r, &plain(name, &target), &format!("/ip/{TARGET} {name}"));
        let r = get(&format!("/ip?field={name}"), None).await;
        assert_plain(&r, &plain(name, &caller), &format!("/ip {name}"));
    }
}

#[tokio::test]
async fn caller_fields_on_ip_are_rendered_from_the_request() {
    let stub = StubIpLookup::new();
    let caller = stub
        .lookup_caller(CALLER.parse().unwrap(), Some(UA))
        .await
        .unwrap();
    for name in ["user_agent", "tcp", "host"] {
        let r = get(&format!("/ip?field={name}"), Some(UA)).await;
        assert_plain(&r, &plain(name, &caller), &format!("/ip {name}"));
    }
    assert_eq!(handlers::tcp::to_plain(&caller), "40000\n");
    let r = get("/ip?field=headers", Some(UA)).await;
    assert_plain(
        &r,
        &handlers::headers::to_plain(&[("user-agent".to_string(), UA.to_string())]),
        "/ip headers",
    );
    assert!(r.body.contains("user-agent"), "{}", r.body);
}

#[tokio::test]
async fn caller_fields_on_an_address_are_refused_naming_the_caller_form() {
    for name in CALLER_ONLY {
        let r = get(&format!("/ip/{TARGET}?field={name}"), Some(UA)).await;
        let (code, message) = error_of(&r, name);
        assert_eq!(code, "INVALID_INPUT", "{name}");
        assert!(
            message.contains(&format!("/api/v1/ip?field={name}")),
            "{name}: {message}"
        );
    }
}

#[tokio::test]
async fn an_unknown_field_is_refused_listing_the_valid_names() {
    let r = get(&format!("/ip/{TARGET}?field=nope"), None).await;
    let (code, message) = error_of(&r, "address");
    assert_eq!(code, "INVALID_INPUT");
    assert!(message.contains("nope"), "{message}");
    assert!(
        message.contains(
            "ip, asn, country, city, region, timezone, latitude, longitude, isp, location, network"
        ),
        "{message}"
    );
    assert!(!message.contains("user_agent"), "{message}");

    let r = get("/ip?field=nope", None).await;
    let (code, message) = error_of(&r, "caller");
    assert_eq!(code, "INVALID_INPUT");
    assert!(
        message.contains("ip, asn, country, city, region, timezone, latitude, longitude, isp, location, network, tcp, user_agent, host, headers"),
        "{message}"
    );
}

fn enum_values(doc: &Value, schema: &Value, out: &mut Vec<String>) {
    if let Some(r) = schema["$ref"].as_str() {
        let name = r.rsplit('/').next().unwrap();
        enum_values(doc, &doc["components"]["schemas"][name], out);
    }
    if let Some(values) = schema["enum"].as_array() {
        out.extend(values.iter().filter_map(|v| v.as_str().map(String::from)));
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        for part in schema[key].as_array().into_iter().flatten() {
            enum_values(doc, part, out);
        }
    }
}

#[test]
fn the_document_lists_the_field_names_as_an_enum_per_path() {
    let doc = serde_json::to_value(netray_web::api::openapi()).unwrap();
    for (path, expected) in [
        ("/api/v1/ip/{address}", ADDRESS_FIELDS.to_vec()),
        (
            "/api/v1/ip",
            ADDRESS_FIELDS.iter().chain(&CALLER_ONLY).copied().collect(),
        ),
    ] {
        let params = doc["paths"][path]["get"]["parameters"]
            .as_array()
            .unwrap_or_else(|| panic!("{path}: no parameters"));
        let field = params
            .iter()
            .find(|p| p["name"] == "field" && p["in"] == "query")
            .unwrap_or_else(|| panic!("{path}: no `field` query parameter"));
        let mut values = vec![];
        enum_values(&doc, &field["schema"], &mut values);
        values.sort();
        let mut expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(values, expected, "{path}");
    }
}
