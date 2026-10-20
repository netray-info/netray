//! v2-inspector-shell requirement 24 (C1-C6): the email inspector page, its live run over the
//! API's email part, selectors, the shared `[api.email]` limiter, the off notice and the config.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_email::security::RateLimitState;
use netray_email::testing::{example_dns, stub_inspector};
use netray_email::{EmailInspect, EmailInspectRequest, EmailJob, EmailRefusal, InspectSlots};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
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

/// Forwards to the real stub inspector and records each request's selectors.
struct Recording {
    inner: Box<dyn EmailInspect>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl EmailInspect for Recording {
    fn inspect(
        &self,
        req: &EmailInspectRequest,
        client: IpAddr,
        limits: &RateLimitState,
        slots: &InspectSlots,
    ) -> Result<EmailJob, EmailRefusal> {
        let job = self.inner.inspect(req, client, limits, slots)?;
        self.seen
            .lock()
            .unwrap()
            .push(format!("{:?}", req.dkim_selectors));
        Ok(job)
    }
}

fn stub() -> Arc<dyn EmailInspect> {
    Arc::new(stub_inspector(example_dns()))
}

fn state(config: ApiConfig, email: Option<Arc<dyn EmailInspect>>) -> ApiState {
    ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        email,
        ..Default::default()
    })
}

fn inspect_app(state: &ApiState) -> Router {
    inspect_router(InspectDeps::new(
        Arc::new(InspectJobs::new(state.clone())),
        InspectConfig::default(),
    ))
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

async fn get(app: &Router, uri: &str) -> (StatusCode, String) {
    let (s, _, b) = send(app, Request::get(uri).body(Body::empty()).unwrap()).await;
    (s, b)
}

fn run_req(body: &str) -> Request<Body> {
    Request::post("/inspect/email/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn run(app: &Router, body: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    send(app, run_req(body)).await
}

#[tokio::test]
async fn c1_get_renders_the_email_shell_and_prefills_with_a_domain() {
    let app = inspect_app(&state(ApiConfig::default(), Some(stub())));
    let (status, body) = get(&app, "/inspect/email").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("inspector-shell"), "{body}");
    assert!(body.contains(r#"data-tool="email""#), "{body}");
    assert!(body.contains(r#"name="d""#), "{body}");
    assert!(body.contains("Domain"), "{body}");
    // The current inspector tab (the site nav marks "Inspectors" current too).
    let tab = body
        .split(r#"<a class="inspector-tabs__tab""#)
        .skip(1)
        .find(|a| {
            a.split('>')
                .next()
                .is_some_and(|t| t.contains(r#"aria-current="page""#))
        })
        .unwrap_or_else(|| panic!("no current tab in {body}"));
    assert!(tab.contains("EMAIL") || tab.contains("Email"), "{tab}");

    let (status, body) = get(&app, "/inspect/email?d=example.com").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"value="example.com""#), "{body}");
}

#[tokio::test]
async fn c2_run_streams_start_band_checks_blocks_and_leaves_the_store_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("history.db");
    let store = Arc::new(HistoryStore::open(&db).await.unwrap());
    store.migrate().await.unwrap();
    let seeded = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    };
    store
        .record("example.com", &seeded, &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&db);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    let snapshot = || async {
        sqlx::query_as::<_, (String, i64)>("SELECT id, last_seen_at FROM entries ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap()
    };
    let before = snapshot().await;
    assert_eq!(before.len(), 1);

    let st = state(ApiConfig::default(), Some(stub()));
    let app = inspect_router(
        InspectDeps::new(Arc::new(InspectJobs::new(st)), InspectConfig::default())
            .with_history(Some(store)),
    );
    let (status, _, stream) = run(&app, "d=example.com").await;
    assert_eq!(status, StatusCode::OK);
    assert!(stream.contains("<h1"), "{stream}");
    assert!(stream.contains("example.com"), "{stream}");
    assert!(
        stream.contains("figure") && stream.contains("inspector-band"),
        "{stream}"
    );
    // The stream carries each fragment's html as a JSON string: read it unescaped.
    let stream = stream.replace("\\\"", "\"").replace("\\n", "\n");
    let section = stream
        .split(r#"id="inspect-checks""#)
        .nth(1)
        .unwrap_or_else(|| panic!("no inspector-checks in {stream}"));
    // One row per Email catalogue check.
    let want = netray_engine::catalogue::catalogue()
        .for_protocol(netray_model::Protocol::Email)
        .entries()
        .len();
    assert_eq!(
        section.matches(r#"<article class="check-row""#).count(),
        want,
        "{section}"
    );
    assert!(stream.contains(r#"id="ev-email."#), "{stream}");
    assert!(stream.contains("event: end"), "{stream}");
    assert_eq!(snapshot().await, before);
}

#[tokio::test]
async fn c3_selectors_reach_the_inspection_and_too_many_answer_the_notice() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let rec: Arc<dyn EmailInspect> = Arc::new(Recording {
        inner: Box::new(stub_inspector(example_dns())),
        seen: seen.clone(),
    });
    let app = inspect_app(&state(ApiConfig::default(), Some(rec)));

    let (status, _, _) = run(&app, "d=example.com+s1+s2").await;
    assert_eq!(status, StatusCode::OK);
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].contains(r#"["s1", "s2"]"#), "{seen:?}");
    }

    seen.lock().unwrap().clear();
    let (status, _, body) = run(&app, "d=example.com+a%2Cb%2Cc%2Cd%2Ce%2Cf").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("inspect-result"), "{body}");
    assert!(body.contains("too many DKIM selectors"), "{body}");
    assert!(!body.contains(r#"id="ev-email."#), "{body}");
    assert!(seen.lock().unwrap().is_empty(), "inspection started");
}

#[tokio::test]
async fn c4_a_spent_api_email_budget_refuses_the_run_with_retry_after() {
    let config: ApiConfig = toml::from_str("[email]\nper_ip_burst = 1\n").unwrap();
    let st = state(config, Some(stub()));
    let api = netray_web::api::router(st.clone());
    let req = Request::post("/email/inspect")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"domain":"example.com"}"#))
        .unwrap();
    let (status, _, body) = send(&api, req).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let app = inspect_app(&st);
    let (status, headers, body) = run(&app, "d=example.com").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert!(headers.contains_key(header::RETRY_AFTER), "{headers:?}");
}

#[tokio::test]
async fn c5_without_the_email_part_the_run_answers_the_off_notice() {
    let app = inspect_app(&state(ApiConfig::default(), None));
    let (_, _, body) = run(&app, "d=example.com").await;
    assert!(body.contains("the email inspector is off"), "{body}");
}

#[test]
fn c6_a_zero_email_deadline_is_refused_by_name() {
    let c: InspectConfig = toml::from_str("[email]\ndeadline_ms = 0\n").unwrap();
    let err = c.validate().unwrap_err().to_string();
    assert!(err.contains("inspect.email.deadline_ms"), "{err}");
}
