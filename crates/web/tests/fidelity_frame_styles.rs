//! Result fidelity, Phase 1 (spec v2-result-fidelity; C1 container, C4 grade, C7 h1, C8 buttons):
//! the stylesheet rules resolve, through the tokens, to the board's values.
use netray_web::tokens::{CSS, values};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

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
use std::net::IpAddr;
use tower::ServiceExt;

/// The class names the coder follows: a shared base and two modifiers, all in `static/button.css`.
const BUTTON: &str = ".button";
const PRIMARY: &str = ".button--primary";
const SECONDARY: &str = ".button--secondary";

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn rules(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push((
            squash(&rest[..open]),
            rest[open + 1..open + close].to_string(),
        ));
        rest = &rest[open + close + 1..];
    }
    out
}

/// The last raw value `prop` takes over all rules whose selector list holds `selector`.
fn raw(css: &str, selector: &str, prop: &str) -> Option<String> {
    let wanted = squash(selector);
    let mut found = None;
    for (prelude, body) in rules(css) {
        if prelude.split(',').any(|s| s == wanted) {
            for decl in body.split(';') {
                if let Some((k, v)) = decl.split_once(':')
                    && k.trim() == prop
                {
                    found = Some(v.trim().to_string());
                }
            }
        }
    }
    found
}

/// The light `:root` declarations of the generated token CSS.
fn root() -> Vec<(String, String)> {
    rules(CSS)
        .into_iter()
        .filter(|(p, _)| p == ":root")
        .flat_map(|(_, body)| {
            body.split(';')
                .filter_map(|d| d.split_once(':'))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `value` with every `var(--x)` replaced by its light token value, recursively.
fn resolve_vars(value: &str) -> String {
    let root = root();
    let mut v = value.to_string();
    for _ in 0..5 {
        let Some(start) = v.find("var(--") else { break };
        let end = start + v[start..].find(')').expect("unclosed var(");
        let name = &v[start + 4..end];
        let token = root
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, val)| val.clone())
            .unwrap_or_else(|| format!("<unknown {name}>"));
        v.replace_range(start..=end, &token);
    }
    v
}

fn resolved(css: &str, selectors: &[&str], prop: &str) -> String {
    let mut got = None;
    for s in selectors {
        if let Some(v) = raw(css, s, prop) {
            got = Some(v);
        }
    }
    got.map(|v| resolve_vars(&v))
        .unwrap_or_else(|| "<missing>".into())
}

fn inline_padding(css: &str, selector: &str) -> String {
    if let Some(v) = raw(css, selector, "padding-inline") {
        return resolve_vars(&v);
    }
    let p = resolved(css, &[selector], "padding");
    p.split_whitespace()
        .nth(1)
        .or(p.split_whitespace().next())
        .unwrap_or("<missing>")
        .to_string()
}

fn all_static_css() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut out = String::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_some_and(|x| x == "css") {
            out.push_str(&fs::read_to_string(e.path()).unwrap());
            out.push('\n');
        }
    }
    out
}

