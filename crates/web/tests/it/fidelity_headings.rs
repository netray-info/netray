//! The heading outline of the result page (spec v2-result-fidelity, requirement 12; C12, C25).
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
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

// 2027-01-15 08:00:00 UTC; entry i is recorded i days later.
const T: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

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

/// Both checks flip with `i`, so every entry differs from its predecessor by two check changes.
fn run(i: usize) -> Run {
    let status = if i % 2 == 0 {
        Status::Pass
    } else {
        Status::Warn
    };
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![
            RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            },
            RunSection {
                protocol: Protocol::Tls,
                outcome: SectionKind::Measured,
                reason: None,
            },
        ],
        checks: ["dns.dnssec", "tls.chain_trusted"]
            .iter()
            .map(|id| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

async fn app(entries: usize) -> (Router, Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for i in 0..entries {
        store
            .record(
                "shop.example.com",
                &run(i),
                &[],
                T + i as i64 * DAY,
                |p, c| diff(p, c, &[], None),
            )
            .await
            .unwrap();
    }
    let store = Arc::new(store);
    let app = router(ResultState::new(store.clone(), Arc::new(StubRuns), 3600));
    (app, store, dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Headings h1-h6 in document order as (level, visible text).
fn headings(body: &str) -> Vec<(u8, String)> {
    let re = regex::Regex::new(r"(?s)<h([1-6])[^>]*>(.*?)</h([1-6])>").unwrap();
    let tag = regex::Regex::new(r"(?s)<[^>]*>").unwrap();
    re.captures_iter(body)
        .map(|c| {
            let text = tag.replace_all(&c[2], " ");
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            (c[1].parse().unwrap(), text)
        })
        .collect()
}

fn assert_no_skip(hs: &[(u8, String)]) {
    let mut prev = 0u8;
    for (level, text) in hs {
        assert!(
            *level <= prev + 1,
            "h{level} {text:?} follows h{prev}: {hs:?}"
        );
        prev = *level;
    }
}

#[tokio::test]
async fn c12_c25_result_page_outline_has_one_h1_an_h2_per_section_and_skips_no_level() {
    let (app, _store, _dir) = app(2).await;

    let (status, body) = get(&app, "/shop.example.com").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let hs = headings(&body);
    let h1: Vec<&str> = hs
        .iter()
        .filter(|h| h.0 == 1)
        .map(|h| h.1.as_str())
        .collect();
    assert_eq!(h1, vec!["shop.example.com"], "{hs:?}");
    let h2: Vec<&str> = hs
        .iter()
        .filter(|h| h.0 == 2)
        .map(|h| h.1.as_str())
        .collect();
    assert!(h2.contains(&"Since run 1"), "{h2:?} in {hs:?}");
    assert!(h2.contains(&"2 lines need attention"), "{h2:?} in {hs:?}");
    assert!(h2.contains(&"Evidence"), "{h2:?} in {hs:?}");
    assert_no_skip(&hs);
}

#[tokio::test]
async fn c12_c25_first_entry_outline_has_the_domain_h1_and_skips_no_level() {
    let (app, _store, _dir) = app(1).await;

    let (status, body) = get(&app, "/shop.example.com").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let hs = headings(&body);
    let h1: Vec<&str> = hs
        .iter()
        .filter(|h| h.0 == 1)
        .map(|h| h.1.as_str())
        .collect();
    assert_eq!(h1, vec!["shop.example.com"], "{hs:?}");
    assert_no_skip(&hs);
}
