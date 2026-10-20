//! The attention board of the result page (spec v2-result-fidelity, Phase 4; R28, R34; C1, C7, C8):
//! one head row with title, meta and the visible grouping switch, and 36 px between sections.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
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

const DOMAIN: &str = "example.com";
const T: i64 = 1_800_000_000;

/// The class names the coder follows.
const HEAD_CLASS: &str = "section-head section-head--split";
const META_CLASS: &str = "attention-meta";

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

fn run() -> Run {
    let rows = [
        ("http.hsts", Status::Fail),
        ("dns.caa", Status::Warn),
        ("tls.ech_advertised", Status::Warn),
        ("http.csp", Status::Warn),
        ("email.dmarc", Status::Warn),
    ];
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::C,
        complete: true,
        sections: [
            Protocol::Dns,
            Protocol::Tls,
            Protocol::Http,
            Protocol::Email,
        ]
        .into_iter()
        .map(|protocol| RunSection {
            protocol,
            outcome: SectionKind::Measured,
            reason: None,
        })
        .collect(),
        checks: rows
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

async fn page() -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    store
        .record(DOMAIN, &run(), &[], T, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let app: Router = router(ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600));
    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The declaration blocks of every rule whose selector list contains `selector` exactly.
fn blocks(file: &str, selector: &str) -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join(file);
    let css = fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {}", path.display()));
    let mut out = Vec::new();
    let mut rest = css.as_str();
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let sel = squash(&rest[..open]);
        if sel.split(',').any(|s| s == squash(selector)) {
            out.push(squash(&rest[open + 1..open + close]));
        }
        rest = &rest[open + close + 1..];
    }
    out
}

fn has(file: &str, selector: &str, decl: &str) -> bool {
    blocks(file, selector)
        .iter()
        .any(|b| b.split(';').any(|d| d == squash(decl)))
}

#[tokio::test]
async fn c1_c8_the_attention_head_row_carries_title_meta_and_a_visible_switch() {
    let body = page().await;
    let at = body.find(r#"id="issues""#).expect("#issues missing");
    let issues = &body[at..];
    let head_at = issues
        .find(&format!(r#"<div class="{HEAD_CLASS}">"#))
        .unwrap_or_else(|| panic!("head row `{HEAD_CLASS}` missing in #issues: {issues}"));
    let head = &issues[head_at..];
    let end = head
        .find("</div>\n</div>")
        .or_else(|| head.find("</div></div>"));
    let head = &head[..end.map_or(head.len().min(1500), |e| e + 12)];

    assert!(head.contains("<h2>5 lines need attention</h2>"), "{head}");
    assert!(
        head.contains(&format!(r#"<p class="{META_CLASS}">1 fail · 4 warn</p>"#)),
        "{head}"
    );
    assert!(head.contains(r#"role="group""#), "{head}");
    assert!(head.contains(r#"aria-label="Group lines by""#), "{head}");
    assert!(
        !head.contains("hidden"),
        "switch hidden in server markup: {head}"
    );
    for label in ["Owner", "Severity", "Protocol"] {
        assert!(
            head.contains(&format!(">{label}</button>")),
            "{label}: {head}"
        );
    }
    assert_eq!(head.matches(r#"aria-pressed="true""#).count(), 1, "{head}");
    assert!(
        head.contains("marker"),
        "options must carry the marker rule class: {head}"
    );
}

#[test]
fn c1_the_switch_styles_resolve_to_the_board_values() {
    let f = "grouping_switch.css";
    let opt = ".grouping-switch__option";
    let rows: [(&str, &str, &str); 11] = [
        (
            f,
            ".grouping-switch",
            "border-bottom:var(--line-hair)solidvar(--color-line)",
        ),
        (f, ".grouping-switch", "display:flex"),
        (f, opt, "min-height:36px"),
        (f, opt, "font-family:var(--font-mono)"),
        (f, opt, "font-size:var(--font-size-label)"),
        (f, opt, "border:0"),
        (f, opt, "background:transparent"),
        (f, opt, "color:var(--color-muted)"),
        (
            f,
            r#".grouping-switch__option[aria-pressed="true"]"#,
            "color:var(--color-ink)",
        ),
        (
            f,
            r#".grouping-switch__option[aria-pressed="true"]"#,
            "font-weight:var(--font-weight-bold)",
        ),
        (f, ".grouping-switch", "gap:var(--space-1)"),
    ];
    let missing: Vec<String> = rows
        .iter()
        .filter(|(file, sel, decl)| !has(file, sel, decl))
        .map(|(_, sel, decl)| format!("{sel} {{ {decl} }}"))
        .collect();
    assert!(missing.is_empty(), "missing declarations: {missing:#?}");
}

#[test]
fn c7_sections_are_36px_apart_at_1280() {
    let tokens =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tokens.json")).unwrap();
    assert!(tokens.contains("\"9\""), "space.9 token missing");
    let gap = |sel| has("pages.css", sel, "gap:var(--space-9)");
    assert!(
        gap("#result") || gap(".result"),
        "#result needs `display:flex; flex-direction:column; gap:var(--space-9)` (36px)"
    );
    let col = |sel| {
        has("pages.css", sel, "flex-direction:column") && has("pages.css", sel, "display:flex")
    };
    assert!(
        col("#result") || col(".result"),
        "#result is not a flex column"
    );
}
