//! v2-dkim-legacy-urls C3, C15, C16: `POST /email/inspect` accepts `dkim_selectors` and answers
//! 400 INVALID_INPUT for too many, a malformed selector, or any other unknown field.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, router,
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
        Box::pin(async { RunStart::NotStarted })
    }
}

fn stub_app() -> Router {
    let parts = ApiParts {
        email: Some(Arc::new(netray_email::testing::stub_inspector(
            netray_email::testing::example_dns(),
        ))),
        ..Default::default()
    };
    router(ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(parts))
}

async fn post(body: Value) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/email/inspect")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer: SocketAddr = "198.51.100.1:0".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let resp = stub_app().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

/// C3: one selector in `dkim_selectors` is accepted.
#[tokio::test]
async fn one_dkim_selector_answers_200() {
    let (status, body) = post(json!({"domain": "example.com", "dkim_selectors": ["s1"]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// C15, C16: six selectors, a malformed selector and another unknown field answer 400 INVALID_INPUT.
#[tokio::test]
async fn bad_selectors_and_unknown_fields_answer_400() {
    let cases = [
        json!({"domain": "example.com", "dkim_selectors": ["a", "b", "c", "d", "e", "f"]}),
        json!({"domain": "example.com", "dkim_selectors": ["bad.sel"]}),
        json!({"domain": "example.com", "selector": "s1"}),
    ];
    for case in cases {
        let (status, body) = post(case.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{case}: {body}");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["error"]["code"], "INVALID_INPUT", "{case}: {body}");
    }
}
