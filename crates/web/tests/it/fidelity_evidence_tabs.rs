//! Result fidelity, Phase 5 (spec v2-result-fidelity, requirement 35; C1-C4): the stored result
//! page's Evidence section is a tabbed card, one tab and one panel per protocol.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::run::Run;
use netray_model::{Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::explore::view_link;
use netray_web::pages::result::views::line_code;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";
const T: i64 = 1_800_000_000;
const PROTOCOLS: [&str; 5] = ["dns", "tls", "http", "email", "ip"];

const HEAD: &str = r#"<div class="section-head"><h2>Evidence</h2><p class="evidence-meta">recorded with this run</p></div>"#;
const TABS: &str = r#"<div class="evidence-tabs" data-island="tabs">"#;
const NAV: &str = r#"<nav class="evidence-tabs__nav" aria-label="Evidence views">"#;

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

/// The design fixture's run: every block shape, every protocol measured.
fn fixture_run() -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/example.com.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(v["run"].clone()).unwrap()
}

/// The fixture with every check passing except those in `open`.
fn run_with_open(open: &[Protocol]) -> Run {
    let mut run = fixture_run();
    for check in &mut run.checks {
        let protocol = check.id.protocol().as_str().to_string();
        let is_open = open.iter().any(|p| p.as_str() == protocol);
        check.status = match (check.status, is_open) {
            (Status::NotApplicable, _) => Status::NotApplicable,
            (_, true) => Status::Warn,
            _ => Status::Pass,
        };
    }
    run
}

/// Returns the page body and the stored entry id.
async fn page(run: &Run) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, run, &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let id = store.latest(DOMAIN).await.unwrap().unwrap().id;
    let app = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
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
    (String::from_utf8_lossy(&bytes).into_owned(), id)
}

fn evidence(body: &str) -> &str {
    let at = body.find(r#"id="evidence""#).expect("#evidence missing");
    &body[at..]
}

/// The panel of `protocol`, from its opening tag to the next panel or the end.
fn panel<'a>(body: &'a str, protocol: &str) -> &'a str {
    let ev = evidence(body);
    let open = format!(
        r#"<div id="evidence-{protocol}" class="evidence-tabs__panel" data-panel="{protocol}""#
    );
    let at = ev
        .find(&open)
        .unwrap_or_else(|| panic!("panel `{open}` missing: {ev}"));
    let rest = &ev[at + open.len()..];
    let end = rest
        .find(r#"class="evidence-tabs__panel""#)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The tab anchor of `protocol`: its opening tag.
fn tab<'a>(body: &'a str, protocol: &str) -> &'a str {
    let ev = evidence(body);
    let open = format!(
        r##"<a class="evidence-tabs__tab marker" href="#evidence-{protocol}" data-tab="{protocol}""##
    );
    let at = ev
        .find(&open)
        .unwrap_or_else(|| panic!("tab `{open}` missing: {ev}"));
    let end = ev[at..].find('>').unwrap();
    &ev[at..at + end]
}

