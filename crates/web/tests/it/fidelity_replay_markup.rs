//! Replay markup, server half (spec v2-result-fidelity, Phase 3; requirements 21-23): the trace
//! caption and hint with their rest text, the gutter, the per-row replay data and the per-run
//! sets the island switches. `GET /{domain}` on a domain with 12 entries.
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

const DOMAIN: &str = "shop.example.com";
const T: i64 = 1_800_000_000;
const DAY: i64 = 86_400;
const CAPTION: &str = "<p class=\"trace__caption\" data-rest=\"Each line carries its trace: 12 runs, newest on top.\">Each line carries its trace: 12 runs, newest on top.</p>";
const HINT: &str = "<p class=\"trace__hint\" data-rest=\"hover a run to replay the spectrum · tap to pin\">hover a run to replay the spectrum · tap to pin</p>";
const GUTTER_OPEN: &str = "<div id=\"spectrum-gutter\" class=\"spectrum-gutter\">";
const GUTTER_RUN: &str = "<span class=\"spectrum-gutter__run\">Run 12</span>";
const GUTTER_DATE: &str = "<span class=\"spectrum-gutter__date\">2027-01-26</span>";
const GUTTER_GRADE: &str = "<span class=\"spectrum-gutter__grade\">grade <strong>B</strong></span>";

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
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
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.ns").unwrap(),
            status,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

async fn page() -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for i in 0..12 {
        store
            .record(DOMAIN, &run(i), &[], T + i as i64 * DAY, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    let app: Router = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn spectrum(html: &str) -> &str {
    let start = html
        .find("<figure id=\"spectrum\"")
        .unwrap_or_else(|| panic!("no figure#spectrum in {html}"));
    let rest = &html[start..];
    &rest[..rest.find("</figure>").unwrap_or(rest.len())]
}

#[tokio::test]
async fn c12_caption_and_hint_carry_their_rest_text() {
    let html = page().await;
    let s = spectrum(&html);
    assert!(s.contains(CAPTION), "{s}");
    assert!(s.contains(HINT), "{s}");
}

#[tokio::test]
async fn c13_gutter_names_the_newest_run() {
    let html = page().await;
    let s = spectrum(&html);
    let gutter = &s[s
        .find(GUTTER_OPEN)
        .unwrap_or_else(|| panic!("no gutter in {s}"))..];
    let gutter = &gutter[..gutter.find("</div>").unwrap_or(gutter.len())];
    for part in [GUTTER_RUN, GUTTER_DATE, GUTTER_GRADE] {
        assert!(gutter.contains(part), "{part} missing in {gutter}");
    }
}

#[tokio::test]
async fn c13_every_trace_row_carries_its_run_number_date_and_grade() {
    let html = page().await;
    let s = spectrum(&html);
    for (k, date) in [(12, "2027-01-26"), (5, "2027-01-19"), (1, "2027-01-15")] {
        let at = s
            .find(&format!("data-run-no=\"{k}\""))
            .unwrap_or_else(|| panic!("no data-run-no={k} in {s}"));
        let open = s[..at].rfind("<li").unwrap();
        let tag = &s[open..open + s[open..].find('>').unwrap()];
        assert!(tag.contains("trace__row"), "{tag}");
        assert!(tag.contains(&format!("data-date=\"{date}\"")), "{tag}");
        assert!(tag.contains("data-grade=\"B\""), "{tag}");
    }
    assert_eq!(s.matches("data-run-no=").count(), 12, "{s}");
}

#[tokio::test]
async fn c13_region_grades_and_legend_fixed_exist_per_run() {
    let html = page().await;
    let s = spectrum(&html);
    for (marker, name) in [
        ("spectrum__region-grades", "region grades"),
        ("legend__fixed", "legend fixed"),
    ] {
        let n = s
            .split('<')
            .filter(|t| t.contains(marker) && t.contains("data-run="))
            .count();
        assert_eq!(n, 12, "{name}: one [data-run] element per run in {s}");
    }
}
