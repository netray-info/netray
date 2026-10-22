//! Result fidelity Phase 4, review findings: the since text over the whole history, the clean
//! bar's count, the attention heading over listed rows, the board's grouping order, the
//! grouping switch before its island is ready, and the Email label.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Severity, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunEvent, RunStart, RunTrigger};
use netray_web::pages::result::views::{Grouping, Links, issues};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "shop.example.com";
const MISSING_ID: &str = "Zz9yXw8v";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn check(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn run_of(checks: Vec<RunCheck>) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: [
            Protocol::Dns,
            Protocol::Tls,
            Protocol::Http,
            Protocol::Email,
            Protocol::Ip,
        ]
        .into_iter()
        .map(|protocol| RunSection {
            protocol,
            outcome: SectionKind::Measured,
            reason: None,
        })
        .collect(),
        checks,
        blocks: vec![],
    }
}

struct NoRuns;

impl DomainRuns for NoRuns {
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

struct StubRuns {
    done: Run,
    recorded: Recorded,
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
        let events = vec![
            RunEvent::Done(self.done.clone()),
            RunEvent::Recorded(Some(self.recorded.clone())),
        ];
        Box::pin(async move { RunStart::Admitted(Box::pin(futures::stream::iter(events))) })
    }
}

async fn store_of(runs: &[Run]) -> Arc<HistoryStore> {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let base = now() - 100_000;
    for (i, run) in runs.iter().enumerate() {
        let recorded = store
            .record(DOMAIN, run, &[], base + i as i64 * 60, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
        assert!(
            matches!(recorded, Recorded::First { .. } | Recorded::Changed { .. }),
            "run {i} stored as {recorded:?}"
        );
    }
    std::mem::forget(dir);
    Arc::new(store)
}

async fn page(store: Arc<HistoryStore>) -> String {
    let app: Router = router(ResultState::new(store, Arc::new(NoRuns), 300));
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

/// The last fragment `id` of the streamed run.
async fn streamed(store: Arc<HistoryStore>, done: Run, recorded: Recorded, id: &str) -> String {
    let app: Router = router(ResultState::new(
        store,
        Arc::new(StubRuns { done, recorded }),
        300,
    ));
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
    let mut found = None;
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
            if j["id"] == id {
                found = Some(j["html"].as_str().unwrap().to_owned());
            }
        }
    }
    found.unwrap_or_else(|| panic!("no fragment {id} in {text}"))
}

