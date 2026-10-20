//! V2 spec v2-http-ui, requirement 14, Phase 4 scenarios 1-3 (C4-C6): the inspector band is the
//! slim spectrum (`data-slim`, plate and line names only; no label band, axis or region labels)
//! at a 96px plate, while the result page's spectrum keeps its full furniture.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckResult, Protocol, Status};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use netray_web::tokens::CSS;
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let part = |n: usize| JobPart {
            label: format!("part{n}"),
            output: Box::pin(async move {
                Ok(vec![Block {
                    id: BlockId::parse(&format!("tls.chain{n}")).unwrap(),
                    title: "Chain".into(),
                    shape: Shape::Fields(vec![("subject".into(), Value::Text("leaf".into()))]),
                }])
            }),
        };
        let checks: Vec<CheckResult> = catalogue()
            .for_protocol(Protocol::Tls)
            .entries()
            .iter()
            .map(|s| CheckResult {
                id: s.id.clone(),
                status: Status::Pass,
                findings: Vec::new(),
                evidence: Vec::new(),
            })
            .collect();
        Ok(Job::new(vec![part(0), part(1)])
            .with_band(Protocol::Tls, Box::pin(async { Ok(checks) })))
    }
}

/// (fragment id, html) per fragment event of the run.
async fn run() -> Vec<(String, String)> {
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(
            Request::post("/inspect/tls/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("h=example.com"))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut out = Vec::new();
    for raw in text.split("\n\n") {
        let mut name = "";
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        if name != "fragment" {
            continue;
        }
        let j: serde_json::Value = serde_json::from_str(&data).unwrap();
        out.push((
            j["id"].as_str().unwrap().to_owned(),
            j["html"].as_str().unwrap().to_owned(),
        ));
    }
    out
}

/// Replace the element carrying `id="<id>"` (open tag to its matching close) with `html`.
fn replace_element(page: &mut String, id: &str, html: &str) {
    let at = page
        .find(&format!(r#"id="{id}""#))
        .unwrap_or_else(|| panic!("no element {id} in {page}"));
    let open = page[..at].rfind('<').unwrap();
    let name: String = page[open + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let (o, c) = (format!("<{name}"), format!("</{name}>"));
    let (mut depth, mut i) = (0usize, open);
    while i < page.len() {
        let rest = &page[i..];
        if rest.starts_with(&o)
            && !rest[o.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric())
        {
            depth += 1;
        } else if rest.starts_with(&c) {
            depth -= 1;
            if depth == 0 {
                page.replace_range(open..i + c.len(), html);
                return;
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    panic!("unclosed element {id}");
}

async fn applied() -> String {
    let frags = run().await;
    assert_eq!(frags[0].0, "inspect-result", "{frags:?}");
    let mut result = frags[0].1.clone();
    for (id, html) in &frags[1..] {
        // Fragments outside the result region (the tabs) are not part of this page slice.
        if !result.contains(&format!(r#"id="{id}""#)) {
            continue;
        }
        replace_element(&mut result, id, html);
    }
    result
}

use axum::Router;
use futures::future::BoxFuture;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade};
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::pages::result::{ResultState, router};

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

/// Entry `i`: five low bits of `i` pick pass or fail on five checks, so every `i` differs in
/// fingerprint. With `degraded`, email timed out and TLS is not applicable (no checks for either); the run stays complete, since the store keeps only complete runs.
fn result_run(i: u32, degraded: bool) -> Run {
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
                &result_run(i, degraded),
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

/// The last declaration of `prop` in the rule whose selector is exactly `selector`.
fn declared(selector: &str, prop: &str) -> Option<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    let mut found = None;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_none_or(|x| x != "css") {
            continue;
        }
        let css = std::fs::read_to_string(e.path()).unwrap();
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            if rest[..open].trim().rsplit('}').next().map(str::trim) == Some(selector) {
                for d in rest[open + 1..open + close].split(';') {
                    if let Some((k, v)) = d.split_once(':')
                        && k.trim() == prop
                    {
                        found = Some(v.trim().to_string());
                    }
                }
            }
            rest = &rest[open + close + 1..];
        }
    }
    found
}

fn resolve(value: &str) -> String {
    let Some(name) = value.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) else {
        return value.to_string();
    };
    let needle = format!("{name}:");
    let at = CSS
        .find(&needle)
        .unwrap_or_else(|| panic!("unknown token {name}"));
    CSS[at + needle.len()..]
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

const REMOVED: [&str; 3] = [
    "spectrum__label-band",
    "spectrum__axis",
    "spectrum__region-label",
];

#[tokio::test]
async fn c4_inspector_band_is_slim_with_plate_and_line_names_only() {
    let result = applied().await;
    let start = result
        .find(r#"id="inspect-band""#)
        .expect("inspect-band element");
    let band = &result[start..];
    let band = &band[..band.find("</figure>").expect("band figure end")];
    assert!(
        band.contains("<div class=\"spectrum\" data-slim>"),
        "{band}"
    );
    for kept in [
        "spectrum__plate",
        "inspector-band__short",
        "inspector-band__code",
    ] {
        assert!(band.contains(kept), "band lacks {kept}: {band}");
    }
    for gone in REMOVED {
        assert!(!band.contains(gone), "slim band still holds {gone}: {band}");
    }
}

#[tokio::test]
async fn c5_result_page_spectrum_keeps_label_band_and_axis_and_is_not_slim() {
    let html = page(3, false).await;
    let start = html
        .find("<figure id=\"spectrum\"")
        .expect("spectrum figure");
    let spectrum = &html[start..];
    let spectrum = &spectrum[..spectrum.find("</figure>").unwrap()];
    assert!(spectrum.contains("spectrum__label-band"), "{spectrum}");
    assert!(spectrum.contains("spectrum__axis"), "{spectrum}");
    assert!(!spectrum.contains("data-slim"), "{spectrum}");
}

#[test]
fn c6_slim_plate_is_96px_high() {
    let height = declared(".spectrum[data-slim] .spectrum__plate", "height")
        .expect("no `.spectrum[data-slim] .spectrum__plate` height rule");
    assert_eq!(resolve(&height), "96px");
}

/// The visual reading beside board G: the slim plate draws G's two guide rules.
#[tokio::test]
async fn the_slim_plate_draws_board_gs_two_guide_rules() {
    let result = applied().await;
    let band = &result[result.find(r#"id="inspect-band""#).expect("band")..];
    let band = &band[..band.find("</figure>").expect("band figure end")];
    assert_eq!(
        band.matches(r#"class="spectrum__guide""#).count(),
        2,
        "{band}"
    );
    let page = page(1, false).await;
    assert!(
        !page.contains("spectrum__guide"),
        "the result page draws no guide rules"
    );
}
