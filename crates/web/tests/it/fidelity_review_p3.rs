//! Result fidelity Phase 3, review findings: the pinned-row marker, the gutter's run slot, the
//! legend of the oldest row in the layout window, the caption's line names and the grade change
//! of a run that was not stored.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::{Diff, GradeChange, diff};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::views::line_code;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const TRACE_CSS: &str = include_str!("../../static/trace.css");
// 2027-01-15 08:00:00 UTC
const T: i64 = 1_800_000_000;
const SHOWN: usize = 24;

fn run(i: usize, complete: bool) -> Run {
    let (grade, status) = if i % 2 == 0 {
        (Grade::F, Status::Fail)
    } else {
        (Grade::B, Status::Pass)
    };
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![
            RunCheck {
                id: CheckId::parse("tls.chain_trusted").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            },
            RunCheck {
                id: CheckId::parse("tls.hostname_match").unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            },
        ],
        blocks: vec![],
    }
}

struct StubRuns {
    done: Option<Run>,
    recorded: Option<Recorded>,
}

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
        match self.done.clone() {
            Some(done) => {
                let events = vec![
                    RunEvent::Done(done),
                    RunEvent::Recorded(self.recorded.clone()),
                ];
                Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
            }
            None => Box::pin(async { RunStart::NotStarted }),
        }
    }
}

