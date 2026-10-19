//! spec v2-ip-cli, Phase 1, criteria C1, C2, C3, C11-C16, C36: reverse DNS on `/ip/{address}` by
//! default (`?dns=`), `?ip=` on `/ip` as the second spelling of `/ip/{address}`, and the
//! `IpLookup` option methods with their defaults. Router over `StubIpLookup`, paths relative to the mount.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::backend::Ifconfig;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, IpRefusal, LookupOptions};
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
        _client: IpAddr,
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

async fn get(path: &str) -> (StatusCode, String) {
    let mut req = Request::get(path).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    let resp = app().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn json(path: &str) -> Value {
    let (status, body) = get(path).await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|_| panic!("{path}: not JSON: {body}"))
}

fn error_of(status: StatusCode, body: &str) -> (String, String) {
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let v: Value = serde_json::from_str(body).unwrap();
    (
        v["error"]["code"].as_str().unwrap_or_default().to_string(),
        v["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

#[tokio::test]
async fn address_lookup_resolves_the_hostname_by_default() {
    let v = json(&format!("/ip/{TARGET}")).await;
    assert_eq!(v["ip"]["hostname"], "stub.example.com");
}

#[tokio::test]
async fn dns_false_leaves_the_hostname_null() {
    let v = json(&format!("/ip/{TARGET}?dns=false")).await;
    assert!(v["ip"]["hostname"].is_null(), "{v}");
}

#[tokio::test]
async fn dns_true_or_1_in_any_case_keeps_reverse_dns() {
    for value in ["true", "TRUE", "True", "1"] {
        let v = json(&format!("/ip/{TARGET}?dns={value}")).await;
        assert_eq!(v["ip"]["hostname"], "stub.example.com", "dns={value}");
    }
}

#[tokio::test]
async fn any_other_dns_value_turns_reverse_dns_off() {
    for value in ["false", "0", "yes", "no", "on", "2", "garbage", ""] {
        let v = json(&format!("/ip/{TARGET}?dns={value}")).await;
        assert!(v["ip"]["hostname"].is_null(), "dns={value}: {v}");
    }
}

#[tokio::test]
async fn host_is_an_address_field() {
    let (status, body) = get(&format!("/ip/{TARGET}?field=host")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, "stub.example.com\n");
}

#[tokio::test]
async fn ip_query_answers_the_named_address() {
    let v = json(&format!("/ip?ip={TARGET}")).await;
    assert_eq!(v["ip"]["addr"], TARGET);
}

#[tokio::test]
async fn ip_query_equals_the_path_form_by_default_and_with_dns_false() {
    for suffix in [
        "",
        "&dns=false",
        "&dns=true",
        "&field=host",
        "&field=country",
    ] {
        let query = get(&format!("/ip?ip={TARGET}{suffix}")).await;
        let path_suffix = suffix.replacen('&', "?", 1);
        let path = get(&format!("/ip/{TARGET}{path_suffix}")).await;
        assert_eq!(query, path, "suffix {suffix:?}");
    }
    let v = json(&format!("/ip?ip={TARGET}")).await;
    assert_eq!(v["ip"]["hostname"], "stub.example.com");
}

#[tokio::test]
async fn ip_query_with_a_private_address_is_invalid_input_like_the_path_form() {
    let (status, body) = get("/ip?ip=10.0.0.1").await;
    let (code, _) = error_of(status, &body);
    assert_eq!(code, "INVALID_INPUT");
    assert_eq!(get("/ip/10.0.0.1").await, (status, body));
}

#[tokio::test]
async fn ip_query_with_a_caller_only_field_is_refused_naming_the_field() {
    for name in ["tcp", "user_agent"] {
        let (status, body) = get(&format!("/ip?ip={TARGET}&field={name}")).await;
        let (code, message) = error_of(status, &body);
        assert_eq!(code, "INVALID_INPUT", "{name}");
        assert!(message.contains(name), "{name}: {message}");
    }
}

#[tokio::test]
async fn stub_with_reverse_dns_sets_the_hostname_and_without_it_does_not() {
    let stub = StubIpLookup::new();
    let ip: IpAddr = TARGET.parse().unwrap();
    let on = stub
        .lookup_with(
            ip,
            LookupOptions {
                reverse_dns: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(on.ip.hostname.as_deref(), Some("stub.example.com"));
    let off = stub
        .lookup_with(ip, LookupOptions::default())
        .await
        .unwrap();
    assert_eq!(off.ip.hostname, None);
    let caller = stub
        .lookup_caller_with(
            CALLER.parse().unwrap(),
            None,
            LookupOptions {
                reverse_dns: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(caller.ip.hostname.as_deref(), Some("stub.example.com"));
    assert!(stub.geoip_build_epoch().is_some());
}

/// Only the four pre-existing methods: the new ones come from the trait defaults.
struct OldLookup(StubIpLookup);

impl IpLookup for OldLookup {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        self.0.lookup(ip)
    }
    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        user_agent: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.0.lookup_caller(remote, user_agent)
    }
    fn range<'a>(
        &'a self,
        cidr: &'a str,
    ) -> BoxFuture<'a, Result<netray_ip::lookup::RangeResponse, IpRefusal>> {
        self.0.range(cidr)
    }
    fn asn(
        &self,
        n: u32,
    ) -> BoxFuture<'_, Result<netray_ip::lookup::AsnLookupResponse, IpRefusal>> {
        self.0.asn(n)
    }
}

#[tokio::test]
async fn an_implementation_with_only_the_old_methods_answers_lookup_with_as_lookup() {
    let old = OldLookup(StubIpLookup::new());
    let ip: IpAddr = TARGET.parse().unwrap();
    let with = old
        .lookup_with(
            ip,
            LookupOptions {
                reverse_dns: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let plain = old.lookup(ip).await.unwrap();
    assert_eq!(with.ip.hostname, None);
    assert_eq!(with.ip.addr, plain.ip.addr);
    assert_eq!(with.ip.hostname, plain.ip.hostname);
    assert_eq!(old.geoip_build_epoch(), None);
}
