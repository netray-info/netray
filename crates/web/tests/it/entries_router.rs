//! The entries router (V2 spec v2-entries, requirements 8, 10, 11, 12, 14): precedence of
//! malformed ID, entry, V1 snapshot and gone page; headers; failed lookups; the 404 catch-all.
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, RETENTION_SECS, Recorded};
use netray_web::entries::{EntriesDeps, EntryCard, NOT_FOUND_HTML, V1Lookup, router};
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

const T: i64 = 1_800_000_000;
const HTML: &str = "text/html; charset=utf-8";
const CACHE: &str = "public, max-age=3600";

fn run() -> Run {
    Run {
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
    }
}

async fn open() -> (Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}

async fn record(store: &HistoryStore, domain: &str, now: i64) -> String {
    match store
        .record(domain, &run(), &[], now, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    }
}

fn app(store: Arc<HistoryStore>, v1: Option<V1Lookup>) -> Router {
    router(EntriesDeps {
        history: store,
        v1,
        card: Arc::new(|_: &EntryCard| Ok(Vec::new())),
    })
}

/// A V1 lookup answering `Some(status, body)` for every ID and counting its calls.
fn v1_stub(status: StatusCode, body: &'static str) -> (V1Lookup, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let lookup: V1Lookup = Arc::new(move |_id: String| {
        c.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let mut resp: Response = (status, body).into_response();
            resp.headers_mut().insert("x-v1", "yes".parse().unwrap());
            Some(resp)
        })
    });
    (lookup, calls)
}

async fn get(app: &Router, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn header_of(h: &axum::http::HeaderMap, name: header::HeaderName) -> &str {
    h.get(name).map(|v| v.to_str().unwrap()).unwrap_or("")
}

#[test]
fn not_found_html_is_the_v1_body_byte_for_byte() {
    assert_eq!(
        NOT_FOUND_HTML,
        "<!DOCTYPE html><html><head><title>Not found</title></head><body><h1>Snapshot not found</h1><p>This snapshot has expired or the ID is unknown.</p></body></html>"
    );
}

#[tokio::test]
async fn c10_entry_page_answers_200_with_the_domain_and_cache_headers() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", T).await;

    let (status, headers, body) = get(&app(store, None), &format!("/r/{id}")).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("a.example.com"));
    assert_eq!(header_of(&headers, header::CONTENT_TYPE), HTML);
    assert_eq!(header_of(&headers, header::CACHE_CONTROL), CACHE);
}

#[tokio::test]
async fn c11_swept_entry_answers_410_gone_without_the_domain() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", T).await;
    assert_eq!(store.sweep(T + RETENTION_SECS + 86_400).await.unwrap(), 1);

    let (status, headers, body) = get(&app(store, None), &format!("/r/{id}")).await;

    assert_eq!(status, StatusCode::GONE);
    assert!(body.contains("This entry has expired"));
    assert!(!body.contains("a.example.com"));
    assert_eq!(header_of(&headers, header::CONTENT_TYPE), HTML);
    assert_eq!(header_of(&headers, header::CACHE_CONTROL), CACHE);
}

#[tokio::test]
async fn c12_never_issued_well_formed_id_answers_410() {
    let (store, _dir) = open().await;
    let (status, headers, _) = get(&app(store, None), "/r/AAAAAAAA").await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(header_of(&headers, header::CONTENT_TYPE), HTML);
    assert_eq!(header_of(&headers, header::CACHE_CONTROL), CACHE);
}

#[tokio::test]
async fn c3_entry_wins_over_a_v1_snapshot_with_the_same_id() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", T).await;
    let (v1, calls) = v1_stub(StatusCode::OK, "V1-PAGE-BODY");

    let (status, _, body) = get(&app(store, Some(v1)), &format!("/r/{id}")).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("a.example.com"));
    assert!(!body.contains("V1-PAGE-BODY"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn c3_v1_snapshot_answers_unchanged_when_no_entry_matches() {
    let (store, _dir) = open().await;
    let (v1, _) = v1_stub(StatusCode::OK, "V1-PAGE-BODY");

    let (status, headers, body) = get(&app(store, Some(v1)), "/r/AAAAAAAA").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "V1-PAGE-BODY");
    assert_eq!(header_of(&headers, "x-v1".parse().unwrap()), "yes");
}

#[tokio::test]
async fn c3_v1_returning_none_falls_through_to_gone() {
    let (store, _dir) = open().await;
    let v1: V1Lookup = Arc::new(|_| Box::pin(async { None }));
    let (status, _, _) = get(&app(store, Some(v1)), "/r/AAAAAAAA").await;
    assert_eq!(status, StatusCode::GONE);
}

#[tokio::test]
async fn c3_malformed_id_is_404_even_with_a_v1_lookup() {
    let (store, _dir) = open().await;
    let (v1, calls) = v1_stub(StatusCode::OK, "V1-PAGE-BODY");
    let (status, _, body) = get(&app(store, Some(v1)), "/r/x").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, NOT_FOUND_HTML);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn c14_failed_v1_lookup_answers_500_not_410() {
    let (store, _dir) = open().await;
    let (v1, _) = v1_stub(StatusCode::INTERNAL_SERVER_ERROR, "V1-FAILED");

    let (status, _, body) = get(&app(store, Some(v1)), "/r/AAAAAAAA").await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, "V1-FAILED");
}

#[tokio::test]
async fn c16_unmatched_and_malformed_paths_answer_404_with_the_not_found_body() {
    let (store, _dir) = open().await;
    let id = record(&store, "a.example.com", T).await;
    let app = app(store, None);
    let paths = [
        "/r/x".to_string(),
        "/r/AAAA-AAA".to_string(),
        "/r/%C0".to_string(),
        "/r/".to_string(),
        "/r/AAAAAAAA/".to_string(),
        "/r/a/b/c".to_string(),
        format!("/r/{id}/other/x"),
    ];
    for path in paths {
        let (status, _, body) = get(&app, &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body, NOT_FOUND_HTML, "{path}");
    }
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn c5_failed_history_lookup_answers_500_and_warns_with_the_id() {
    let (store, dir) = open().await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("history.db"));
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();
    let (v1, calls) = v1_stub(StatusCode::OK, "V1-PAGE-BODY");

    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let (status, _, body) = get(&app(store, Some(v1)), "/r/AAAAAAAA").await;
    drop(guard);

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body.contains("V1-PAGE-BODY"));
    assert!(!body.contains("This entry has expired"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    let line = logged
        .lines()
        .find(|l| l.contains("AAAAAAAA"))
        .unwrap_or_else(|| panic!("no log line names the ID:\n{logged}"));
    assert!(line.contains("WARN"), "not a warn line: {line}");
}