async fn store_of(dir: &tempfile::TempDir, stored: usize) -> Arc<HistoryStore> {
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for i in 0..stored {
        store
            .record(DOMAIN, &run(i, true), &[], T + i as i64 * 86_400, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    Arc::new(store)
}

async fn page(stored: usize) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = store_of(&dir, stored).await;
    let app: Router = router(ResultState::new(
        store,
        Arc::new(StubRuns {
            done: None,
            recorded: None,
        }),
        300,
    ));
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

/// (id, html) of every fragment of a streamed run over `stored` entries.
async fn stream(stored: usize, done: Run, recorded: Option<Recorded>) -> Vec<(String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let store = store_of(&dir, stored).await;
    let app: Router = router(ResultState::new(
        store,
        Arc::new(StubRuns {
            done: Some(done),
            recorded,
        }),
        0,
    ));
    let resp = app
        .oneshot(
            Request::post(format!("/{DOMAIN}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::from("trigger=recheck"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut out = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut name = String::new();
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        if name == "fragment" {
            let j: serde_json::Value = serde_json::from_str(&data).unwrap();
            out.push((
                j["id"].as_str().unwrap().to_owned(),
                j["html"].as_str().unwrap().to_owned(),
            ));
        }
    }
    out
}

fn last_of<'a>(fragments: &'a [(String, String)], id: &str) -> &'a str {
    &fragments
        .iter()
        .rev()
        .find(|(i, _)| i == id)
        .unwrap_or_else(|| panic!("no fragment {id}"))
        .1
}

/// The legend entry of run `k`, its markup between `<li ...>` and `</li>`.
fn legend_entry(html: &str, k: usize) -> &str {
    let open = format!("<li class=\"legend__fixed\" data-run=\"{k}\"");
    let at = html
        .find(&open)
        .unwrap_or_else(|| panic!("no legend entry {k} in {html}"));
    let rest = &html[at..];
    let body = &rest[rest.find('>').unwrap() + 1..];
    &body[..body.find("</li>").unwrap()]
}

fn rule<'a>(selector: &str) -> &'a str {
    let start = TRACE_CSS
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("no rule for {selector}"));
    let rest = &TRACE_CSS[start..];
    &rest[..rest.find('}').unwrap()]
}

// Finding 2: the replayed or pinned trace row carries a 3 px ink marker

#[test]
fn a_hovered_or_pinned_row_draws_a_marker_outside_the_trace() {
    let r = rule(".trace__row:is([data-hovered], [data-pinned])::before");
    for want in [
        "content: \"\"",
        "position: absolute",
        "inset-inline-start: -12px",
        "width: 3px",
        "height: 9px",
        "background: var(--color-ink)",
    ] {
        assert!(r.contains(want), "{want} missing: {r}");
    }
    assert!(rule(".trace__row").contains("position: relative"));
}

#[test]
fn a_row_at_rest_has_no_marker() {
    assert_eq!(
        TRACE_CSS.matches(".trace__row::before").count(),
        0,
        "{TRACE_CSS}"
    );
    assert!(!TRACE_CSS.contains(".trace__row:not("), "{TRACE_CSS}");
}

// Finding 3: the gutter always carries its run slot

#[tokio::test]
async fn the_gutter_of_a_run_without_a_number_still_has_a_run_slot() {
    let fragments = stream(2, run(5, true), None).await;
    let spectrum = last_of(&fragments, "spectrum");
    assert!(
        spectrum.contains("<span class=\"spectrum-gutter__run\"></span>"),
        "{spectrum}"
    );
}

#[tokio::test]
async fn the_gutter_of_a_numbered_run_names_it() {
    let html = page(3).await;
    assert!(
        html.contains("<span class=\"spectrum-gutter__run\">Run 3</span>"),
        "{html}"
    );
}

// Finding 4: the oldest row of the window has no predecessor to be fixed since

#[tokio::test]
async fn the_oldest_stored_row_of_a_full_window_names_no_fixed_since() {
    let html = page(SHOWN + 2).await;
    assert!(
        legend_entry(&html, SHOWN - 1).is_empty(),
        "{}",
        legend_entry(&html, SHOWN - 1)
    );
    assert!(
        legend_entry(&html, SHOWN - 2).contains("fixed since run"),
        "{}",
        legend_entry(&html, SHOWN - 2)
    );
}

#[tokio::test]
async fn the_oldest_stored_row_of_a_short_history_names_no_fixed_since() {
    let html = page(3).await;
    assert!(legend_entry(&html, 2).is_empty());
    assert!(legend_entry(&html, 1).contains("fixed since run 1"));
}

#[tokio::test]
async fn the_oldest_row_of_a_finished_run_over_a_full_window_names_no_fixed_since() {
    let fragments = stream(SHOWN + 2, run(0, true), None).await;
    let spectrum = last_of(&fragments, "spectrum");
    assert!(
        legend_entry(spectrum, SHOWN - 1).is_empty(),
        "{}",
        legend_entry(spectrum, SHOWN - 1)
    );
    assert!(
        legend_entry(spectrum, SHOWN - 2).contains("fixed since run"),
        "{}",
        legend_entry(spectrum, SHOWN - 2)
    );
    assert!(legend_entry(spectrum, 0).contains("fixed since run"));
}

// Finding 5: the caption names a line by its short label and line code

#[tokio::test]
async fn each_sentence_item_carries_the_short_label_and_line_code() {
    let html = page(2).await;
    let id = CheckId::parse("tls.hostname_match").unwrap();
    let short = &catalogue().get(&id).unwrap().short;
    let want = format!(
        "<li data-code=\"tls.hostname_match\" data-name=\"{short} {}\"",
        line_code(&id).unwrap()
    );
    assert!(html.contains(&want), "want {want} in {html}");
}

#[tokio::test]
async fn every_catalogue_line_has_a_named_sentence_item() {
    let html = page(2).await;
    let items = html.matches("<li data-code=").count();
    assert_eq!(items, catalogue().entries().len());
    let names = html
        .split("<li data-code=")
        .skip(1)
        .filter(|li| li[..li.find('>').unwrap()].contains("data-name=\""))
        .count();
    assert_eq!(names, items);
}

// Finding 6: a run that was not stored keeps its grade change

#[tokio::test]
async fn a_run_not_stored_shows_the_grade_change_with_the_latest_stored_date() {
    let change = Diff {
        checks: vec![],
        evidence: vec![],
        grade: GradeChange {
            from: Grade::B,
            rescored: None,
            to: Grade::F,
        },
        engine_changed: false,
    };
    let fragments = stream(
        1,
        run(2, false),
        Some(Recorded::NotStored { diff: Some(change) }),
    )
    .await;
    let all: String = fragments.iter().map(|(_, h)| h.as_str()).collect();
    assert!(
        all.contains("<p class=\"since-meta\">2027-01-15 \u{b7} grade B \u{2192} F</p>"),
        "{all}"
    );
}

#[tokio::test]
async fn a_run_not_stored_without_a_diff_has_no_grade_meta() {
    let fragments = stream(1, run(2, false), Some(Recorded::NotStored { diff: None })).await;
    let all: String = fragments.iter().map(|(_, h)| h.as_str()).collect();
    assert!(!all.contains("since-meta"), "{all}");
}
