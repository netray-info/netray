//! V2 spec v2-result-page, Phase 2 (requirement 1, criteria C1 web half, C19): the result
//! router serves `/`, `/{domain}` and `/{domain}/history`, hands single non-domain segments to
//! the fallback, and every response carries v2-pages' CSP.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use futures::future::BoxFuture;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        let d = domain.trim().to_lowercase();
        let d = d.strip_suffix('.').unwrap_or(&d).to_string();
        if d.is_empty() || d.contains('*') {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: format!("invalid domain: {domain}"),
            });
        }
        Ok(d)
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

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record("shop.example.com", &run(), &[], T, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let state = ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600).with_fallback(
        Arc::new(|_req| -> BoxFuture<'static, Response<Body>> {
            Box::pin(async {
                Response::builder()
                    .status(StatusCode::IM_A_TEAPOT)
                    .body(Body::empty())
                    .unwrap()
            })
        }),
    );
    (router(state), dir)
}

async fn get(app: &Router, path: &str) -> Response<Body> {
    app.clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn single_segments_that_are_not_domains_go_to_the_fallback() {
    let (app, _dir) = app().await;
    for path in [
        "/robots.txt",
        "/sitemap.xml",
        "/favicon.svg",
        "/favicon.ico",
        "/bimi-logo.svg",
        "/nodot",
        "/.well-known/history",
    ] {
        assert_eq!(get(&app, path).await.status(), 418, "{path}");
    }
}

#[tokio::test]
async fn every_result_route_carries_the_v2_pages_csp() {
    let (app, _dir) = app().await;
    let guide = pages::router()
        .oneshot(Request::get("/guide/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(guide.status(), 200);
    let expected = guide
        .headers()
        .get("content-security-policy")
        .expect("guide csp")
        .to_str()
        .unwrap()
        .to_string();

    let routes = [
        ("/", 200),
        ("/shop.example.com", 200),
        ("/nobody.example.com", 200),
        ("/shop.example.com/history", 200),
        ("/nobody.example.com/history", 404),
        ("/bad%2Aname.example.com", 400),
        ("/Shop.Example.com", 301),
    ];
    for (path, status) in routes {
        let res = get(&app, path).await;
        assert_eq!(res.status().as_u16(), status, "{path}: status");
        let csp = res
            .headers()
            .get("content-security-policy")
            .unwrap_or_else(|| panic!("{path}: missing csp"))
            .to_str()
            .unwrap();
        assert_eq!(csp, expected, "{path}: csp");
        let script_src = csp
            .split("; ")
            .find(|d| d.starts_with("script-src"))
            .unwrap();
        assert!(!script_src.contains("'unsafe-inline'"), "{path}");
        assert!(!script_src.contains("'unsafe-eval'"), "{path}");
    }
}