/// The since text of the row of `code` in `html`, if it has one.
fn since_of(html: &str, code: &str) -> Option<String> {
    let row = html
        .split(r#"<article class="check-row""#)
        .skip(1)
        .find(|r| r.contains(code))
        .unwrap_or_else(|| panic!("no {code} row in {html}"));
    let marker = r#"<span class="check-row__since">"#;
    let at = row.find(marker)? + marker.len();
    Some(row[at..at + row[at..].find("</span>").unwrap()].to_string())
}

/// Runs whose dns.caa follows `caa`; dns.dnssec_valid alternates so that no run equals its
/// predecessor.
fn history(caa: impl Fn(usize) -> Status) -> Vec<Run> {
    (1..=30)
        .map(|n| {
            let other = if n % 2 == 0 {
                Status::Pass
            } else {
                Status::Warn
            };
            run_of(vec![
                check("dns.caa", caa(n)),
                check("dns.dnssec_valid", other),
            ])
        })
        .collect()
}

// Finding 1: since over every stored entry

#[tokio::test]
async fn since_counts_runs_beyond_the_24_entry_window() {
    let store = store_of(&history(
        |n| if n <= 6 { Status::Pass } else { Status::Warn },
    ))
    .await;

    let body = page(store).await;

    assert_eq!(
        since_of(&body, "dns.caa").as_deref(),
        Some("lit up in run 7")
    );
}

#[tokio::test]
async fn since_says_all_30_runs_when_the_check_warned_in_every_stored_entry() {
    let store = store_of(&history(|_| Status::Warn)).await;

    let body = page(store).await;

    assert_eq!(
        since_of(&body, "dns.caa").as_deref(),
        Some("in all 30 runs")
    );
}

// Finding 2: no since text without an ordinal

#[tokio::test]
async fn a_record_missing_from_the_re_read_gets_no_since_text() {
    let older = run_of(vec![check("dns.caa", Status::Pass)]);
    let done = run_of(vec![check("dns.caa", Status::Warn)]);
    let store = store_of(std::slice::from_ref(&older)).await;
    let recorded = Recorded::Changed {
        id: MISSING_ID.to_string(),
        diff: diff(&older, &done, &[], None),
    };

    let html = streamed(store, done, recorded, "issues").await;

    assert!(html.contains("dns.caa"), "{html}");
    assert!(!html.contains("check-row__since"), "{html}");
}

// Finding 3: the clean bar counts clean lines only

fn clean_bar(body: &str) -> &str {
    let at = body.find(r#"<details id="passed""#).unwrap();
    &body[at..at + body[at..].find("</details>").unwrap() + "</details>".len()]
}

#[tokio::test]
async fn the_clean_bar_counts_pass_and_not_applicable_but_lists_every_row() {
    let ids: Vec<String> = catalogue()
        .entries()
        .iter()
        .take(5)
        .map(|s| s.id.to_string())
        .collect();
    let statuses = [
        Status::Pass,
        Status::Pass,
        Status::NotApplicable,
        Status::Unmeasured,
        Status::NotTested,
    ];
    let checks = ids
        .iter()
        .zip(statuses)
        .map(|(id, status)| check(id, status))
        .collect();
    let store = store_of(&[run_of(checks)]).await;

    let body = page(store).await;

    let bar = clean_bar(&body);
    assert!(bar.contains("3 lines clean</strong>"), "{bar}");
    assert_eq!(bar.matches(r#"class="check-row""#).count(), 5, "{bar}");
}

#[tokio::test]
async fn a_never_checked_domain_has_an_empty_hidden_clean_bar() {
    let body = page(store_of(&[]).await).await;

    assert!(
        body.contains(r#"<details id="passed" class="clean-bar" hidden></details>"#),
        "{body}"
    );
    assert!(!body.contains("0 lines clean"), "{body}");
}

// Finding 4: the heading, the meta and the groups agree

#[tokio::test]
async fn the_attention_heading_counts_listed_rows_only() {
    let run = run_of(vec![
        check("http.hsts", Status::Fail),
        check("dns.caa", Status::Warn),
        check("dns.legacy_check", Status::Warn),
    ]);

    let body = page(store_of(&[run]).await).await;

    assert!(body.contains("<h2>2 lines need attention</h2>"), "{body}");
    assert!(body.contains("1 fail · 1 warn"), "{body}");
}

#[tokio::test]
async fn no_attention_head_when_only_unlisted_checks_are_open() {
    let run = run_of(vec![
        check("dns.caa", Status::Pass),
        check("dns.legacy_check", Status::Warn),
    ]);

    let body = page(store_of(&[run]).await).await;

    assert!(!body.contains("need attention"), "{body}");
    assert!(!body.contains("attention-meta"), "{body}");
}

// Finding 5: groups follow the board

fn rank(severity: Severity) -> usize {
    match severity {
        Severity::Critical => 0,
        Severity::High => 1,
        Severity::Medium => 2,
        Severity::Low => 3,
    }
}

fn groups_of(run: &Run, grouping: Grouping) -> Vec<(String, Vec<String>)> {
    issues(run, catalogue(), Links::Live, grouping, &Default::default())
        .into_iter()
        .map(|g| (g.key, g.rows.into_iter().map(|r| r.view.code).collect()))
        .collect()
}

#[test]
fn a_fail_row_leads_its_group_and_a_failing_group_leads_the_page() {
    let specs = catalogue().entries();
    let (warned, failed) = specs
        .iter()
        .flat_map(|a| specs.iter().map(move |b| (a, b)))
        .find(|(a, b)| {
            rank(a.severity) < rank(b.severity)
                && a.fix_owner != b.fix_owner
                && a.protocol != b.protocol
        })
        .expect("a pair that differs in severity, owner and protocol");
    let run = run_of(vec![
        check(&warned.id.to_string(), Status::Warn),
        check(&failed.id.to_string(), Status::Fail),
    ]);

    for grouping in [Grouping::Owner, Grouping::Severity, Grouping::Protocol] {
        let groups = groups_of(&run, grouping);
        assert_eq!(groups.len(), 2, "{grouping:?}");
        assert_eq!(groups[0].1, vec![failed.id.to_string()], "{grouping:?}");
        assert_eq!(groups[1].1, vec![warned.id.to_string()], "{grouping:?}");
    }
}

#[test]
fn inside_a_group_a_fail_row_precedes_a_warn_row_of_higher_severity() {
    let specs = catalogue().entries();
    let (warned, failed) = specs
        .iter()
        .flat_map(|a| specs.iter().map(move |b| (a, b)))
        .find(|(a, b)| rank(a.severity) < rank(b.severity) && a.fix_owner == b.fix_owner)
        .expect("a pair of one owner that differs in severity");
    let run = run_of(vec![
        check(&warned.id.to_string(), Status::Warn),
        check(&failed.id.to_string(), Status::Fail),
    ]);

    let groups = groups_of(&run, Grouping::Owner);

    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].1,
        vec![failed.id.to_string(), warned.id.to_string()]
    );
}

#[tokio::test]
async fn the_page_opens_with_the_web_server_group_when_hsts_fails() {
    let run = run_of(vec![
        check("dns.caa", Status::Warn),
        check("http.hsts", Status::Fail),
    ]);

    let body = page(store_of(&[run]).await).await;

    let web = body.find(r#"data-group="web_server""#).unwrap();
    let dns = body.find(r#"data-group="dns_provider""#).unwrap();
    assert!(web < dns, "{body}");
}

// Finding 7: the switch waits for its island

fn css(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join(file);
    fs::read_to_string(path).unwrap()
}

#[tokio::test]
async fn the_switch_stays_in_the_markup_and_is_hidden_until_the_section_is_ready() {
    let squashed: String = css("grouping_switch.css")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(
        squashed.contains("#issues:not([data-ready]).grouping-switch{visibility:hidden;}"),
        "{squashed}"
    );

    let body = page(store_of(&[run_of(vec![check("http.hsts", Status::Fail)])]).await).await;
    assert!(body.contains(r#"class="grouping-switch""#), "{body}");
    assert!(!body.contains("data-ready"), "{body}");
}

// Finding 8: the protocol label

#[test]
fn the_protocol_group_is_labelled_email() {
    let run = run_of(vec![check("email.dmarc", Status::Warn)]);

    let groups = issues(
        &run,
        catalogue(),
        Links::Live,
        Grouping::Protocol,
        &Default::default(),
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].key, "email");
    assert_eq!(groups[0].label, "Email");
}
