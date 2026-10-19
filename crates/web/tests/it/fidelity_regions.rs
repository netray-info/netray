//! Spectrum region labels (spec v2-result-fidelity, Phase 2; requirements 16 and 19, C3, C10, C11,
//! C6/C13): mono caption caps centred under each region, the section's grade after the name, and
//! the catalogue's short for a line label.
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
use netray_engine::history::score_run;
use netray_engine::profile::Profiles;
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

const T: i64 = 1_800_000_000;

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

fn between<'a>(html: &'a str, from: &str, to: &str) -> &'a str {
    let start = html
        .find(from)
        .unwrap_or_else(|| panic!("no {from} in {html}"));
    let rest = &html[start..];
    &rest[..rest.find(to).unwrap_or(rest.len())]
}

fn run_with(checks: Vec<RunCheck>, email: SectionKind) -> Run {
    let mut sections = vec![];
    for p in [Protocol::Dns, Protocol::Tls, Protocol::Http, Protocol::Email, Protocol::Ip] {
        let outcome = if p == Protocol::Email {
            email.clone()
        } else {
            SectionKind::Measured
        };
        sections.push(section(p, outcome));
    }
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections,
        checks,
        blocks: vec![],
    }
}

async fn page_of(run: &Run) -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record("example.com", run, &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
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

/// The region labels as (opening tag, inner html), in document order.
fn region_labels(html: &str) -> Vec<(String, String)> {
    const OPEN: &str = "<span class=\"spectrum__region-label\"";
    let axis = between(html, "class=\"spectrum__axis\"", "</div>");
    axis.split(OPEN)
        .skip(1)
        .map(|chunk| {
            let gt = chunk.find('>').unwrap();
            let tag = format!("{OPEN}{}>", &chunk[..gt]);
            let rest = &chunk[gt + 1..];
            let inner = if let Some(g) = rest.find("</span></span>") {
                rest[..g + 7].to_string()
            } else {
                rest[..rest.find("</span>").unwrap_or(rest.len())].to_string()
            };
            (tag, inner)
        })
        .collect()
}

fn letter(g: Grade) -> &'static str {
    match g {
        Grade::APlus => "A+",
        Grade::A => "A",
        Grade::B => "B",
        Grade::C => "C",
        Grade::D => "D",
        Grade::F => "F",
        Grade::Incomplete => "incomplete",
    }
}

fn mixed_run() -> Run {
    run_with(
        vec![
            check("dns.caa", Status::Pass),
            check("dns.ns", Status::Fail),
            check("tls.chain_trusted", Status::Pass),
            check("http.hsts", Status::Pass),
            check("ip.reputation", Status::Pass),
        ],
        SectionKind::NotApplicable,
    )
}

fn rule<'a>(css: &'a str, selector: &str) -> &'a str {
    let start = css
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("no rule for {selector}"));
    let rest = &css[start..];
    &rest[..rest.find('}').unwrap()]
}

#[test]
fn c3_region_label_css() {
    let css = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/static/spectrum.css"
    ))
    .unwrap();
    let r = rule(&css, ".spectrum__region-label");
    for want in [
        "font-size: var(--font-size-caption)",
        "font-family: var(--font-mono)",
        "text-transform: uppercase",
        "letter-spacing: .14em",
        "text-align: center",
    ] {
        assert!(r.contains(want), "region label rule lacks {want}: {r}");
    }
}

#[tokio::test]
async fn c3_region_labels_centred_with_left_and_width() {
    let html = page_of(&mixed_run()).await;
    let labels = region_labels(&html);
    let names: Vec<String> = labels
        .iter()
        .map(|(_, inner)| inner.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_uppercase())
        .collect();
    assert_eq!(names, ["DNS", "TLS", "HTTP", "EMAIL", "IP"], "{html}");
    for (tag, _) in &labels {
        assert!(tag.contains("left: "), "{tag}");
        assert!(tag.contains("width: "), "{tag}");
        assert!(tag.contains("%"), "{tag}");
    }
}

#[tokio::test]
async fn c10_dns_label_carries_the_section_grade() {
    let run = mixed_run();
    let profile = Profiles::get(3).unwrap();
    let pct = score_run(&run, profile).sections[&Protocol::Dns].percentage;
    let expected = profile
        .thresholds
        .iter()
        .find(|(_, t)| pct >= f64::from(*t))
        .map_or(Grade::F, |(g, _)| *g);
    let html = page_of(&run).await;
    let labels = region_labels(&html);
    let (_, inner) = labels
        .iter()
        .find(|(_, inner)| inner.to_uppercase().starts_with("DNS"))
        .unwrap_or_else(|| panic!("no DNS label in {html}"));
    let want = format!(
        "<span class=\"spectrum__region-grade\">{}</span>",
        letter(expected)
    );
    assert!(inner.to_uppercase().starts_with("DNS"), "{inner}");
    assert!(inner.contains(&want), "want {want} in {inner}");
}

#[tokio::test]
async fn c11_unscored_section_shows_name_only() {
    let html = page_of(&mixed_run()).await;
    let labels = region_labels(&html);
    let (_, inner) = labels
        .iter()
        .find(|(_, inner)| inner.to_uppercase().starts_with("EMAIL"))
        .unwrap_or_else(|| panic!("no EMAIL label in {html}"));
    assert!(!inner.contains("spectrum__region-grade"), "{inner}");
    // the scored sections do carry one, so the absence above is the section's, not the page's
    assert!(html.contains("spectrum__region-grade"), "{html}");
}

#[tokio::test]
async fn c6_expiry_window_short_is_expiry() {
    let id = CheckId::parse("tls.expiry_window").unwrap();
    let spec = netray_engine::catalogue::catalogue().get(&id).unwrap();
    assert_eq!(spec.short, "Expiry");

    let run = run_with(
        vec![
            check("dns.caa", Status::Pass),
            check("tls.expiry_window", Status::Warn),
            check("http.hsts", Status::Pass),
            check("ip.reputation", Status::Pass),
        ],
        SectionKind::NotApplicable,
    );
    let html = page_of(&run).await;
    assert!(html.contains("data-code=\"Expiry\""), "{html}");
    assert!(!html.contains("data-code=\"Renewal\""), "{html}");
}
