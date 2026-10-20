//! V2 spec v2-inspector-shell, requirement 25 (C1 to C4, server half): the help dialog, the
//! export slot and the `page` island on the result page, the evidence views, the idle page and
//! the not-found page.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use netray_web::pages::{EvidenceDeps, evidence_router};
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

/// Every catalogue check passes except the first TLS check (fail, with a finding) and the first
/// HTTP check (warn); the email section carries one block.
fn run() -> (Run, String, String) {
    let first = |p: Protocol| {
        catalogue()
            .entries()
            .iter()
            .find(|e| e.protocol == p)
            .unwrap()
            .id
            .clone()
    };
    let (tls, http) = (first(Protocol::Tls), first(Protocol::Http));
    let checks = catalogue()
        .entries()
        .iter()
        .map(|e| {
            let (status, findings) = if e.id == tls {
                (Status::Fail, vec!["certificate expired".to_string()])
            } else if e.id == http {
                (Status::Warn, vec!["no HSTS header".to_string()])
            } else {
                (Status::Pass, vec![])
            };
            RunCheck {
                id: e.id.clone(),
                status,
                findings,
                evidence: vec![],
            }
        })
        .collect();
    let sections = [Protocol::Tls, Protocol::Http, Protocol::Email]
        .into_iter()
        .map(|protocol| RunSection {
            protocol,
            outcome: SectionKind::Measured,
            reason: None,
        })
        .collect();
    let run = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections,
        checks,
        blocks: vec![Block {
            id: BlockId::parse("email.records").unwrap(),
            title: "Mail records".into(),
            shape: Shape::Text("v=spf1 -all".into()),
        }],
    };
    (run, tls.to_string(), http.to_string())
}

struct Apps {
    result: Router,
    evidence: Router,
    api: Router,
    id: String,
    tls: String,
    http: String,
    _dir: tempfile::TempDir,
}

async fn apps() -> Apps {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let (run, tls, http) = run();
    store
        .record(DOMAIN, &run, &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let id = store.latest(DOMAIN).await.unwrap().unwrap().id;
    let store = Arc::new(store);
    Apps {
        result: router(ResultState::new(store.clone(), Arc::new(StubRuns), 300)),
        evidence: evidence_router(EvidenceDeps {
            history: store.clone(),
            v1: None,
        }),
        api: netray_web::api::router(
            ApiState::new(ApiConfig::default(), Arc::new(StubRuns)).with_store(Some(store)),
        ),
        id,
        tls,
        http,
        _dir: dir,
    }
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
    )
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&#x2f;", "/")
        .replace("&amp;", "&")
}

/// The unescaped text of `<template data-export="kind">`.
fn template(html: &str, kind: &str) -> String {
    let open = format!("<template data-export=\"{kind}\">");
    let rest = html
        .split(&open)
        .nth(1)
        .unwrap_or_else(|| panic!("no {kind} template in {html}"));
    unescape(rest.split("</template>").next().unwrap())
}

