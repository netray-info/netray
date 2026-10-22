//! Result fidelity, Phase 4 (v2-result-fidelity R29, R30, R31): issue items as the board draws
//! them (check_row side column, meta row, line code) and issue groups as cards.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_engine::profile::Profiles;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::views::line_code;
use netray_web::pages::result::{ResultState, router};
use netray_web::spectrum::{RunInput, layout};
use tower::ServiceExt;

const DOMAIN: &str = "example.com";

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

fn run(failing: &[&str]) -> Run {
    Run {
        profile_version: Profiles::current().version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::C,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Http,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: failing
            .iter()
            .map(|id| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status: Status::Fail,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

async fn page(failing: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        - 60;
    match store
        .record(DOMAIN, &run(failing), &[], at, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { .. } => {}
        other => panic!("expected First, got {other:?}"),
    }
    let app: Router = router(ResultState::new(store, Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn issues(body: &str) -> &str {
    let from = body.find(r#"id="issues""#).expect("issues section");
    let rest = &body[from..];
    &rest[..rest.find(r#"<details id="passed""#).unwrap_or(rest.len())]
}

/// The CSS rule bodies whose selector list mentions `selector`, from every stylesheet.
fn css_rules(selector: &str) -> String {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    let mut out = String::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "css") {
            continue;
        }
        let css = std::fs::read_to_string(&path).unwrap();
        for rule in css.split('}') {
            if let Some((sel, body)) = rule.split_once('{')
                && sel.trim().split(',').any(|s| s.trim() == selector)
            {
                out.push_str(body);
                out.push('\n');
            }
        }
    }
    out
}

#[test]
fn r29_line_code_is_the_tag_and_the_spectrum_position() {
    let profile = Profiles::current();
    let current = RunInput {
        statuses: Default::default(),
        sections: Default::default(),
        profile,
    };
    let lay = layout(&current, &[], catalogue(), 1200).unwrap();
    let position = |id: &CheckId| lay.lines.iter().position(|l| &l.id == id).unwrap() + 1;
    let hsts = CheckId::parse("http.hsts").unwrap();
    assert_eq!(
        line_code(&CheckId::parse("http.hsts").unwrap()),
        Some(format!("HTTP\u{b7}{:02}", position(&hsts)))
    );
    let mail = lay
        .lines
        .iter()
        .find(|l| l.protocol == Protocol::Email)
        .unwrap();
    let code = line_code(&mail.id).unwrap();
    assert!(code.starts_with("MAIL\u{b7}"), "{code}");
    assert_eq!(code, format!("MAIL\u{b7}{:02}", position(&mail.id)));
}

#[tokio::test]
async fn r31_issue_item_is_a_side_column_and_a_content_column() {
    let body = page(&["http.hsts"]).await;
    let part = issues(&body);
    let code = line_code(&CheckId::parse("http.hsts").unwrap()).unwrap();
    for needle in [
        r#"<div class="check-row__side">"#.to_string(),
        r#"<span class="check-row__bar" data-tone="fail">"#.to_string(),
        r#"<span class="check-row__sev">fail</span>"#.to_string(),
        format!(r#"<span class="check-row__code">{code}</span>"#),
        r#"class="check-row__title""#.to_string(),
        r#"class="check-row__since""#.to_string(),
        r#"class="check-row__body""#.to_string(),
        r#"class="check-row__fix""#.to_string(),
        "Fix:".to_string(),
        r#"<p class="check-row__meta"><span>HTTP · RFC 6797</span>"#.to_string(),
        "evidence →</a>".to_string(),
        "guide →</a>".to_string(),
    ] {
        assert!(part.contains(&needle), "{needle} missing in {part}");
    }
}

#[test]
fn r31_check_row_css_follows_the_board() {
    let row = css_rules(".check-row");
    assert!(row.contains("grid-template-columns: 84px"), "{row}");
    assert!(row.contains("gap: var(--space-4)"), "{row}");
    assert!(css_rules(".check-row__bar").contains("height: 34px"));
    let sev = css_rules(".check-row__sev");
    assert!(sev.contains("text-transform: uppercase"), "{sev}");
    assert!(
        sev.contains("font-weight: var(--font-weight-bold)"),
        "{sev}"
    );
    assert!(css_rules(".check-row__code").contains("var(--color-muted)"));
    let body = css_rules(".check-row__body");
    assert!(body.contains("font-size: var(--font-size-text)"), "{body}");
    assert!(css_rules(".check-row__meta").contains("font-size: var(--font-size-label)"));
    assert!(css_rules(".check-row__link").contains("text-decoration: underline"));
}

#[tokio::test]
async fn r30_issue_groups_are_cards_with_label_and_line_count() {
    let body = page(&["http.hsts", "http.csp"]).await;
    let part = issues(&body);
    assert!(part.contains(r#"<section class="issue-group""#), "{part}");
    let head = part
        .find(r#"<header class="issue-group__head"><h3 class="issue-group__label">"#)
        .unwrap_or_else(|| panic!("card header missing in {part}"));
    let head = &part[head..];
    let head = &head[..head.find("</header>").unwrap()];
    assert!(head.contains("web server"), "{head}");
    assert!(
        head.ends_with(r#"<span class="issue-group__count">2 lines</span>"#),
        "{head}"
    );

    let card = css_rules(".issue-group");
    assert!(
        card.contains("border: var(--line-hair) solid var(--color-line)"),
        "{card}"
    );
    assert!(card.contains("border-radius: var(--radius-max)"), "{card}");
    assert!(card.contains("background: var(--color-surface)"), "{card}");
    let label = css_rules(".issue-group__label");
    assert!(
        label.contains("font-size: var(--font-size-text)"),
        "{label}"
    );
    assert!(css_rules(".issue-group__count").contains("font-size: var(--font-size-label)"));
}

#[tokio::test]
async fn r30_one_line_group_reads_one_line() {
    let body = page(&["http.hsts"]).await;
    assert!(
        issues(&body).contains(r#"<span class="issue-group__count">1 line</span>"#),
        "{}",
        issues(&body)
    );
}
