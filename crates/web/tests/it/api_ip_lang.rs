//! spec v2-ip-cli, criteria C4 R4, C17, C18: `?lang=<tag>` on `/ip` and `/ip/{address}` reaches the
//! lookup (`city_localized` / `country_localized`) and a malformed tag answers 400 `INVALID_INPUT`.
use std::net::SocketAddr;
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

async fn get(path: &str) -> (StatusCode, Value) {
    let mut req = Request::get(path).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    let resp = app().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("{path}: body is not JSON: {e}: {bytes:?}"));
    (status, body)
}

#[tokio::test]
async fn lang_de_localizes_country_and_city_on_address_and_caller() {
    for path in [format!("/ip/{TARGET}?lang=de"), "/ip?lang=de".to_string()] {
        let (status, body) = get(&path).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert_eq!(
            body["location"]["country_localized"], "Deutschland",
            "{path}"
        );
        assert_eq!(body["location"]["city_localized"], "Berlin", "{path}");
        assert_eq!(body["location"]["country"], "Germany", "{path}");
    }
}

#[tokio::test]
async fn without_lang_the_answer_carries_no_localized_country() {
    for path in [format!("/ip/{TARGET}"), "/ip".to_string()] {
        let (status, body) = get(&path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(
            body["location"]["country_localized"].is_null(),
            "{path}: {body}"
        );
    }
}

#[tokio::test]
async fn a_16_char_tag_of_letters_digits_and_hyphen_is_accepted() {
    let tag = "zh-Hant-TW-x1234"; // 16 chars
    assert_eq!(tag.len(), 16);
    for path in [
        format!("/ip/{TARGET}?lang={tag}"),
        format!("/ip?lang={tag}"),
        format!("/ip/{TARGET}?lang=pt-BR"),
    ] {
        let (status, body) = get(&path).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }
}

#[tokio::test]
async fn malformed_lang_tags_answer_400_invalid_input() {
    let cases = [
        ("empty", ""),
        ("17 chars", "abcdefghijklmnopq"),
        ("semicolon", "de%3Bx"),
        ("underscore", "de_DE"),
        ("space", "de%20x"),
        ("non-ascii letter", "d%C3%A9"),
        ("slash", "de%2Fx"),
    ];
    for (label, tag) in cases {
        for path in [
            format!("/ip/{TARGET}?lang={tag}"),
            format!("/ip?lang={tag}"),
        ] {
            let (status, body) = get(&path).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{label} {path}: {body}");
            assert_eq!(body["error"]["code"], "INVALID_INPUT", "{label} {path}");
        }
    }
}