#[test]
fn fidelity_frame_styles_resolve_to_the_board_values() {
    let pages = read("static/pages.css");
    let grade = read("static/grade.css");
    let button =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/button.css"))
            .unwrap_or_default();
    let everywhere = all_static_css();

    // (what, actual, expected)
    let mut rows: Vec<(String, String, String)> = Vec::new();
    let mut row = |what: &str, got: String, want: &str| {
        rows.push((what.to_string(), got, want.to_string()));
    };

    // C1: one container, 1120 px wide with 32 px inline padding.
    for sel in [".page-header", "main", ".page-footer"] {
        row(
            &format!("{sel} max-width"),
            resolved(&pages, &[sel], "max-width"),
            "1120px",
        );
        row(
            &format!("{sel} inline padding"),
            inline_padding(&pages, sel),
            "32px",
        );
    }

    // C4: grade letter and bar.
    row(
        "grade letter font-size",
        resolved(&grade, &[".grade__letter"], "font-size"),
        "88px",
    );
    row(
        "grade letter font-weight",
        resolved(&grade, &[".grade__letter"], "font-weight"),
        "700",
    );
    row(
        "grade letter letter-spacing",
        resolved(&grade, &[".grade__letter"], "letter-spacing"),
        "-0.04em",
    );
    row(
        "grade letter line-height",
        resolved(&grade, &[".grade__letter"], "line-height"),
        "0.85",
    );
    let bar = resolved(&grade, &[".grade"], "border-inline-start");
    row(
        "grade bar width",
        bar.split_whitespace()
            .next()
            .unwrap_or("<missing>")
            .to_string(),
        "4px",
    );
    row(
        "grade bar colour",
        raw(&grade, ".grade", "border-inline-start-color").unwrap_or_default(),
        "var(--grade-tone)",
    );
    for (tone, light) in [
        ("pass", "#1e7a4c"),
        ("warn", "#9a5f00"),
        ("fail", "#c42b22"),
    ] {
        let sel = format!(r#".grade[data-tone="{tone}"]"#);
        row(
            &format!("grade tone {tone} (light)"),
            resolved(&grade, &[&sel], "--grade-tone"),
            light,
        );
    }
    row(
        "grade tone pass names the text token",
        raw(&grade, r#".grade[data-tone="pass"]"#, "--grade-tone").unwrap_or_default(),
        "var(--status-pass-text)",
    );
    row(
        "pass text token (dark)",
        values::STATUS_DARK_PASS_TEXT.to_string(),
        "#4fd18f",
    );

    // C7: the result header's h1.
    let h1: &[&str] = &["#result h1", "#result-header h1", ".result-header h1"];
    row(
        "h1 font-size",
        resolved(&everywhere, h1, "font-size"),
        "40px",
    );
    row(
        "h1 font-weight",
        resolved(&everywhere, h1, "font-weight"),
        "650",
    );
    row(
        "h1 letter-spacing",
        resolved(&everywhere, h1, "letter-spacing"),
        "-0.03em",
    );
    row(
        "h1 line-height",
        resolved(&everywhere, h1, "line-height"),
        "1.1",
    );

    // C8: buttons, base plus modifier.
    let primary = [BUTTON, PRIMARY];
    let secondary = [BUTTON, SECONDARY];
    for (name, sels) in [("primary", &primary), ("secondary", &secondary)] {
        row(
            &format!("{name} min-height"),
            resolved(&button, sels, "min-height"),
            "44px",
        );
        row(
            &format!("{name} font-size"),
            resolved(&button, sels, "font-size"),
            "14px",
        );
        row(
            &format!("{name} radius"),
            resolved(&button, sels, "border-radius"),
            "2px",
        );
    }
    row(
        "primary padding",
        resolved(&button, &primary, "padding"),
        "0 18px",
    );
    row(
        "primary background",
        resolved(&button, &primary, "background"),
        "#0f1216",
    );
    row(
        "primary color",
        resolved(&button, &primary, "color"),
        "#f7f8fa",
    );
    row(
        "primary font-weight",
        resolved(&button, &primary, "font-weight"),
        "600",
    );
    row(
        "primary border",
        resolved(&button, &primary, "border"),
        "1px solid #0f1216",
    );
    row(
        "secondary background",
        resolved(&button, &secondary, "background"),
        "transparent",
    );
    row(
        "secondary color",
        resolved(&button, &secondary, "color"),
        "#0f1216",
    );
    row(
        "secondary border",
        resolved(&button, &secondary, "border"),
        "1px solid #e0e3e8",
    );

    let bad: Vec<String> = rows
        .iter()
        .filter(|(_, got, want)| got != want)
        .map(|(what, got, want)| format!("{what}: got {got:?}, want {want:?}"))
        .collect();
    assert!(
        bad.is_empty(),
        "{} rows differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

// --- rendered markup -------------------------------------------------------------------------

const DOMAIN: &str = "shop.example.com";

struct StubRuns {
    _starts: AtomicUsize,
}

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

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

#[tokio::test]
async fn fidelity_result_page_buttons_carry_the_button_classes() {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    store
        .record(DOMAIN, &run(), &[], now - 60, |p, c| diff(p, c, &[], None))
        .await
        .unwrap();
    let runs = Arc::new(StubRuns {
        _starts: AtomicUsize::new(0),
    });
    let app = router(ResultState::new(store, runs, 300));

    let resp = app
        .oneshot(
            Request::get(format!("/{DOMAIN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body =
        String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).into_owned();

    assert!(
        body.contains(r#"<button id="recheck-button" class="button button--primary""#),
        "primary button markup missing in {body}"
    );
    let at = body
        .find(r#"id="share""#)
        .unwrap_or_else(|| panic!("no share in {body}"));
    let open = body[..at].rfind('<').unwrap();
    let tag = &body[open..at + body[at..].find('>').unwrap()];
    assert!(
        tag.contains(r#"class="button button--secondary""#),
        "share control lacks the secondary button classes: {tag}"
    );
}
