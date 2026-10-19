//! The trace of the result page (spec v2-result-fidelity, Phase 3; requirements 20 and 24;
//! criteria C1, C5, C9, C10, C11, C15): one cell per spectrum line, edge dates, a table link.
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
            .record("example.com", &run(i), &[], T + i as i64 * DAY, |p, c| {
                diff(p, c, &[], None)
            })
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

const ROW_CLASS: &str = "trace__row";
const CELL_CLASS: &str = "trace__cell";
const EDGE_CLASS: &str = "trace__edge";
const LINK_CLASS: &str = "trace__table-link";
const TRACE_CSS: &str = include_str!("../../static/trace.css");

/// The rows of `<ol class="trace">` as chunks, newest first.
fn trace_rows(body: &str) -> Vec<&str> {
    let ol = body
        .split("<ol class=\"trace\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no ol.trace in {body}"));
    let ol = ol.split("</ol>").next().unwrap();
    ol.split("<li").skip(1).collect()
}

/// One cell per spectrum line: the targets of the first run of each band.
fn line_count(body: &str) -> usize {
    body.matches("class=\"spectrum__target\"").count()
}

async fn page(n: usize) -> String {
    let (app, _store, _dir) = app(n).await;
    let (status, body) = get(&app, "/example.com").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn c9_twelve_entries_give_twelve_rows_with_a_cell_per_line_and_edge_dates() {
    let body = page(12).await;
    let rows = trace_rows(&body);
    let lines = line_count(&body);

    assert!(lines > 0, "no spectrum lines in {body}");
    assert_eq!(rows.len(), 12);
    for (k, row) in rows.iter().enumerate() {
        assert!(row.contains(ROW_CLASS), "row {k}: {row}");
        assert!(
            row.contains(&format!("data-run=\"{k}\"")),
            "row {k} is not data-run {k}: {row}"
        );
        assert_eq!(
            row.matches(&format!("class=\"{CELL_CLASS}\"")).count(),
            lines,
            "row {k}: {row}"
        );
        assert!(!row.contains("trace__date"), "row {k}: {row}");
        assert!(!row.contains("trace__grade"), "row {k}: {row}");
    }
    // entry 11 is T + 11 days = 2027-01-26, entry 0 is 2027-01-15
    assert!(
        body.contains(&format!(
            "<span class=\"{EDGE_CLASS}\" data-edge=\"newest\">01-26</span>"
        )),
        "{body}"
    );
    assert!(
        body.contains(&format!(
            "<span class=\"{EDGE_CLASS}\" data-edge=\"oldest\">01-15</span>"
        )),
        "{body}"
    );
}

#[tokio::test]
async fn c10_thirty_entries_give_twenty_four_rows() {
    let body = page(30).await;

    assert_eq!(trace_rows(&body).len(), 24);
}

#[tokio::test]
async fn c11_pass_and_warn_cells_carry_their_tone() {
    let body = page(12).await;
    let rows = trace_rows(&body);

    // entry 11 (odd) warns on both checks, entry 10 (even) passes on both
    assert!(rows[0].contains("data-tone=\"warn\""), "{}", rows[0]);
    assert!(!rows[0].contains("data-tone=\"pass\""), "{}", rows[0]);
    assert!(rows[1].contains("data-tone=\"pass\""), "{}", rows[1]);
    assert!(!rows[1].contains("data-tone=\"warn\""), "{}", rows[1]);
}

/// First declaration of `prop` in the first rule whose selector list is exactly one of `selectors`.
fn decl(selectors: &[&str], prop: &str) -> Option<String> {
    for wanted in selectors {
        let mut rest = TRACE_CSS;
        while let Some(open) = rest.find('{') {
            let sel = rest[..open].rsplit('}').next().unwrap().trim();
            let close = open + rest[open..].find('}').unwrap();
            if sel.split(',').any(|s| s.trim() == *wanted) {
                for d in rest[open + 1..close].split(';') {
                    if let Some((k, v)) = d.split_once(':')
                        && k.trim() == prop
                    {
                        return Some(v.trim().to_string());
                    }
                }
            }
            rest = &rest[close + 1..];
        }
    }
    None
}

/// Resolves `var(--x)` through the generated tokens, repeatedly.
fn resolve(value: &str) -> String {
    let mut v = value.to_string();
    while let Some(name) = v.strip_prefix("var(--").and_then(|s| s.strip_suffix(')')) {
        let css = netray_web::tokens::CSS;
        let at = css
            .find(&format!("--{name}:"))
            .unwrap_or_else(|| panic!("no token {name}"));
        let rest = &css[at + name.len() + 3..];
        v = rest.split(';').next().unwrap().trim().to_string();
    }
    v
}

fn mark(tone: &str, prop: &str) -> String {
    let before = format!(".trace__cell[data-tone=\"{tone}\"]::before");
    let cell = format!(".trace__cell[data-tone=\"{tone}\"]");
    let value = decl(
        &[&before, &cell, ".trace__cell::before", ".trace__cell"],
        prop,
    )
    .unwrap_or_else(|| panic!("no {prop} for a {tone} mark"));
    resolve(&value)
}

#[test]
fn c1_trace_css_sizes_rows_and_marks() {
    let row = decl(&[".trace__row"], "height").expect("trace row has no height");
    assert_eq!(resolve(&row), "5px");

    for tone in ["pass", "warn", "fail"] {
        assert_eq!(mark(tone, "width"), "5px", "{tone} width");
    }
    assert_eq!(mark("pass", "height"), "1px");
    assert_eq!(mark("warn", "height"), "3px");
    assert_eq!(mark("fail", "height"), "3px");

    let hair = resolve("var(--status-pass-hair)");
    let warn = resolve("var(--status-warn-mark)");
    let fail = resolve("var(--status-fail-mark)");
    let untested = resolve("var(--status-untested-mark)");
    assert_eq!(mark("pass", "background"), hair);
    assert_eq!(mark("warn", "background"), warn);
    assert_eq!(mark("fail", "background"), fail);
    assert_eq!(mark("untested", "background"), untested);
}

#[tokio::test]
async fn c15_table_link_follows_the_trace_inside_the_spectrum() {
    let body = page(12).await;
    let link = format!(
        "<a class=\"{LINK_CLASS}\" href=\"/example.com/history\">Show history as a table</a>"
    );

    let figure = body
        .split("<figure id=\"spectrum\"")
        .nth(1)
        .expect("no #spectrum");
    let figure = figure.split("</figure>").next().unwrap();
    let trace_at = figure.find("<ol class=\"trace\"").expect("no trace");
    let link_at = figure
        .find(&link)
        .unwrap_or_else(|| panic!("no link in {figure}"));
    assert!(link_at > trace_at, "link before the trace");
}
