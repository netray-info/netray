//! Review findings on v2-inspector-shell Phase 1 (requirements 3, 4, 5, 6, 7, 26): the run's h1
//! names the target the job read, a notice keeps an h1, the example chips leave when a run is
//! shown, the tabs carry address and IPv6 targets and follow a posted run, an incomplete entry
//! gives no grade letter, and the card label is not uppercased.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use percent_encoding::percent_decode_str;
use tower::ServiceExt;

struct Stub {
    refuse: bool,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        if self.refuse {
            return Err("underscores are not allowed in hostnames".into());
        }
        Ok(Job::new(vec![]))
    }
}

fn app(refuse: bool) -> Router {
    inspect_router(InspectDeps::new(
        Arc::new(Stub { refuse }),
        InspectConfig::default(),
    ))
}

async fn get(app: &Router, uri: &str) -> String {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// (id, html) of every fragment of a posted run.
async fn fragments(app: &Router, tool: &str, body: &str) -> Vec<(String, String)> {
    let res = app
        .clone()
        .oneshot(
            Request::post(format!("/inspect/{tool}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    text.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d.trim()).ok())
        .filter_map(|j| Some((j["id"].as_str()?.to_owned(), j["html"].as_str()?.to_owned())))
        .collect()
}

fn h1_text(html: &str) -> Option<String> {
    let at = html.find("<h1")?;
    let open = html[at..].find('>')? + at + 1;
    let close = html[open..].find("</h1>")? + open;
    Some(html[open..close].to_owned())
}

/// The decoded href of the tab labelled `label` in `html`.
fn tab(html: &str, label: &str) -> String {
    html.split("<a class=\"inspector-tabs__tab\"")
        .skip(1)
        .find(|a| a.contains(&format!(">{label}</a>")))
        .and_then(|a| a.split("href=\"").nth(1))
        .and_then(|h| h.split('"').next())
        .map(|h| {
            percent_decode_str(&h.replace("&amp;", "&"))
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| panic!("no {label} tab in {html}"))
}

#[tokio::test]
async fn the_h1_names_the_target_the_job_read() {
    let app = app(false);
    let start = |frags: Vec<(String, String)>| frags[0].1.clone();

    let html = start(fragments(&app, "ip", "ip=").await);
    let caller = h1_text(&html).expect("an h1");
    assert!(
        caller.trim().parse::<IpAddr>().is_ok(),
        "the caller's own address names the run: {html}"
    );

    let html = start(fragments(&app, "http", "url=example.com").await);
    assert_eq!(h1_text(&html).as_deref(), Some("https://example.com/"));

    let html = start(fragments(&app, "tls", "h=example.com.").await);
    assert_eq!(h1_text(&html).as_deref(), Some("example.com:443"));
}

#[tokio::test]
async fn a_notice_keeps_an_h1_naming_the_inspector() {
    let app = app(true);
    let frags = fragments(&app, "tls", "h=_x.example.com").await;
    let notice = &frags[0].1;
    assert!(notice.contains("underscores"), "{notice}");
    assert_eq!(
        h1_text(notice).as_deref(),
        Some("TLS inspector"),
        "{notice}"
    );
}

#[tokio::test]
async fn example_chips_leave_when_a_run_is_shown() {
    let app = app(false);
    let page = get(&app, "/inspect/tls").await;
    let region = page
        .split("<div id=\"inspect-result\">")
        .nth(1)
        .expect("a result region");
    assert!(
        region.contains("inspector-examples__chip"),
        "the chips sit in the region a run replaces: {page}"
    );
    let frags = fragments(&app, "tls", "h=example.com").await;
    assert!(!frags[0].1.contains("inspector-examples"), "{}", frags[0].1);
}

#[tokio::test]
async fn address_targets_carry_to_the_ip_tab() {
    let app = app(false);
    let page = get(&app, "/inspect/http?url=192.0.2.24").await;
    assert_eq!(tab(&page, "IP"), "/inspect/ip?ip=192.0.2.24");
    let page = get(&app, "/inspect/tls?h=%5B2001%3Adb8%3A%3A24%5D%3A443").await;
    assert_eq!(tab(&page, "IP"), "/inspect/ip?ip=2001:db8::24");
}

#[tokio::test]
async fn the_tabs_follow_a_posted_run() {
    let app = app(false);
    let frags = fragments(&app, "tls", "h=www.example.com").await;
    let tabs = frags
        .iter()
        .find(|(id, _)| id == "inspector-tabs")
        .map(|(_, html)| html.clone())
        .expect("a fragment re-rendering the tabs for the posted target");
    assert_eq!(tab(&tabs, "DNS"), "/inspect/dns?q=www.example.com");
    assert_eq!(tab(&tabs, "EMAIL"), "/inspect/email?d=example.com");
}

#[tokio::test]
async fn tabs_never_carry_a_host_the_other_inspector_refuses() {
    let app = app(false);
    let page = get(&app, "/inspect/dns?q=_dmarc.example.com%20TXT").await;
    assert_eq!(tab(&page, "TLS"), "/inspect/tls");
    assert_eq!(tab(&page, "HTTP"), "/inspect/http");
    assert_eq!(tab(&page, "EMAIL"), "/inspect/email?d=example.com");
}

#[tokio::test]
async fn an_incomplete_entry_gives_no_grade_letter() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let run = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::Incomplete,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    };
    store
        .record("example.com", &run, &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let app = inspect_router(
        InspectDeps::new(Arc::new(Stub { refuse: false }), InspectConfig::default())
            .with_history(Some(Arc::new(store))),
    );
    let frags = fragments(&app, "tls", "h=example.com").await;
    let html = &frags[0].1;
    assert!(html.contains("inspector-backlink"), "{html}");
    assert!(!html.contains("inspector-backlink__grade"), "{html}");
}

#[tokio::test]
async fn the_clear_button_is_hidden_in_the_server_html() {
    // Without JavaScript the clear button does nothing, so it is never shown by the server; the
    // island shows it once it mounts and the input holds text (requirement 26).
    let app = app(false);
    let page = get(&app, "/inspect/tls?h=example.com").await;
    let button = page
        .split("class=\"query-input__clear\"")
        .nth(1)
        .and_then(|b| b.split('>').next())
        .expect("a clear button");
    assert!(button.contains("hidden"), "{button}");
}

#[test]
fn the_card_label_is_not_uppercased() {
    let css = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("static/query_input.css"),
    )
    .unwrap();
    let rule = css
        .split('}')
        .find(|r| {
            r.split('{')
                .next()
                .is_some_and(|s| s.trim() == ".query-input--card .query-input__label")
        })
        .expect("a card label rule");
    assert!(rule.contains("text-transform: none"), "{rule}");
}
