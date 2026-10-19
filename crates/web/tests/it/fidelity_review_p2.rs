//! Result fidelity Phase 2, review findings: the last axis tick and the region labels stay inside
//! the plate's box, region grades follow the score guard, the legend counts the drawn lines and
//! the labels are measured at the size they draw at.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::history::score_run;
use netray_engine::profile::Profiles;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use netray_web::spectrum::label_width;
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const CSS: &str = include_str!("../../static/spectrum.css");

fn check(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn section(protocol: Protocol) -> RunSection {
    RunSection {
        protocol,
        outcome: SectionKind::Measured,
        reason: None,
    }
}

/// A hostname-match failure is a hard fail: the engine grades the run F.
fn hard_fail_run(grade: Grade) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![section(Protocol::Tls)],
        checks: vec![
            check("tls.chain_trusted", Status::Pass),
            check("tls.hostname_match", Status::Fail),
        ],
        blocks: vec![],
    }
}

struct Runs(Option<Run>);

impl DomainRuns for Runs {
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
        match self.0.clone() {
            Some(done) => Box::pin(async move {
                let events = vec![RunEvent::Done(done), RunEvent::Recorded(None)];
                RunStart::Admitted(Box::pin(futures::stream::iter(events)))
            }),
            None => Box::pin(async { RunStart::NotStarted }),
        }
    }
}

async fn store_with(dir: &tempfile::TempDir, stored: &[Run]) -> Arc<HistoryStore> {
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for (i, run) in stored.iter().enumerate() {
        store
            .record(DOMAIN, run, &[], 1_800_000_000 + i as i64, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    Arc::new(store)
}

async fn page(stored: &[Run]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(&dir, stored).await;
    let app: Router = router(ResultState::new(store, Arc::new(Runs(None)), 300));
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

/// The `spectrum` fragments of a streamed run, in order.
async fn streamed_spectra(done: Run) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(&dir, &[]).await;
    let app: Router = router(ResultState::new(store, Arc::new(Runs(Some(done))), 300));
    let resp = app
        .oneshot(
            Request::post(format!("/{DOMAIN}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=visit"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut out = Vec::new();
    for raw in text.split("\n\n") {
        let mut name = "";
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        if name == "fragment" {
            let j: serde_json::Value = serde_json::from_str(&data).unwrap();
            if j["id"] == "spectrum" {
                out.push(j["html"].as_str().unwrap().to_owned());
            }
        }
    }
    out
}

fn rule<'a>(selector: &str) -> &'a str {
    let start = CSS
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("no rule for {selector}"));
    let rest = &CSS[start..];
    &rest[..rest.find('}').unwrap()]
}

fn lines_drawn(html: &str) -> usize {
    html.matches("data-target=").count()
}

fn note_count(html: &str) -> usize {
    let at = html
        .find("<span class=\"legend__note\">")
        .unwrap_or_else(|| panic!("no key note in {html}"));
    html[at + 27..]
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

// Finding 1: the last tick ends at the plate's right edge

#[test]
fn the_last_tick_is_pulled_back_by_its_own_width() {
    let r = rule(".spectrum__tick[data-last]");
    assert!(r.contains("transform: translateX(-100%)"), "{r}");
}

#[tokio::test]
async fn only_the_tick_at_100_percent_is_marked_last() {
    let html = page(&[hard_fail_run(Grade::F)]).await;
    let axis = &html[html.find("class=\"spectrum__axis\"").unwrap()..];
    let ticks: Vec<&str> = axis
        .split("<span class=\"spectrum__tick\"")
        .skip(1)
        .map(|t| &t[..t.find('>').unwrap()])
        .collect();
    assert_eq!(
        ticks.iter().filter(|t| t.contains("data-last")).count(),
        1,
        "{ticks:?}"
    );
    let last = ticks.last().unwrap();
    assert!(
        last.contains("data-last") && last.contains("left: 100.0000%"),
        "{last}"
    );
}

// Finding 2: region labels centre both ways and the last one ends at the plate's edge

#[test]
fn region_labels_overflow_symmetrically_and_the_last_ends_at_the_edge() {
    let r = rule(".spectrum__region-label");
    for want in [
        "display: flex",
        "justify-content: center",
        "white-space: nowrap",
    ] {
        assert!(r.contains(want), "{want} missing: {r}");
    }
    let last = rule(".spectrum__region-label:last-child");
    assert!(last.contains("justify-content: flex-end"), "{last}");
}

#[tokio::test]
async fn the_last_region_label_is_the_last_child_of_the_axis() {
    let html = page(&[hard_fail_run(Grade::F)]).await;
    let axis = &html[html.find("class=\"spectrum__axis\"").unwrap()..];
    let axis = &axis[..axis.find("</div>").unwrap()];
    let last_open = axis.rfind("<span ").unwrap();
    assert!(
        axis[last_open..].starts_with("<span class=\"spectrum__region-label\""),
        "{axis}"
    );
}

// Finding 3: region grades follow the score guard

fn disputed() -> (Grade, Grade) {
    let run = hard_fail_run(Grade::A);
    let computed = score_run(&run, Profiles::get(3).unwrap()).grade;
    assert_eq!(computed, Grade::F);
    (computed, Grade::A)
}

#[tokio::test]
async fn region_grades_show_when_the_stored_grade_is_the_engines() {
    let (computed, _) = disputed();
    let html = page(&[hard_fail_run(computed)]).await;
    assert!(html.contains("spectrum__region-grade"), "{html}");
}

#[tokio::test]
async fn no_region_grade_under_a_disputed_stored_grade() {
    let (_, stored) = disputed();
    let html = page(&[hard_fail_run(stored)]).await;
    assert!(html.contains("spectrum__region-label"), "{html}");
    assert!(!html.contains("spectrum__region-grade"), "{html}");
}

#[tokio::test]
async fn no_region_grade_on_an_unrecorded_run_under_a_disputed_letter() {
    let (_, stored) = disputed();
    let spectra = streamed_spectra(hard_fail_run(stored)).await;
    let last = spectra.last().unwrap();
    assert!(last.contains("spectrum__region-label"), "{last}");
    assert!(!last.contains("spectrum__region-grade"), "{last}");
}

// Finding 4: the key note counts the lines the spectrum draws

#[tokio::test]
async fn the_key_note_counts_the_drawn_lines_not_the_run_checks() {
    let html = page(&[hard_fail_run(Grade::F)]).await;
    let drawn = lines_drawn(&html);
    assert!(drawn > 2, "{html}");
    assert_eq!(note_count(&html), drawn, "{html}");
}

#[tokio::test]
async fn the_live_stream_key_note_counts_every_drawn_line() {
    let spectra = streamed_spectra(hard_fail_run(Grade::F)).await;
    let first = &spectra[0];
    assert_eq!(note_count(first), lines_drawn(first), "{first}");
    assert!(note_count(first) > 2, "{first}");
}

#[tokio::test]
async fn the_never_checked_page_has_no_key_note() {
    let html = page(&[]).await;
    assert!(!html.contains("legend__note"), "{html}");
    assert!(!html.contains(" 0 checks"), "{html}");
}

// Finding 5: labels are measured at the caption size

#[test]
fn label_widths_are_measured_at_11px() {
    assert!((label_width(4) - (4.0 * 0.62 * 11.0 + 4.0)).abs() < 1e-9);
    assert!((label_width(0) - 4.0).abs() < 1e-9);
}
