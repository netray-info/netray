//! Result page clean bar and coverage line (spec v2-result-fidelity, Phase 4, requirement 33;
//! C6, C14, C15): the passed section is a `clean-bar` and no coverage line renders.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.trim().to_lowercase())
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

/// 35 real catalogue checks, alternating pass and not-applicable, and every protocol measured.
fn run() -> Run {
    let checks: Vec<RunCheck> = catalogue()
        .entries()
        .iter()
        .take(35)
        .enumerate()
        .map(|(i, spec)| RunCheck {
            id: spec.id.clone(),
            status: if i % 2 == 0 {
                Status::Pass
            } else {
                Status::NotApplicable
            },
            findings: vec![],
            evidence: vec![],
        })
        .collect();
    let sections = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ]
    .into_iter()
    .map(|protocol| RunSection {
        protocol,
        outcome: SectionKind::Measured,
        reason: None,
    })
    .collect();
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections,
        checks,
        blocks: vec![],
    }
}

async fn render() -> String {
    let r = run();
    assert_eq!(r.checks.len(), 35, "catalogue holds at least 35 checks");
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    store
        .record(DOMAIN, &r, &[], 1_760_000_000, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app: Router = router(ResultState::new(store, Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(Request::get(format!("/{DOMAIN}")).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn c6_c14_passed_section_is_a_clean_bar_over_the_35_rows() {
    let body = render().await;

    assert!(
        body.contains(r#"<details id="passed" class="clean-bar">"#),
        "{body}"
    );
    let at = body.find(r#"<details id="passed""#).unwrap();
    let details = &body[at..at + body[at..].find("</details>").unwrap()];
    let summary = &details[details.find("<summary").unwrap()..details.find("</summary>").unwrap()];
    assert_eq!(
        summary,
        concat!(
            r#"<summary class="clean-bar__summary"><span><strong class="clean-bar__count">"#,
            r#"35 lines clean</strong> <span class="clean-bar__label">· checks that pass</span>"#,
            r#"</span><span class="clean-bar__show">show</span>"#
        )
    );
    assert_eq!(details.matches(r#"class="check-row""#).count(), 35, "{details}");
}

#[tokio::test]
async fn c15_no_coverage_line_renders() {
    let body = render().await;

    assert!(body.contains(r#"id="evidence""#), "{body}");
    for protocol in ["dns", "tls", "http", "email", "ip"] {
        assert!(
            !body.contains(&format!("{protocol} measured")),
            "{protocol} measured in {body}"
        );
    }
    assert!(!body.contains("<span>measured</span>"), "{body}");
}
