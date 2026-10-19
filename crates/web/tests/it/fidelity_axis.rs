//! (copied harness) The result page's spectrum (spec v2-result-page, Phase 2; requirement 15, C7, C15):
//! `GET /example.com` holds the spectrum over the latest and at most 23 older entries.
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

const T: i64 = 1_800_000_000;
const PROTOCOLS: [&str; 5] = ["dns", "tls", "http", "email", "ip"];

struct StubRuns;

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
        Box::pin(async { RunStart::NotStarted })
    }
}

fn check(id: &str, status: Status) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    }
}

fn section(protocol: Protocol, outcome: SectionKind) -> RunSection {
    RunSection {
        protocol,
        outcome,
        reason: None,
    }
}

/// Entry `i`: five low bits of `i` pick pass or fail on five checks, so every `i` differs in
/// fingerprint. With `degraded`, email timed out and TLS is not applicable (no checks for either); the run stays complete, since the store keeps only complete runs.
fn run(i: u32, degraded: bool) -> Run {
    let s = |bit: u32| {
        if i >> bit & 1 == 1 {
            Status::Fail
        } else {
            Status::Pass
        }
    };
    let mut checks = vec![
        check("dns.caa", s(0)),
        check("dns.ns", s(1)),
        check("http.hsts", s(2)),
        check("ip.reputation", Status::Pass),
    ];
    let (tls, email) = if degraded {
        (SectionKind::NotApplicable, SectionKind::TimedOut)
    } else {
        checks.push(check("tls.chain_trusted", s(3)));
        checks.push(check("tls.not_expired", s(4)));
        checks.push(check("email.spf", Status::Pass));
        (SectionKind::Measured, SectionKind::Measured)
    };
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![
            section(Protocol::Dns, SectionKind::Measured),
            section(Protocol::Tls, tls),
            section(Protocol::Http, SectionKind::Measured),
            section(Protocol::Email, email),
            section(Protocol::Ip, SectionKind::Measured),
        ],
        checks,
        blocks: vec![],
    }
}

/// Records `n` entries at increasing times; the entry one older than the newest is degraded when
/// `degrade_age_one`.
async fn page(n: u32, degrade_age_one: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for i in 0..n {
        let degraded = degrade_age_one && i + 2 == n;
        store
            .record(
                "example.com",
                &run(i, degraded),
                &[],
                T + i64::from(i) * 100,
                |p, c| diff(p, c, &[], None),
            )
            .await
            .unwrap();
    }
    let state = ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600);
    let app: Router = router(state);
    let resp = app
        .oneshot(Request::get("/example.com").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn between<'a>(html: &'a str, from: &str, to: &str) -> &'a str {
    let start = html
        .find(from)
        .unwrap_or_else(|| panic!("no {from} in {html}"));
    let rest = &html[start..];
    &rest[..rest.find(to).unwrap_or(rest.len())]
}


const CSS_SRC: &str = include_str!("../../static/spectrum.css");
const TEMPLATE: &str = include_str!("../../templates/components/spectrum.html");

/// Body of the first rule whose selector list is exactly `selector`.
fn rule<'a>(css: &'a str, selector: &str) -> &'a str {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let sel = rest[..open].rsplit('}').next().unwrap().trim();
        let close = open + rest[open..].find('}').unwrap();
        if sel == selector {
            return &rest[open + 1..close];
        }
        rest = &rest[close + 1..];
    }
    panic!("no rule {selector}");
}

fn declaration(body: &str, prop: &str) -> Option<String> {
    body.split(';').find_map(|d| {
        let (k, v) = d.split_once(':')?;
        (k.trim() == prop).then(|| v.trim().to_string())
    })
}

/// Resolves a single `var(--x)` through the generated tokens; other values pass through.
fn resolve(value: &str) -> String {
    match value.strip_prefix("var(--").and_then(|v| v.strip_suffix(')')) {
        Some(name) => {
            let body = netray_web::tokens::CSS;
            declaration(
                &body[body.find(&format!("--{name}:")).unwrap_or_else(|| panic!("no token {name}"))..],
                &format!("--{name}"),
            )
            .unwrap()
        }
        None => value.to_string(),
    }
}

fn prop(selector: &str, name: &str) -> String {
    let v = declaration(rule(CSS_SRC, selector), name)
        .unwrap_or_else(|| panic!("{selector} has no {name}"));
    resolve(&v)
}

#[test]
fn c2_plate_radius_and_axis_height_come_from_tokens() {
    assert_eq!(prop(".spectrum__plate", "border-radius"), "2px");
    assert_eq!(prop(".spectrum__axis", "height"), "32px");
    assert_eq!(prop(".spectrum__tick", "height"), "4px");
    assert_eq!(prop(".spectrum__tick[data-major]", "height"), "10px");
    assert!(TEMPLATE.contains("class=\"spectrum__tick\""), "template has no tick span");
}

#[tokio::test]
async fn c9_axis_has_one_tick_per_line_boundary_and_major_ticks_at_ends_and_regions() {
    let html = page(3, false).await;
    let spectrum = between(&html, "<figure id=\"spectrum\"", "</figure>");
    let lines = spectrum.matches("class=\"spectrum__target\"").count();
    assert!(lines > 0, "no lines: {spectrum}");

    let axis = between(spectrum, "class=\"spectrum__axis\"", "</figure>");
    let ticks: Vec<&str> = axis
        .split("<span class=\"spectrum__tick\"")
        .skip(1)
        .map(|t| &t[..t.find('>').unwrap()])
        .collect();
    assert_eq!(ticks.len(), lines + 1, "ticks vs lines: {axis}");
    for t in &ticks {
        assert!(t.contains("style=\"left: ") && t.contains("%"), "tick without left pct: {t}");
    }
    let regions = axis.matches("class=\"spectrum__region-label\"").count();
    assert_eq!(regions, 5, "regions: {axis}");
    let major = ticks.iter().filter(|t| t.contains("data-major")).count();
    assert_eq!(major, 2 + (regions - 1), "major ticks: {axis}");
}
