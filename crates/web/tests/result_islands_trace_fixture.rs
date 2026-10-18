//! Golden markup for the spectrum island's vitest suite (spec v2-result-page, Phase 5): the
//! result page with three recorded entries whose check statuses differ, read by
//! `islands/test/spectrum.test.ts`.
//!
//! Entry ids are random: each is replaced by `ENTRY0`, `ENTRY1`, ... in order of first appearance.
//! `UPDATE_GOLDEN=1 cargo test -p netray-web --test result_islands_trace_fixture` rewrites
//! `islands/test/fixtures/result-trace.html`.
use std::net::IpAddr;
use std::path::PathBuf;
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
const FIRST_AT: i64 = 1_760_000_000;
const ORDER: [(Protocol, &str); 5] = [
    (Protocol::Dns, "dns.ns"),
    (Protocol::Tls, "tls.chain_trusted"),
    (Protocol::Http, "http.hsts"),
    (Protocol::Email, "email.spf"),
    (Protocol::Ip, "ip.reputation"),
];
/// Oldest first.
const STATUSES: [Status; 3] = [Status::Pass, Status::Warn, Status::Fail];

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

fn run_with(grade: Grade, status: Status) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: ORDER
            .iter()
            .map(|(p, _)| RunSection {
                protocol: *p,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        checks: ORDER
            .iter()
            .map(|(_, id)| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

async fn get_main() -> (Arc<HistoryStore>, tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let grades = [Grade::A, Grade::B, Grade::C];
    for (i, status) in STATUSES.iter().enumerate() {
        store
            .record(
                DOMAIN,
                &run_with(grades[i], *status),
                &[],
                FIRST_AT + i as i64 * 86_400,
                |p, c| diff(p, c, &[], None),
            )
            .await
            .unwrap();
    }
    let app: Router = router(ResultState::new(
        store.clone(),
        Arc::new(StubRuns),
        u64::MAX / 2,
    ));
    let req = Request::get(format!("/{DOMAIN}"))
        .header("accept", "text/html")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).expect("utf-8");
    let start = html.find("<main>").expect("<main>") + "<main>".len();
    let end = html.find("</main>").expect("</main>");
    (store, dir, html[start..end].to_string())
}

/// Replace every stored entry id with `ENTRY<n>`, numbered by first appearance in `text`, and the
/// wall-clock minute of a live run with `NOW`.
async fn normalise(store: &HistoryStore, text: String) -> String {
    let mut ids: Vec<(usize, String)> = store
        .entries(DOMAIN)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|e| text.find(&e.id).map(|at| (at, e.id)))
        .collect();
    ids.sort();
    let mut out = text;
    for (n, (_, id)) in ids.iter().enumerate() {
        out = out.replace(id.as_str(), &format!("ENTRY{n}"));
    }
    // A live run's trace row carries the wall clock; every fixed fixture time lies in 2025.
    regex::Regex::new(r"20(2[6-9]|[3-9][0-9])-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}")
        .unwrap()
        .replace_all(&out, "NOW")
        .into_owned()
}

fn check(name: &str, got: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("islands/test/fixtures")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, got).expect("write fixture");
    }
    let want =
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture {name} is missing"));
    assert_eq!(
        got, want,
        "{name} differs from the fixture; if intended, run with UPDATE_GOLDEN=1"
    );
}

#[tokio::test]
async fn result_trace_markup_matches_fixture() {
    let (store, _dir, main) = get_main().await;
    let got = normalise(&store, main).await;
    assert!(got.contains(r#"id="spectrum""#), "{got}");
    check("result-trace.html", &got);
}
