//! The history page and the trace depth of the result page (spec v2-result-page, requirement 18;
//! C8, C17): `GET /{domain}/history` through `netray_web::pages::result::router`.
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
        checks: ["dns.dnssec_valid", "tls.chain_trusted"]
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

/// The `<tr>` chunks of `<table id="history">`, in document order.
fn history_rows(body: &str) -> Vec<&str> {
    let table = body
        .split("<table id=\"history\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no table#history in {body}"));
    let table = table.split("</table>").next().unwrap();
    table.split("<tr").skip(1).collect()
}

/// The visible words of a chunk of HTML.
fn words(html: &str) -> Vec<String> {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().map(str::to_string).collect()
}

#[tokio::test]
async fn c8_history_lists_every_entry_newest_first_with_its_columns() {
    let (app, store, _dir) = app(3).await;
    let entries = store.entries("shop.example.com").await.unwrap();
    assert_eq!(entries.len(), 3);

    let (status, body) = get(&app, "/shop.example.com/history").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let rows: Vec<&str> = history_rows(&body)
        .into_iter()
        .filter(|r| r.contains("href=\"/r/"))
        .collect();
    assert_eq!(rows.len(), 3, "{body}");
    // Newest first: entry 2, 1, 0 (created 2027-01-17, -16, -15).
    let want_created = ["2027-01-17", "2027-01-16", "2027-01-15"];
    let want_ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    for (k, row) in rows.iter().enumerate() {
        assert!(
            want_ids
                .iter()
                .any(|id| row.contains(&format!("href=\"/r/{id}\""))),
            "row {k} links no entry: {row}"
        );
        assert!(row.contains(want_created[k]), "row {k}: {row}");
        let w = words(row);
        assert!(w.iter().any(|x| x == "A"), "grade in row {k}: {row}");
        assert!(
            w.iter().any(|x| x == "3"),
            "profile version in row {k}: {row}"
        );
        if k < 2 {
            assert!(w.iter().any(|x| x == "2"), "two changes in row {k}: {row}");
            assert!(!w.iter().any(|x| x == "first"), "row {k}: {row}");
        } else {
            assert!(w.iter().any(|x| x == "first"), "row {k}: {row}");
        }
    }
    // The newest row links the newest entry, the oldest row the oldest.
    let newest = entries.iter().max_by_key(|e| e.created_at).unwrap();
    let oldest = entries.iter().min_by_key(|e| e.created_at).unwrap();
    assert!(
        rows[0].contains(&format!("href=\"/r/{}\"", newest.id)),
        "{}",
        rows[0]
    );
    assert!(
        rows[2].contains(&format!("href=\"/r/{}\"", oldest.id)),
        "{}",
        rows[2]
    );
}

#[tokio::test]
async fn c8_history_without_an_entry_is_404_with_a_link_back() {
    let (app, _store, _dir) = app(0).await;

    let (status, body) = get(&app, "/shop.example.com/history").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(body.contains("not checked yet"), "{body}");
    assert!(body.contains("href=\"/shop.example.com\""), "{body}");
}

#[tokio::test]
async fn c17_thirty_entries_all_listed_and_the_trace_shows_twenty_four_rows() {
    let (app, store, _dir) = app(30).await;
    let entries = store.entries("shop.example.com").await.unwrap();
    assert_eq!(entries.len(), 30);
    let mut want: Vec<&netray_store::Entry> = entries.iter().collect();
    want.sort_by_key(|e| std::cmp::Reverse(e.created_at));

    let (status, body) = get(&app, "/shop.example.com/history").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let got: Vec<String> = history_rows(&body)
        .into_iter()
        .filter_map(|r| {
            let rest = r.split("href=\"/r/").nth(1)?;
            Some(rest.split('"').next()?.to_string())
        })
        .collect();
    let want_ids: Vec<String> = want.iter().map(|e| e.id.clone()).collect();
    assert_eq!(got, want_ids);

    let (status, page) = get(&app, "/shop.example.com").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let trace = page
        .split("id=\"trace\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no #trace in {page}"));
    let trace = trace.split("id=\"legend\"").next().unwrap();
    assert_eq!(trace.matches("data-run=\"").count(), 24, "{trace}");
}
