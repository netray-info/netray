//! Golden markup for the issues island's vitest suite (spec v2-result-page, Phase 6): the result
//! page of one entry with warn and fail checks across several owners, severities and protocols,
//! read by `islands/test/issues.test.ts`.
//!
//! Entry ids are random: each is replaced by `ENTRY0`, ... in order of first appearance.
//! `UPDATE_GOLDEN=1 cargo test -p netray-web --test result_islands_issues_fixture` rewrites
//! `islands/test/fixtures/result-issues.html`.
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
/// 2025-10-09 08:53:20 UTC.
const CREATED_AT: i64 = 1_760_000_000;

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

async fn get_main() -> (Arc<HistoryStore>, tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    store
        .record(DOMAIN, &run(), &[], CREATED_AT, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
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
async fn result_issues_markup_matches_fixture() {
    let (store, _dir, main) = get_main().await;
    let got = normalise(&store, main).await;
    assert!(got.contains(r#"id="issues""#), "{got}");
    check("result-issues.html", &got);
}
