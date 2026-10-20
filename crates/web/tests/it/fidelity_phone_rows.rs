//! Result fidelity, Phase 6 (spec v2-result-fidelity, requirements 26 and 31; C7, C8): the phone
//! half of the diff rows and the check rows, in markup and in the `@media (max-width: 480px)` blocks
//! of `static/diff_block.css` and `static/check_row.css`.
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::Status::{self, Pass, Warn};
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::views::line_code;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

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

fn run(checks: &[(&str, Status)]) -> Run {
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
        checks: checks
            .iter()
            .map(|(id, status)| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status: *status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

/// `GET /example.com` over stored runs recorded just now, oldest first.
async fn page(runs: &[Run]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for (i, r) in runs.iter().enumerate() {
        store
            .record("example.com", r, &[], now - 3600 + i as i64 * 60, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    let app = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 300));
    let resp = app
        .oneshot(Request::get("/example.com").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The visible text of a chunk of HTML, whitespace collapsed.
fn text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

type Decls = HashMap<String, String>;

/// Rules of a stylesheet body as (selector, declarations), in source order; nested blocks skipped.
fn rules(css: &str) -> Vec<(String, Decls)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim().to_string();
        let close = rest[open..].find('}').expect("unclosed rule") + open;
        let decls = rest[open + 1..close]
            .split(';')
            .filter_map(|d| d.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        for s in selector.split(',') {
            out.push((s.trim().to_string(), decls_clone(&decls)));
        }
        rest = &rest[close + 1..];
    }
    out
}

fn decls_clone(d: &Decls) -> Decls {
    d.clone()
}

/// Splits a stylesheet into (top-level text, bodies of `@media (max-width: 480px)` blocks).
fn split_media(css: &str) -> (String, Vec<String>) {
    let mut top = String::new();
    let mut phone = Vec::new();
    let mut rest = css;
    while let Some(at) = rest.find("@media") {
        top.push_str(&rest[..at]);
        let open = rest[at..].find('{').expect("media without block") + at;
        let query = rest[at + 6..open]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut depth = 0;
        let mut end = open;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        if query == "(max-width: 480px)" {
            phone.push(rest[open + 1..end].to_string());
        }
        rest = &rest[end + 1..];
    }
    top.push_str(rest);
    (top, phone)
}

/// Merged declarations of every rule for exactly `selector`.
fn decl(rules: &[(String, Decls)], selector: &str) -> Decls {
    let mut merged = Decls::new();
    for (s, d) in rules {
        if s == selector {
            merged.extend(d.clone());
        }
    }
    merged
}

fn sheet(name: &str) -> (Vec<(String, Decls)>, Vec<(String, Decls)>) {
    let path = format!("{}/static/{name}", env!("CARGO_MANIFEST_DIR"));
    let css = std::fs::read_to_string(&path).unwrap();
    let (top, phone) = split_media(&css);
    (rules(&top), rules(&phone.concat()))
}

fn get<'a>(d: &'a Decls, key: &str, what: &str) -> &'a str {
    d.get(key)
        .unwrap_or_else(|| panic!("{what}: no `{key}` in {d:?}"))
}

#[tokio::test]
async fn c7_diff_entry_carries_the_phone_meta_and_the_phone_css_shows_it() {
    let body = page(&[
        run(&[("dns.dnssec", Pass), ("email.dkim", Warn)]),
        run(&[("dns.dnssec", Pass), ("email.dkim", Pass)]),
    ])
    .await;

    let entry = body
        .split("<li class=\"diff-block__entry\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no diff entry: {body}"))
        .split("</li>")
        .next()
        .unwrap();
    let code = line_code(&CheckId::parse("email.dkim").unwrap()).unwrap();
    let meta = entry
        .split("<span class=\"diff-block__meta\">")
        .nth(1)
        .unwrap_or_else(|| panic!("no diff-block__meta in {entry}"))
        .split("</span>")
        .next()
        .unwrap();
    assert_eq!(meta, format!("fixed \u{b7} {code}"), "{entry}");

    let (desktop, phone) = sheet("diff_block.css");
    assert_eq!(
        get(
            &decl(&desktop, ".diff-block__meta"),
            "display",
            "desktop meta"
        ),
        "none"
    );
    assert_ne!(
        get(&decl(&phone, ".diff-block__meta"), "display", "phone meta"),
        "none"
    );
    for hidden in [".diff-block__kind", ".diff-block__code"] {
        assert_eq!(get(&decl(&phone, hidden), "display", hidden), "none");
    }
    assert_eq!(
        get(
            &decl(&phone, ".diff-block__entry"),
            "padding",
            "phone entry"
        ),
        "11px 14px"
    );
}

#[tokio::test]
async fn c8_check_row_carries_the_phone_meta_and_the_phone_css_stacks_it() {
    let body = page(&[
        run(&[("dns.caa", Pass), ("dns.dnssec", Pass)]),
        run(&[("dns.caa", Warn), ("dns.dnssec", Pass)]),
    ])
    .await;

    let row = body
        .split(r#"<article class="check-row""#)
        .skip(1)
        .find(|r| r.contains("dns.caa"))
        .unwrap_or_else(|| panic!("no dns.caa row in {body}"));
    let at = row
        .find("<p class=\"check-row__phone-meta\">")
        .unwrap_or_else(|| panic!("no check-row__phone-meta in {row}"));
    assert!(at < row.find("check-row__title").unwrap(), "{row}");
    let meta = row[at..].split("</p>").next().unwrap();
    let code = line_code(&CheckId::parse("dns.caa").unwrap()).unwrap();
    assert_eq!(
        text(meta),
        format!("warn \u{b7} {code} \u{b7} lit up in run 2"),
        "{meta}"
    );
    assert!(
        meta.contains("<span class=\"check-row__sev\">warn</span>"),
        "{meta}"
    );

    let (desktop, phone) = sheet("check_row.css");
    assert_eq!(
        get(
            &decl(&desktop, ".check-row__phone-meta"),
            "display",
            "desktop"
        ),
        "none"
    );
    let shown = decl(&phone, ".check-row__phone-meta");
    assert_ne!(get(&shown, "display", "phone meta"), "none");
    assert_eq!(
        get(&shown, "font-size", "phone meta"),
        "var(--font-size-caption)"
    );
    assert_eq!(get(&shown, "font-family", "phone meta"), "var(--font-mono)");
    assert_eq!(
        get(&shown, "font-weight", "phone meta"),
        "var(--font-weight-bold)"
    );
    let row_css = decl(&phone, ".check-row");
    let one_column = row_css.get("display").is_some_and(|v| v == "flex")
        || row_css
            .get("grid-template-columns")
            .is_some_and(|v| v.split_whitespace().count() == 1);
    assert!(
        one_column,
        "phone .check-row is not one column: {row_css:?}"
    );
    assert_eq!(
        get(&decl(&phone, ".check-row__title"), "font-size", "title"),
        "var(--font-size-body)"
    );
    assert_eq!(
        get(&decl(&phone, ".check-row__meta"), "font-size", "meta"),
        "var(--font-size-label)"
    );
}
