//! Review findings of Phase 6 (v2-inspectors): the page charges one unit per port at admission
//! and the inspection never refuses afterwards; a busy refusal charges no limiter; the IP page
//! charges the caller before parsing, as v1's `GET /api/v1/ip/{a}` does.
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::testing::StubIpLookup;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value};
use netray_tls::testing::{StubTlsBackend, stub_state};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
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
        unreachable!("no run is started by the inspector routes")
    }
}

fn page(tool: &str, form: &str) -> Request<Body> {
    Request::post(format!("/inspect/{tool}/run"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_string()))
        .unwrap()
}

async fn answer(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (status, headers, decode(&text))
}

/// The html of every SSE `data:` line, JSON-unescaped; a body that is no stream stays as is.
fn decode(text: &str) -> String {
    let html: Vec<String> = text
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .filter_map(|v| v["html"].as_str().map(str::to_string))
        .collect();
    if html.is_empty() {
        text.to_string()
    } else {
        html.join("\n")
    }
}

#[tokio::test]
async fn tls_page_jobs_are_refused_with_429_never_inside_the_stream() {
    let config: ApiConfig = toml::from_str(
        "[tls]\nper_ip_burst = 3\nper_ip_per_minute = 1\nper_target_burst = 1000\nper_target_per_minute = 1000\n",
    )
    .expect("[api.tls] keys parse");
    let parts = ApiParts {
        tls: Some(stub_state(Arc::new(StubTlsBackend::new())).inspector()),
        ..Default::default()
    };
    let state = ApiState::new(config, Arc::new(NoRuns)).with_parts(parts);
    let app = inspect_router(InspectDeps::new(
        Arc::new(InspectJobs::new(state)),
        InspectConfig::default(),
    ));

    let mut statuses = Vec::new();
    for n in 0..6 {
        let (status, headers, body) = answer(&app, page("tls", "h=example.com")).await;
        statuses.push(status);
        match status {
            StatusCode::OK => {
                assert!(body.contains("id=\"ev-tls."), "answer {n}: {body}");
                assert!(!body.contains("rate limited"), "answer {n}: {body}");
                assert!(!body.contains("api_v1"), "answer {n}: {body}");
            }
            StatusCode::TOO_MANY_REQUESTS => {
                assert!(headers.contains_key(header::RETRY_AFTER), "answer {n}");
            }
            other => panic!("answer {n}: unexpected {other}: {body}"),
        }
    }
    assert_eq!(statuses[0], StatusCode::OK, "{statuses:?}");
    assert!(
        statuses[1..].contains(&StatusCode::TOO_MANY_REQUESTS),
        "{statuses:?}"
    );
}

struct Counting {
    admits: AtomicUsize,
}

impl InspectorJobs for Counting {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        self.admits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn start(&self, _: JobRequest) -> Result<Job, String> {
        Ok(Job::new(vec![JobPart {
            label: "handshake".into(),
            output: Box::pin(async {
                tokio::time::sleep(Duration::from_millis(500)).await;
                Ok(vec![Block {
                    id: BlockId::parse("tls.chain").unwrap(),
                    title: "Title".into(),
                    shape: Shape::Records(vec![Record {
                        owner: Value::Text("www.example.com".into()),
                        rtype: Value::Text("A".into()),
                        ttl: Value::Int(60),
                        data: Value::Addr("192.0.2.10".parse().unwrap()),
                    }]),
                }])
            }),
        }]))
    }
}

#[tokio::test]
async fn a_busy_refusal_charges_no_limiter() {
    let stub = Arc::new(Counting {
        admits: AtomicUsize::new(0),
    });
    let mut config = InspectConfig::default();
    config.tls.max_concurrent = 1;
    let app = inspect_router(InspectDeps::new(stub.clone(), config));

    let first = {
        let app = app.clone();
        tokio::spawn(async move { answer(&app, page("tls", "h=example.com")).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (status, _, body) = answer(&app, page("tls", "h=example.com")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body.contains("busy, try again in a moment"), "{body}");
    assert_eq!(
        stub.admits.load(Ordering::SeqCst),
        1,
        "the busy refusal charged a limiter"
    );
    first.await.unwrap();
}

#[tokio::test]
async fn ip_admission_charges_the_caller_as_v1_even_for_invalid_input() {
    let config: ApiConfig =
        toml::from_str("[ip]\nper_ip_burst = 2\nper_ip_per_minute = 1\ninternal_mode = true\n")
            .expect("[api.ip] keys parse");
    let parts = ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    };
    let state = ApiState::new(config, Arc::new(NoRuns)).with_parts(parts);
    let app = inspect_router(InspectDeps::new(
        Arc::new(InspectJobs::new(state)),
        InspectConfig::default(),
    ));

    for n in 0..2 {
        let (status, _, body) = answer(&app, page("ip", "ip=foo")).await;
        assert_eq!(status, StatusCode::OK, "invalid {n}: {body}");
        assert!(body.contains("invalid IP address"), "invalid {n}: {body}");
    }
    let (status, headers, body) = answer(&app, page("ip", "ip=192.0.2.1")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert!(headers.contains_key(header::RETRY_AFTER));
}
