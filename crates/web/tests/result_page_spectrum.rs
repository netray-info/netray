//! The result page's spectrum (spec v2-result-page, Phase 2; requirement 15, C7, C15):
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

/// The opening tags carrying `data-run="<k>"` inside `scope`, in document order.
fn run_tags(scope: &str) -> Vec<(u32, String)> {
    let mut out = vec![];
    let mut rest = scope;
    while let Some(p) = rest.find("data-run=\"") {
        let tag_start = rest[..p].rfind('<').unwrap();
        let tag_end = p + rest[p..].find('>').unwrap();
        let k: u32 = rest[p + 10..].split('"').next().unwrap().parse().unwrap();
        out.push((k, rest[tag_start..=tag_end].to_string()));
        rest = &rest[tag_end..];
    }
    out
}

fn band<'a>(spectrum: &'a str, protocol: &str) -> &'a str {
    let from = format!("id=\"band-{protocol}\"");
    let start = spectrum
        .find(&from)
        .unwrap_or_else(|| panic!("no {from} in {spectrum}"));
    let rest = &spectrum[start + from.len()..];
    let end = rest.find("id=\"band-").or_else(|| rest.find("</svg>"));
    &rest[..end.unwrap_or(rest.len())]
}

#[tokio::test]
async fn c15_spectrum_holds_five_bands_with_one_run_group_per_entry() {
    let html = page(3, false).await;
    let spectrum = between(&html, "<figure id=\"spectrum\"", "</figure>");

    for p in PROTOCOLS {
        let tags = run_tags(band(spectrum, p));
        let ks: Vec<u32> = tags.iter().map(|(k, _)| *k).collect();
        assert_eq!(ks, vec![0, 1, 2], "band-{p}: {spectrum}");
        for (k, tag) in &tags {
            assert_eq!(
                tag.contains("data-shown"),
                *k == 0,
                "band-{p} run {k}: {tag}"
            );
        }
    }
    let labels = between(spectrum, "spectrum__label-band", "<svg");
    for (k, tag) in run_tags(labels) {
        assert_eq!(tag.contains("data-shown"), k == 0, "label set {k}: {tag}");
    }
    for word in ["pass", "warn", "fail", "fixed", "muted"] {
        let legend = between(spectrum, "id=\"legend\"", "</ul>");
        assert!(legend.contains(word), "legend lacks {word}: {legend}");
    }
}

#[tokio::test]
async fn c15_unmeasured_marks_only_the_timed_out_band_of_that_run() {
    let html = page(3, true).await;
    let spectrum = between(&html, "<figure id=\"spectrum\"", "</figure>");

    let email = run_tags(band(spectrum, "email"));
    for (k, tag) in &email {
        assert_eq!(
            tag.contains("data-unmeasured"),
            *k == 1,
            "email run {k}: {tag}"
        );
    }
    for (k, tag) in run_tags(band(spectrum, "tls")) {
        assert!(!tag.contains("data-unmeasured"), "tls run {k}: {tag}");
    }
}

#[tokio::test]
async fn c7_trace_shows_24_rows_of_26_entries_newest_first() {
    let html = page(26, false).await;
    let spectrum = between(&html, "<figure id=\"spectrum\"", "</figure>");

    let trace = between(spectrum, "id=\"trace\"", "id=\"legend\"");
    let ks: Vec<u32> = run_tags(trace).into_iter().map(|(k, _)| k).collect();
    assert_eq!(ks, (0..24).collect::<Vec<u32>>(), "{trace}");

    let ks: Vec<u32> = run_tags(band(spectrum, "dns"))
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(ks, (0..24).collect::<Vec<u32>>());
}