fn assert_help(html: &str, title: &str) {
    assert!(
        html.contains("class=\"inspector-help__open\""),
        "help button"
    );
    // One help dialog (a stored entry also renders the share dialog).
    assert_eq!(
        html.matches(r#"id="inspector-help""#).count(),
        1,
        "one help dialog"
    );
    assert!(html.contains("id=\"inspector-help\""), "dialog id");
    assert!(
        html.contains(&format!(">{title}</h2>")),
        "h2 {title} in {html}"
    );
    assert!(html.contains("inspector-help__keys"), "key table");
}

#[tokio::test]
async fn result_page_carries_help_export_and_page_island() {
    let a = apps().await;
    let (status, html) = get(&a.result, &format!("/{DOMAIN}")).await;
    assert_eq!(status, StatusCode::OK);

    assert_help(&html, "Result page");
    let actions = html
        .split("result-hero__actions")
        .nth(1)
        .unwrap_or_else(|| panic!("no result-hero__actions in {html}"));
    let actions = actions.split("</div>").next().unwrap();
    assert!(actions.contains("inspector-help__open"), "{actions}");
    assert!(actions.contains("hidden"), "{actions}");
    assert!(html.contains("data-island=\"page\""), "page island");
    assert!(html.contains("id=\"inspect-export\""), "export slot");
    assert!(
        html.contains(&format!("data-filename=\"netray-{DOMAIN}-{}\"", a.id)),
        "filename"
    );

    let md = template(&html, "markdown");
    assert!(md.starts_with("# example.com\n\nGrade B · score "), "{md}");
    let line = md.lines().nth(2).unwrap();
    let n = line
        .strip_prefix("Grade B · score ")
        .and_then(|s| s.strip_suffix(" / 100"))
        .unwrap_or_else(|| panic!("grade line: {line}"));
    assert!(n.parse::<u32>().unwrap() <= 100, "{line}");

    let (issues, checks) = md
        .split_once("## Checks")
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .unwrap_or_else(|| panic!("no checks section: {md}"));
    assert!(issues.contains("## Open issues\n"), "{issues}");
    for (code, finding) in [(&a.tls, "certificate expired"), (&a.http, "no HSTS header")] {
        let l = issues
            .lines()
            .find(|l| l.starts_with("- ") && l.contains(code.as_str()))
            .unwrap_or_else(|| panic!("no issue line for {code}: {issues}"));
        assert!(l.ends_with(finding), "{l}");
    }
    assert!(
        checks.contains("| Code | Check | Status | Finding | Fix |"),
        "{checks}"
    );
    let rows = checks
        .lines()
        .filter(|l| l.starts_with("| "))
        .filter(|l| {
            catalogue()
                .entries()
                .iter()
                .any(|e| l.contains(&e.id.to_string()))
        })
        .count();
    assert_eq!(rows, catalogue().entries().len(), "{checks}");
}

#[tokio::test]
async fn evidence_views_carry_help_tools_and_export() {
    let a = apps().await;
    for path in [format!("/{DOMAIN}/tls"), format!("/r/{}/tls", a.id)] {
        let (status, html) = get(&a.evidence, &path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_help(&html, "Evidence view");
        let tools = html
            .split("evidence-view__tools")
            .nth(1)
            .unwrap_or_else(|| panic!("no evidence-view__tools on {path}"));
        assert!(tools.contains("inspector-help__open"), "{path}");
        assert!(tools.contains("inspect-export"), "{path}");
        assert!(html.contains("data-island=\"page\""), "{path}");
    }
}

#[tokio::test]
async fn email_view_json_export_equals_the_entry_api_body() {
    let a = apps().await;
    let (status, html) = get(&a.evidence, &format!("/{DOMAIN}/email")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(&format!(
            "data-filename=\"netray-example.com-email-{}\"",
            a.id
        )),
        "filename in {html}"
    );
    let exported: serde_json::Value = serde_json::from_str(&template(&html, "json")).unwrap();
    let (status, api) = get(&a.api, &format!("/entries/{}", a.id)).await;
    assert_eq!(status, StatusCode::OK);
    let api: serde_json::Value = serde_json::from_str(&api).unwrap();
    assert_eq!(exported, api);
}

#[tokio::test]
async fn idle_and_not_found_pages_carry_the_page_island_and_input() {
    let a = apps().await;
    let (status, idle) = get(&a.result, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(idle.contains("data-island=\"page\""), "{idle}");
    assert!(idle.contains("query-input__field"), "{idle}");

    let app = Router::new().fallback(netray_web::pages::not_found);
    let (status, nf) = get(&app, "/no/such/page").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(nf.contains("data-island=\"page\""), "{nf}");
    assert!(nf.contains("query-input__field"), "{nf}");
}

#[tokio::test]
async fn a_page_without_a_stored_entry_has_no_export_slot() {
    let a = apps().await;
    let (_, html) = get(&a.result, "/unseen.example.com").await;
    assert!(!html.contains("id=\"inspect-export\""), "{html}");
}
