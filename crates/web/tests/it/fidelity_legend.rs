//! Result fidelity, Phase 2 (R17 label band, R18 legend): `GET /example.com` markup and CSS.
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


const SPECTRUM_CSS: &str = include_str!("../../static/spectrum.css");
const LEGEND_CSS: &str = concat!(
    include_str!("../../static/spectrum.css"),
    include_str!("../../static/pages.css")
);

/// Expected markup (R17): the label band carries `data-rows` and an inline `height: 56px` while
/// at most three label rows are needed; leaders carry inline `height: 8px|22px|36px`.
const BAND_56: &str = "height: 56px";

/// The declaration block of the first rule whose selector list contains `selector`.
fn rule<'a>(css: &'a str, selector: &str) -> &'a str {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let sel = &rest[..open];
        let close = open + rest[open..].find('}').unwrap();
        if sel.split(',').any(|s| s.trim() == selector) {
            return &rest[open + 1..close];
        }
        rest = &rest[close + 1..];
    }
    panic!("no rule for {selector}");
}

fn declared(block: &str, prop: &str) -> Option<String> {
    block.split(';').find_map(|d| {
        let (k, v) = d.split_once(':')?;
        (k.trim() == prop).then(|| v.trim().to_string())
    })
}

#[test]
fn c4_line_label_css_is_caption_mono_600_transparent() {
    let block = rule(SPECTRUM_CSS, ".spectrum__line-label");
    assert_eq!(declared(block, "font-size").as_deref(), Some("var(--font-size-caption)"), "{block}");
    assert_eq!(declared(block, "font-weight").as_deref(), Some("600"), "{block}");
    assert_eq!(declared(block, "font-family").as_deref(), Some("var(--font-mono)"), "{block}");
    let bg = declared(block, "background");
    assert!(bg.is_none() || bg.as_deref() == Some("transparent"), "{block}");
}

#[tokio::test]
async fn c4_label_band_is_56px_and_leaders_take_three_staggered_lengths() {
    let html = page(8, false).await;
    let spectrum = between(&html, "<figure id=\"spectrum\"", "</figure>");
    let band_tag = between(spectrum, "<div class=\"spectrum__label-band\"", ">");
    assert!(band_tag.contains(BAND_56), "band: {band_tag}");

    let labels = between(spectrum, "spectrum__label-band", "<svg");
    let shown = between(labels, "data-shown", "spectrum__labels\" data-run");
    let heights: Vec<&str> = shown
        .split("<span class=\"spectrum__leader\"")
        .skip(1)
        .map(|t| {
            let t = &t[..t.find('>').unwrap()];
            let p = t.find("height: ").unwrap_or_else(|| panic!("no height: {t}"));
            t[p + 8..].split(';').next().unwrap().trim_end_matches('"')
        })
        .collect();
    assert!(heights.len() >= 3, "leaders: {shown}");
    for (i, h) in heights.iter().enumerate() {
        assert_eq!(*h, ["8px", "22px", "36px"][i % 3], "leader {i}: {shown}");
    }
}

#[test]
fn c5_legend_css_is_mono_12_muted_with_line_width_marks() {
    let legend = rule(LEGEND_CSS, "#legend");
    assert_eq!(declared(legend, "font-size").as_deref(), Some("var(--font-size-label)"), "{legend}");
    assert_eq!(declared(legend, "font-family").as_deref(), Some("var(--font-mono)"), "{legend}");
    assert_eq!(declared(legend, "color").as_deref(), Some("var(--color-muted)"), "{legend}");
    for (tone, w) in [("pass", "pass"), ("warn", "warn"), ("fail", "fail")] {
        let block = rule(LEGEND_CSS, &format!(".legend__mark[data-tone=\"{tone}\"]"));
        assert_eq!(declared(block, "width"), Some(format!("var(--line-{w})")), "{tone}: {block}");
    }
    let mark = rule(LEGEND_CSS, ".legend__mark");
    assert_eq!(declared(mark, "height").as_deref(), Some("14px"), "{mark}");
}

#[tokio::test]
async fn c5_legend_of_second_entry_has_marks_fixed_since_run_1_and_note() {
    let html = page(2, false).await;
    let legend = between(&html, "id=\"legend\"", "</ul>");
    for tone in ["pass", "warn", "fail", "fix"] {
        let mark = format!("<span class=\"legend__mark\" data-tone=\"{tone}\">");
        assert!(legend.contains(&mark), "no {mark}: {legend}");
    }
    assert!(legend.contains("fixed since run 1<"), "{legend}");
    assert!(!legend.contains("muted"), "{legend}");
    let spectrum = between(&html, "<figure id=\"spectrum\"", "</figure>");
    let lines = between(spectrum, "id=\"band-dns\"", "id=\"band-tls\"")
        .matches("class=\"spectrum__line\"")
        .count()
        + 0;
    assert!(lines > 0, "no lines counted: {spectrum}");
    let note = between(&html, "<span class=\"legend__note\">", "</span>");
    assert!(
        note.ends_with(" checks · line height = weight in the grade") || note.contains("checks · line height = weight in the grade"),
        "{note}"
    );
    assert!(note.starts_with("<span class=\"legend__note\">7 checks"), "{note}");
}

#[tokio::test]
async fn c12_first_entry_legend_has_no_fixed_entry() {
    let html = page(1, false).await;
    let legend = between(&html, "id=\"legend\"", "</ul>");
    assert!(!legend.contains("fixed since"), "{legend}");
    assert!(!legend.contains("data-tone=\"fix\""), "{legend}");
    assert!(legend.contains("<span class=\"legend__mark\" data-tone=\"pass\">"), "{legend}");
}
