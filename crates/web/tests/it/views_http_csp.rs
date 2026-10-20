//! v2-http-ui Phase 2 (C1, C5, C6, C7, C8): the `http.csp` tree block is read by the CSP view:
//! mode, issue count, root issues, one row per directive with one span per source.
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Node, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::pages::views::{ViewContext, render};
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn directive(name: &str, sources: &[&str], issues: &[&str]) -> Node {
    let mut fields = vec![("directive".to_string(), text(name))];
    fields.extend(sources.iter().map(|s| ("source".to_string(), text(s))));
    fields.extend(issues.iter().map(|i| ("issue".to_string(), text(i))));
    Node {
        fields,
        children: vec![],
    }
}

fn csp_block(report_only: bool, root_issues: &[&str], children: Vec<Node>) -> Block {
    let count = root_issues.len() as i64
        + children
            .iter()
            .flat_map(|c| c.fields.iter())
            .filter(|(k, _)| k == "issue")
            .count() as i64;
    let mut fields = vec![
        ("report_only".to_string(), Value::Bool(report_only)),
        ("issue_count".to_string(), Value::Int(count)),
    ];
    fields.extend(root_issues.iter().map(|i| ("issue".to_string(), text(i))));
    Block {
        id: BlockId::parse("http.csp").unwrap(),
        title: "Content-Security-Policy".into(),
        shape: Shape::Tree(Node { fields, children }),
    }
}

fn view(block: &Block, status: Status) -> Option<String> {
    let statuses = [(CheckId::parse("http.csp").unwrap(), status)];
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let feeds = vec![];
    let ctx = ViewContext {
        statuses: &statuses,
        decorate: &decorate,
        feeds: &feeds,
    };
    render(block, &ctx)
}

fn enforced() -> Block {
    csp_block(
        false,
        &["Missing base-uri directive"],
        vec![
            directive("default-src", &["'self'"], &[]),
            directive(
                "script-src",
                &["'self'", "'unsafe-inline'", "https://cdn.example.com"],
                &["script-src contains 'unsafe-inline'"],
            ),
        ],
    )
}

#[test]
fn c5_enforced_policy_shows_mode_count_root_issues_and_directive_rows() {
    let html = view(&enforced(), Status::Warn).expect("csp block is claimed");
    assert!(
        html.contains(r#"<p class="csp-view__mode">Enforced</p>"#),
        "{html}"
    );
    assert!(html.contains("2 issues"), "{html}");
    let issues = &html[html.find("csp-view__issues").expect("root issue list")..];
    let issues = &issues[..issues.find("</ul>").unwrap()];
    assert!(issues.contains("Missing base-uri directive"), "{html}");
    let d = html.find(r#"data-directive="default-src""#).unwrap();
    let s = html.find(r#"data-directive="script-src""#).unwrap();
    assert!(d < s, "{html}");
    let row = &html[s..];
    let row = &row[..row.find("</tr>").unwrap()];
    assert_eq!(
        row.matches(r#"class="csp-view__source""#).count(),
        3,
        "{row}"
    );
    assert!(
        row.contains(r#"<li data-tone="warn">script-src contains 'unsafe-inline'</li>"#),
        "{row}"
    );
}

#[test]
fn c6_report_only_policy_is_flagged_as_not_enforcing() {
    let block = csp_block(true, &[], vec![directive("default-src", &["'self'"], &[])]);
    let html = view(&block, Status::Pass).expect("csp block is claimed");
    assert!(
        html.contains(r#"<p class="csp-view__mode" data-tone="warn">Report-Only</p>"#),
        "{html}"
    );
    assert!(
        html.contains("Browsers report violations but enforce nothing."),
        "{html}"
    );
}

#[test]
fn c7_long_policy_has_one_span_per_source_inside_a_scrolling_wrapper() {
    let sources: Vec<String> = (0..120)
        .map(|i| format!("https://cdn{i}.assets.example.com"))
        .collect();
    let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
    let block = csp_block(false, &[], vec![directive("script-src", &refs, &[])]);
    let html = view(&block, Status::Pass).expect("csp block is claimed");
    assert_eq!(
        html.matches(r#"class="csp-view__source""#).count(),
        120,
        "{html}"
    );
    let scroll = html
        .find(r#"class="csp-view__scroll""#)
        .expect("scroll wrapper");
    let table = html
        .find(r#"<table class="csp-view__table""#)
        .expect("table");
    assert!(scroll < table, "{html}");

    let css =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/static/pages.css")).unwrap();
    let rule = &css[css.find(".csp-view__scroll").expect("rule in pages.css")..];
    let rule = &rule[..rule.find('}').unwrap()];
    assert!(rule.contains("overflow-x: auto"), "{rule}");
}

#[test]
fn c1_child_with_an_unknown_key_leaves_the_block_unclaimed() {
    let mut odd = directive("default-src", &["'self'"], &[]);
    odd.fields.push(("mystery".to_string(), text("x")));
    let block = csp_block(false, &[], vec![odd]);
    assert_eq!(view(&block, Status::Pass), None);
}

fn run_without_csp() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Http,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("http.hsts").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![BlockId::parse("http.hsts").unwrap()],
        }],
        blocks: vec![Block {
            id: BlockId::parse("http.hsts").unwrap(),
            title: "HSTS".into(),
            shape: Shape::Fields(vec![
                ("max_age".to_string(), Value::Int(31_536_000)),
                ("include_subdomains".to_string(), Value::Bool(true)),
                ("preload".to_string(), Value::Bool(false)),
            ]),
        }],
    }
}

#[tokio::test]
async fn c8_run_without_csp_block_shows_no_csp_view_on_the_evidence_route() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let id = match store
        .record("a.example.com", &run_without_csp(), &[], T, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    let app: Router = evidence_router(EvidenceDeps {
        history: Arc::new(store),
        v1: None,
    });
    let resp = app
        .oneshot(
            Request::get(format!("/r/{id}/http"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes);
    assert!(body.contains(r#"id="ev-http.hsts""#), "{body}");
    assert!(!body.contains(r#"id="ev-http.csp""#), "{body}");
}