#[tokio::test]
async fn c1_c2_a_stored_entry_renders_the_evidence_card() {
    let (body, id) = page(&fixture_run()).await;
    let ev = evidence(&body);
    for needle in [HEAD, TABS, NAV] {
        assert!(ev.contains(needle), "lacks {needle}: {ev}");
    }
    let mut at = 0;
    for p in PROTOCOLS {
        let needle = format!(r##"href="#evidence-{p}" data-tab="{p}""##);
        at += ev[at..]
            .find(&needle)
            .unwrap_or_else(|| panic!("tab {p} missing or out of order"))
            + needle.len();
    }
    let mut at = 0;
    for p in PROTOCOLS {
        let needle = format!(r#"<div id="evidence-{p}" class="evidence-tabs__panel""#);
        at += ev[at..]
            .find(&needle)
            .unwrap_or_else(|| panic!("panel {p} missing or out of order"))
            + needle.len();
    }
    let protocols = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ];
    for (name, protocol) in PROTOCOLS.iter().zip(protocols) {
        let panel = panel(&body, name);
        assert!(
            panel.contains(r#"class="evidence-block""#),
            "{name}: no blocks"
        );
        assert!(
            panel.contains(r#"<p class="evidence-tabs__feeds">Feeds lines: <code>"#),
            "{name}: no feeds line"
        );
        assert!(
            panel.contains(&format!(
                r#"<a class="evidence-tabs__all" href="/r/{id}/{name}">All evidence →</a>"#
            )),
            "{name}: no all-evidence link"
        );
        let link = view_link(protocol, DOMAIN);
        assert!(
            panel.contains(&format!(
                r#"<a class="evidence-tabs__explore" href="{}""#,
                link.href.replace('&', "&amp;")
            )),
            "{name}: no explore link"
        );
        assert!(panel.contains(&link.label), "{name}: explore label");
    }
    let tls = panel(&body, "tls");
    let code =
        line_code(&netray_model::CheckId::parse("tls.chain_trusted").unwrap()).expect("code");
    assert!(
        tls.contains(&format!("<code>{code}</code> Trust")),
        "tls feeds lacks the chain-trusted line: {tls}"
    );
}

#[tokio::test]
async fn c3_the_first_non_pass_protocol_is_the_current_tab() {
    let cases: [(&[Protocol], &str); 3] = [
        (&[Protocol::Tls], "tls"),
        (&[], "dns"),
        (&[Protocol::Email, Protocol::Http], "http"),
    ];
    for (open, current) in cases {
        let (body, _) = page(&run_with_open(open)).await;
        for p in PROTOCOLS {
            let t = tab(&body, p);
            let shown = panel(&body, p).contains("data-shown") && {
                let ev = evidence(&body);
                let o = format!(
                    r#"<div id="evidence-{p}" class="evidence-tabs__panel" data-panel="{p}""#
                );
                let at = ev.find(&o).unwrap() + o.len();
                ev[at..].starts_with(" data-shown")
            };
            assert_eq!(
                t.contains(r#"aria-current="true""#),
                p == current,
                "{open:?} tab {p}"
            );
            assert_eq!(shown, p == current, "{open:?} panel {p}");
        }
    }
}

#[tokio::test]
async fn c4_a_not_applicable_section_shows_its_reason_and_no_blocks() {
    let mut run = fixture_run();
    let ip = run
        .sections
        .iter_mut()
        .find(|s| s.protocol == Protocol::Ip)
        .unwrap();
    ip.outcome = netray_model::run::SectionKind::NotApplicable;
    ip.reason = Some("no public address to look up".into());
    run.blocks.retain(|b| b.id.module() != "ip");
    let (body, _) = page(&run).await;
    let ip = panel(&body, "ip");
    assert!(
        ip.contains(r#"<p class="evidence-tabs__reason">no public address to look up</p>"#),
        "{ip}"
    );
    assert!(!ip.contains(r#"class="evidence-block""#), "{ip}");
}

fn css_blocks(selector: &str) -> Vec<String> {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let mut out = Vec::new();
    for entry in fs::read_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "css") {
            continue;
        }
        let css = fs::read_to_string(&path).unwrap();
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            if squash(&rest[..open])
                .split(',')
                .any(|s| s == squash(selector))
            {
                out.push(squash(&rest[open + 1..open + close]));
            }
            rest = &rest[open + close + 1..];
        }
    }
    out
}

#[test]
fn the_evidence_tabs_styles_resolve_to_the_board_values() {
    let rows: [(&str, &str); 10] = [
        (
            ".evidence-tabs",
            "border:var(--line-hair)solidvar(--color-line)",
        ),
        (".evidence-tabs", "border-radius:2px"),
        (".evidence-tabs", "background:var(--color-surface)"),
        (".evidence-tabs__tab", "font-family:var(--font-mono)"),
        (".evidence-tabs__tab", "font-size:var(--font-size-label)"),
        (".evidence-tabs__tab", "text-transform:uppercase"),
        (
            r#".evidence-tabs__tab[aria-current="true"]"#,
            "font-weight:var(--font-weight-bold)",
        ),
        (
            ".evidence-tabs[data-ready] .evidence-tabs__panel:not([data-shown])",
            "display:none",
        ),
        (".evidence-tabs__panel table", "overflow-x:auto"),
        (".evidence-tabs__swipe", "display:none"),
    ];
    let missing: Vec<String> = rows
        .iter()
        .filter(|(sel, decl)| {
            !css_blocks(sel)
                .iter()
                .any(|b| b.split(';').any(|d| d == *decl))
        })
        .map(|(sel, decl)| format!("{sel} {{ {decl} }}"))
        .collect();
    assert!(missing.is_empty(), "missing declarations: {missing:#?}");
}
