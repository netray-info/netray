//! Server half of the issues and recent islands (spec v2-result-page, Phase 6; requirements 10, 25):
//! `data-owner-label` on every issue row, `data-island="recent"` on the result root, and the grade
//! and date of the shown run on `#result-grade`.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
/// 2025-10-09 08:53:20 UTC.
const CREATED_AT: i64 = 1_760_000_000;
const DATE: &str = "2025-10-09";

/// (check id, status, expected data-owner, expected label)
const CHECKS: [(&str, Status, &str, &str); 5] = [
    ("dns.ns", Status::Warn, "dns_provider", "your DNS provider"),
    (
        "tls.chain_trusted",
        Status::Fail,
        "web_server",
        "your web server configuration",
    ),
    (
        "http.hsts",
        Status::Warn,
        "web_server",
        "your web server configuration",
    ),
    (
        "email.spf",
        Status::Fail,
        "dns_provider",
        "your DNS provider",
    ),
    (
        "ip.reputation",
        Status::Fail,
        "hosting",
        "your hosting provider",
    ),
];
const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

struct StubRuns;

impl DomainRuns for StubRuns {
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

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::C,
        complete: true,
        sections: PROTOCOLS
            .iter()
            .map(|p| RunSection {
                protocol: *p,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        checks: CHECKS
            .iter()
            .map(|(id, status, _, _)| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status: *status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

async fn get_page(with_entry: bool, path: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    if with_entry {
        store
            .record(DOMAIN, &run(), &[], CREATED_AT, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    let app: Router = router(ResultState::new(store, Arc::new(StubRuns), u64::MAX / 2));
    let req = Request::get(path)
        .header("accept", "text/html")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

/// The opening tag that starts with `prefix`.
fn tags<'a>(body: &'a str, prefix: &str) -> Vec<&'a str> {
    body.match_indices(prefix)
        .map(|(at, _)| &body[at..at + body[at..].find('>').unwrap()])
        .collect()
}

#[tokio::test]
async fn c1_every_issue_row_carries_its_owner_label() {
    let body = get_page(true, &format!("/{DOMAIN}")).await;

    let rows = tags(&body, r#"<div class="issue""#);
    assert_eq!(rows.len(), CHECKS.len(), "{body}");
    for (_, _, owner, label) in CHECKS {
        let want = format!(r#"data-owner="{owner}""#);
        let label_attr = format!(r#"data-owner-label="{label}""#);
        let with_owner: Vec<_> = rows.iter().filter(|t| t.contains(&want)).collect();
        assert!(!with_owner.is_empty(), "no row for {owner}: {rows:?}");
        for t in with_owner {
            assert!(t.contains(&label_attr), "{label_attr} missing in {t}");
        }
    }
}

#[tokio::test]
async fn c2_result_root_and_grade_carry_the_recent_island_data() {
    let body = get_page(true, &format!("/{DOMAIN}")).await;

    let root = tags(&body, r#"<div id="result""#);
    assert_eq!(root.len(), 1, "{body}");
    assert!(root[0].contains(r#"data-island="recent""#), "{}", root[0]);
    let grade = tags(&body, r#"<div id="result-grade""#);
    assert_eq!(grade.len(), 1, "{body}");
    assert!(grade[0].contains(r#"data-grade="C""#), "{}", grade[0]);
    assert!(
        grade[0].contains(&format!(r#"data-date="{DATE}""#)),
        "{}",
        grade[0]
    );
}

#[tokio::test]
async fn c2_without_an_entry_the_grade_has_no_recent_data() {
    let body = get_page(false, &format!("/{DOMAIN}")).await;

    let grade = tags(&body, r#"<div id="result-grade""#);
    assert_eq!(grade.len(), 1, "{body}");
    assert!(!grade[0].contains("data-grade"), "{}", grade[0]);
    assert!(!grade[0].contains("data-date"), "{}", grade[0]);
}

#[tokio::test]
async fn c2_idle_page_keeps_the_hidden_recent_list() {
    let body = get_page(false, "/").await;

    assert!(
        body.contains(r#"<ul id="recent" data-island="recent" hidden>"#),
        "{body}"
    );
}
