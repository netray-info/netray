//! The result page after the Phase 2 review (spec v2-result-page, requirements 1, 3, 15, 16): an
//! entry scored with a profile this build does not embed still renders, a check's `since` starts
//! where the check first appears, and a wrong method on a non-domain segment reaches the fallback.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        let d = domain.to_lowercase();
        let d = d.strip_suffix('.').unwrap_or(&d).to_string();
        if !d.contains('.') {
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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn check(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn run(profile_version: u32, checks: Vec<RunCheck>) -> Run {
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks,
        blocks: vec![],
    }
}

async fn store_with(runs: &[(Run, i64)]) -> (Arc<HistoryStore>, Vec<String>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let mut ids = Vec::new();
    for (r, at) in runs {
        match store
            .record(DOMAIN, r, &[], *at, |p, c| diff(p, c, &[], None))
            .await
            .unwrap()
        {
            Recorded::First { id } | Recorded::Changed { id, .. } => ids.push(id),
            other => panic!("expected a new entry, got {other:?}"),
        }
    }
    (Arc::new(store), ids, dir)
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    send(app, Request::get(path).body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn an_entry_scored_with_an_unknown_profile_still_renders_as_the_latest() {
    let at = now() - 60;
    let (store, ids, _dir) =
        store_with(&[(run(4, vec![check("tls.chain_trusted", Status::Warn)]), at)]).await;
    let app = router(ResultState::new(store, Arc::new(StubRuns), 300));
    let (status, body) = get(&app, "/shop.example.com").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"data-state="fresh""#), "{body}");
    assert!(body.contains(&format!("entry {}", ids[0])), "{body}");
    assert!(!body.contains("not checked yet"), "{body}");
    assert!(body.contains(r#"id="band-tls""#), "{body}");
    assert!(body.contains("tls.chain_trusted"), "{body}");
}

#[tokio::test]
async fn since_starts_where_a_check_first_appears() {
    let day = 86_400;
    let t0 = now() - 30 * day;
    // E1 lacks tls.caa_compliant; E2 and E3 carry it as warn. E3 differs from E2 in chain_trusted.
    let (store, _ids, _dir) = store_with(&[
        (run(3, vec![check("tls.chain_trusted", Status::Pass)]), t0),
        (
            run(
                3,
                vec![
                    check("tls.chain_trusted", Status::Pass),
                    check("tls.caa_compliant", Status::Warn),
                ],
            ),
            t0 + 10 * day,
        ),
        (
            run(
                3,
                vec![
                    check("tls.chain_trusted", Status::Fail),
                    check("tls.caa_compliant", Status::Warn),
                ],
            ),
            t0 + 20 * day,
        ),
    ])
    .await;
    let app = router(ResultState::new(store, Arc::new(StubRuns), 300));
    let (_, body) = get(&app, "/shop.example.com").await;
    let secs = t0 + 10 * day;
    let days = secs.div_euclid(day);
    // days-to-civil for the month and day of E2.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let want = format!("{m:02}-{d:02}");
    let row = body
        .split(r#"<article class="check-row""#)
        .skip(1)
        .find(|r| r.contains("tls.caa_compliant"))
        .expect("caa issue row");
    assert!(
        row.contains(&format!(r#"<span class="check-row__since">{want}</span>"#))
            || row.contains(&format!("check-row__since\">since {want}")),
        "since {want} missing in {row}"
    );
}

#[tokio::test]
async fn a_wrong_method_on_a_segment_without_a_dot_reaches_the_fallback() {
    let (store, _ids, _dir) = store_with(&[]).await;
    let fallback: Arc<
        dyn Fn(Request<Body>) -> BoxFuture<'static, axum::response::Response> + Send + Sync,
    > = Arc::new(|_req| Box::pin(async { StatusCode::IM_A_TEAPOT.into_response() }));
    let app = router(ResultState::new(store, Arc::new(StubRuns), 300).with_fallback(fallback));
    let (status, _) = send(&app, Request::post("/about").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
    let (status, _) = get(&app, "/about/run").await;
    assert_eq!(status, StatusCode::IM_A_TEAPOT);
}
