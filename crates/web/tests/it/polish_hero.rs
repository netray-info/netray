//! Result polish, Phase 2 (spec v2-result-polish, requirements 11 and 13; C1, C3-C6): the grade
//! descriptor does not widen the grade block, and the result header's h1 breaks after each dot.
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::run::Run;
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use std::net::IpAddr;
use tower::ServiceExt;

const T: i64 = 1_800_000_000;
const LONG: &str = "checkout.eu-central.storefront.example.com";

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

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn top_level(list: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in list.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

fn blocks_in(css: &str, selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        if top_level(&squash(&rest[..open]))
            .iter()
            .any(|s| *s == squash(selector))
        {
            out.push(squash(&rest[open + 1..open + close]));
        }
        rest = &rest[open + close + 1..];
    }
    out
}

fn css_blocks(selector: &str) -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "css") {
            continue;
        }
        out.extend(blocks_in(&fs::read_to_string(&path).unwrap(), selector));
    }
    out
}

fn fixture_run() -> Run {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acceptance/fixtures/history/new.example.com.json");
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    serde_json::from_value(v["run"].clone()).unwrap()
}

async fn page(domain: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(domain, &fixture_run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
    let resp = app
        .oneshot(
            Request::get(format!("/{domain}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn grade_descriptor_does_not_widen_the_grade_block() {
    let blocks = css_blocks(".grade__descriptor");
    assert!(
        blocks
            .iter()
            .any(|b| b.contains("width:0") && b.contains("min-width:100%")),
        "no `.grade__descriptor` block with width:0 and min-width:100%: {blocks:?}"
    );
}

#[tokio::test]
async fn long_domain_h1_breaks_after_each_dot() {
    let body = page(LONG).await;
    assert!(
        body.contains("<h1><span class=\"result-hero__label\">checkout.</span><wbr><span class=\"result-hero__label\">eu-central.</span><wbr><span class=\"result-hero__label\">storefront.</span><wbr><span class=\"result-hero__label\">example.</span><wbr><span class=\"result-hero__label\">com</span></h1>"),
        "h1 not broken at dots: {body}"
    );
    assert!(body.contains(&format!(r#"data-domain="{LONG}""#)));
}

#[tokio::test]
async fn short_domain_h1_breaks_after_the_dot() {
    let body = page("example.com").await;
    assert!(
        body.contains("<h1><span class=\"result-hero__label\">example.</span><wbr><span class=\"result-hero__label\">com</span></h1>"),
        "h1 not broken at dot: {body}"
    );
}

#[test]
fn result_header_h1_wraps_anywhere_outside_the_phone_rule() {
    let blocks = css_blocks("#result-header h1");
    assert!(
        blocks.iter().any(|b| b.contains("overflow-wrap:anywhere")),
        "no `#result-header h1` block with overflow-wrap:anywhere: {blocks:?}"
    );
}
