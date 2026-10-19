//! Golden markup for the islands' vitest suite (spec v2-result-page, Phase 4): the result page and
//! the run stream as the Phase 3 templates render them, read by `islands/test/*.test.ts`.
//!
//! Entry ids are random, so every recorded id is replaced by `ENTRY0`, `ENTRY1`, ... in order of
//! first appearance. `UPDATE_GOLDEN=1 cargo test -p netray-web --test result_islands_fixtures`
//! rewrites the four files under `islands/test/fixtures/`.
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{
    DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunRefusal, RunStart, RunTrigger,
};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const FIRST_AT: i64 = 1_760_000_000;
const RUN_AT: i64 = 1_760_000_900;
const ORDER: [(Protocol, &str); 5] = [
    (Protocol::Dns, "dns.ns"),
    (Protocol::Tls, "tls.chain_trusted"),
    (Protocol::Http, "http.hsts"),
    (Protocol::Email, "email.spf"),
    (Protocol::Ip, "ip.reputation"),
];

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

async fn record(store: &HistoryStore, run: &Run, at: i64) {
    store
        .record(DOMAIN, run, &[], at, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
}

#[derive(Clone, Copy)]
enum Mode {
    Admit,
    RefuseBudget,
}

struct StubRuns {
    store: Arc<HistoryStore>,
    mode: Mode,
}

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
        Box::pin(async move {
            if let Mode::RefuseBudget = self.mode {
                return RunStart::Refused(RunRefusal {
                    reason: RefusalReason::Budget,
                    retry_after_s: 2,
                    last_run_ago_s: None,
                    last_incomplete: false,
                });
            }
            let mut events: Vec<RunEvent> = ORDER
                .iter()
                .map(|(protocol, id)| {
                    RunEvent::Section(SectionEvent {
                        protocol: *protocol,
                        outcome: SectionOutcome::Measured {
                            checks: vec![CheckResult {
                                id: CheckId::parse(id).unwrap(),
                                status: Status::Warn,
                                findings: vec![],
                                evidence: vec![],
                            }],
                            evidence: vec![],
                        },
                    })
                })
                .collect();
            let run = run_with(Grade::B, Status::Warn);
            let recorded = self
                .store
                .record(DOMAIN, &run, &[], RUN_AT, |p, c| diff(p, c, &[], None))
                .await
                .unwrap();
            events.push(RunEvent::Done(run));
            events.push(RunEvent::Recorded(Some(recorded)));
            RunStart::Admitted(Box::pin(futures::stream::iter(events)))
        })
    }
}

struct Fixture {
    app: Router,
    store: Arc<HistoryStore>,
    _dir: tempfile::TempDir,
}

/// One recorded entry at `FIRST_AT`; `ttl` decides stale or fresh against the real clock.
async fn fixture(mode: Mode, ttl: u64) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    record(&store, &run_with(Grade::A, Status::Pass), FIRST_AT).await;
    let runs = Arc::new(StubRuns {
        store: store.clone(),
        mode,
    });
    let app = router(ResultState::new(store.clone(), runs, ttl));
    Fixture {
        app,
        store,
        _dir: dir,
    }
}

async fn body_of(app: &Router, req: Request<Body>) -> String {
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

async fn get_main(f: &Fixture) -> String {
    let req = Request::get(format!("/{DOMAIN}"))
        .header("accept", "text/html")
        .body(Body::empty())
        .unwrap();
    let html = body_of(&f.app, req).await;
    let start = html.find("<main>").expect("<main>") + "<main>".len();
    let end = html.find("</main>").expect("</main>");
    html[start..end].to_string()
}

async fn post_run(f: &Fixture) -> String {
    let req = Request::post(format!("/{DOMAIN}/run"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/event-stream")
        .body(Body::from("trigger=visit"))
        .unwrap();
    body_of(&f.app, req).await
}

/// Replace every stored entry id with `ENTRY<n>`, numbered by first appearance in `text`, and the
/// wall-clock minute of the live run with `NOW`.
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
    // The live run's trace row carries the wall clock; every fixed fixture time lies in 2025.
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
async fn result_stale_markup_matches_fixture() {
    let f = fixture(Mode::Admit, 300).await;
    let got = normalise(&f.store, get_main(&f).await).await;
    assert!(got.contains(r#"data-island="stream""#), "{got}");
    check("result-stale.html", &got);
}

#[tokio::test]
async fn result_fresh_markup_matches_fixture() {
    let f = fixture(Mode::Admit, u64::MAX / 2).await;
    let got = normalise(&f.store, get_main(&f).await).await;
    assert!(got.contains(r#"data-island="stream""#), "{got}");
    check("result-fresh.html", &got);
}

#[tokio::test]
async fn result_run_stream_matches_fixture() {
    let f = fixture(Mode::Admit, 300).await;
    let got = normalise(&f.store, post_run(&f).await).await;
    assert!(got.contains("event: end"), "{got}");
    check("result-run.sse", &got);
}

#[tokio::test]
async fn result_refused_stream_matches_fixture() {
    let f = fixture(Mode::RefuseBudget, 300).await;
    let got = normalise(&f.store, post_run(&f).await).await;
    assert!(got.contains("event: end"), "{got}");
    check("result-refused.sse", &got);
}
