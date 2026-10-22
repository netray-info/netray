//! The phone figure (spec v2-result-fidelity, Phase 6; requirements 37, 38, 39 and 16's phone
//! half; criteria C2, C3, C4, C6, C10, C11): the phone run title, hero, spectrum and stepper.
//! Markup comes through the router with 12 stored entries; CSS from the
//! `@media (max-width: 480px)` blocks of `static/*.css` and the phone token overrides.
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

// 2027-01-15 08:00:00 UTC; entry i is recorded i days later.
const T: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

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

/// Both checks flip with `i`, so every entry differs from its predecessor by two check changes.
fn run(i: usize) -> Run {
    let status = if i % 2 == 0 {
        Status::Pass
    } else {
        Status::Warn
    };
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![
            RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            },
            RunSection {
                protocol: Protocol::Tls,
                outcome: SectionKind::Measured,
                reason: None,
            },
            RunSection {
                protocol: Protocol::Email,
                outcome: SectionKind::Measured,
                reason: None,
            },
        ],
        checks: ["dns.dnssec_valid", "tls.chain_trusted", "email.spf"]
            .iter()
            .map(|id| RunCheck {
                id: CheckId::parse(id).unwrap(),
                status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

async fn app(entries: usize) -> (Router, Arc<HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    for i in 0..entries {
        store
            .record("example.com", &run(i), &[], T + i as i64 * DAY, |p, c| {
                diff(p, c, &[], None)
            })
            .await
            .unwrap();
    }
    let store = Arc::new(store);
    let app = router(ResultState::new(store.clone(), Arc::new(StubRuns), 3600));
    (app, store, dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

const PROFILE_PART: &str = "run-line__part\" data-part=\"profile\"";
const REGION_NAME_CLASS: &str = "spectrum__region-name";
const STEPPER_RUN_CLASS: &str = "spectrum-stepper__run";
const STEPPER_STATE_CLASS: &str = "spectrum-stepper__state";
const STEPPER_HINT_CLASS: &str = "spectrum-stepper__hint";
const HINT_TEXT: &str = "Tap a line for its history. Step back with ‹ to replay a run.";

async fn page() -> String {
    let (app, _store, _dir) = app(12).await;
    let (status, body) = get(&app, "/example.com").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

// ---- CSS helpers ----------------------------------------------------------------------------

struct Rule {
    selectors: Vec<String>,
    decls: Vec<(String, String)>,
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = match rest[i..].find("*/") {
            Some(j) => &rest[i + j + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// Top-level `(prelude, body)` pairs, brace-matched.
fn top_blocks(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut body_start) = (0usize, 0usize, 0usize);
    for (i, c) in css.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    body_start = i + 1;
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push((
                        css[start..body_start - 1].trim().to_string(),
                        css[body_start..i].to_string(),
                    ));
                    start = i + 1;
                }
            }
            _ => {}
        }
    }
    out
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_rules(css: &str) -> Vec<Rule> {
    top_blocks(css)
        .into_iter()
        .filter(|(p, _)| !p.starts_with('@'))
        .map(|(p, body)| Rule {
            selectors: p.split(',').map(norm).collect(),
            decls: body
                .split(';')
                .filter_map(|d| d.split_once(':'))
                .map(|(k, v)| (k.trim().to_string(), norm(v)))
                .collect(),
        })
        .collect()
}

fn static_css() -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "css"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| strip_comments(&std::fs::read_to_string(p).unwrap()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_phone_media(prelude: &str) -> bool {
    prelude.starts_with("@media")
        && prelude
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains("max-width:480px")
}

/// Rules of every `@media (max-width: 480px)` block in `static/*.css`.
fn phone_rules() -> Vec<Rule> {
    top_blocks(&static_css())
        .into_iter()
        .filter(|(p, _)| is_phone_media(p))
        .flat_map(|(_, body)| parse_rules(&body))
        .collect()
}

/// Rules outside any at-rule.
fn base_rules() -> Vec<Rule> {
    parse_rules(&static_css())
}

/// Custom properties at phone width: `:root` of the token CSS, then the phone block over it.
fn phone_vars() -> Vec<(String, String)> {
    let css = strip_comments(netray_web::tokens::CSS);
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut push = |rules: Vec<Rule>, only_root: bool| {
        for r in rules {
            if only_root && !r.selectors.iter().any(|s| s == ":root") {
                continue;
            }
            vars.extend(r.decls.into_iter().filter(|(k, _)| k.starts_with("--")));
        }
    };
    let blocks = top_blocks(&css);
    push(parse_rules(&css), true);
    for (p, body) in blocks.iter().filter(|(p, _)| is_phone_media(p)) {
        let _ = p;
        push(parse_rules(body), false);
    }
    vars
}

fn resolve(value: &str) -> String {
    let vars = phone_vars();
    let mut v = value.to_string();
    for _ in 0..8 {
        let Some(i) = v.find("var(") else { break };
        let j = i + v[i..].find(')').unwrap();
        let name = v[i + 4..j].trim().to_string();
        let val = vars
            .iter()
            .rev()
            .find(|(k, _)| *k == name)
            .map(|(_, x)| x.clone())
            .unwrap_or_else(|| panic!("no token {name}"));
        v = format!("{}{}{}", &v[..i], val, &v[j + 1..]);
    }
    v
}

/// The phone-width value of `prop` for the rules whose selector satisfies `pred`: base rules,
/// then phone rules, the last declaration wins; `var()` resolves against the phone tokens.
fn effective(pred: impl Fn(&str) -> bool, prop: &str) -> Option<String> {
    let mut found = None;
    for r in base_rules().into_iter().chain(phone_rules()) {
        if !r.selectors.iter().any(|s| pred(s)) {
            continue;
        }
        for (k, v) in &r.decls {
            if k == prop {
                found = Some(resolve(v));
            }
        }
    }
    found
}

fn phone_value(pred: impl Fn(&str) -> bool, prop: &str) -> Option<String> {
    let mut found = None;
    for r in phone_rules() {
        if r.selectors.iter().any(|s| pred(s)) {
            for (k, v) in &r.decls {
                if k == prop {
                    found = Some(resolve(v));
                }
            }
        }
    }
    found
}

fn eq(sel: &'static str) -> impl Fn(&str) -> bool {
    move |s| s == sel
}

fn text_between<'a>(body: &'a str, open: &str) -> &'a str {
    let rest = body
        .split(open)
        .nth(1)
        .unwrap_or_else(|| panic!("no {open} in {body}"));
    rest.split("</div>")
        .next()
        .unwrap()
        .split("</p>")
        .next()
        .unwrap()
}

// ---- C2 / R37 -------------------------------------------------------------------------------

#[tokio::test]
async fn c2_phone_title_hides_the_profile_part_and_hero_takes_phone_values() {
    let body = page().await;
    let title = body
        .split("<p class=\"run-line run-line--title\">")
        .nth(1)
        .unwrap_or_else(|| panic!("no run title in {body}"))
        .split("</p>")
        .next()
        .unwrap();
    assert!(
        title.contains(PROFILE_PART),
        "profile part is not marked: {title}"
    );
    assert!(title.contains("Run 12"), "{title}");

    let hides = phone_value(
        |s| s.contains(".run-line--title") && s.contains("[data-part=\"profile\"]"),
        "display",
    );
    assert_eq!(
        hides.as_deref(),
        Some("none"),
        "phone rule hiding the profile part"
    );

    assert_eq!(
        effective(eq(".grade__letter"), "font-size").as_deref(),
        Some("64px")
    );
    assert_eq!(
        effective(|s| s.contains("h1") && s.contains("result"), "font-size").as_deref(),
        Some("26px")
    );
    assert_eq!(
        effective(|s| s.contains("h1") && s.contains("result"), "line-height").as_deref(),
        Some("1.15")
    );
    assert_eq!(
        phone_value(eq(".result-hero__actions"), "width").as_deref(),
        Some("100%"),
        "actions row is full width"
    );
    assert_eq!(
        phone_value(eq("#recheck-button"), "flex").as_deref(),
        Some("1")
    );
    assert_eq!(
        effective(eq(".button"), "font-size").as_deref(),
        Some("15px")
    );
}

// ---- C3 / C10 / C6 (R38, R16 phone half) ----------------------------------------------------

#[tokio::test]
async fn c3_c10_c6_phone_spectrum_drops_the_min_width_and_sets_labels() {
    let body = page().await;
    let box_tag = body
        .split("<div class=\"spectrum__box\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no spectrum box in {body}"))
        .split('>')
        .next()
        .unwrap();
    let inline_forces = box_tag.contains("min-width");
    let phone_resets = phone_value(eq(".spectrum__box"), "min-width").as_deref() == Some("0")
        || phone_value(eq(".spectrum__box"), "min-width").as_deref() == Some("0px");
    assert!(
        !inline_forces || phone_resets,
        "inline min-width forces a width at phone width: {box_tag}"
    );

    assert_eq!(
        effective(eq(".spectrum__line-label"), "font-size").as_deref(),
        Some("10px")
    );
    assert_eq!(
        effective(eq(".spectrum__region-label"), "font-size").as_deref(),
        Some("9px")
    );
    assert_eq!(
        effective(eq(".spectrum__region-label"), "letter-spacing").as_deref(),
        Some(".1em")
    );

    // The email region carries both names; the phone rule shows the short one.
    let email = body
        .split("<span class=\"spectrum__region-label\" data-protocol=\"EMAIL\"")
        .nth(1)
        .unwrap_or_else(|| panic!("no email region label in {body}"))
        .split("</span></span>")
        .next()
        .unwrap();
    assert!(
        email.contains(&format!(
            "class=\"{REGION_NAME_CLASS}\" data-short=\"MAIL\">EMAIL<"
        )),
        "email region name markup: {email}"
    );
    let shows = phone_value(
        |s| s.contains(REGION_NAME_CLASS) && s.contains("::after"),
        "content",
    );
    assert_eq!(
        shows.as_deref(),
        Some("attr(data-short) / \"\""),
        "phone rule showing MAIL"
    );
    assert!(
        base_rules()
            .iter()
            .flat_map(|r| &r.selectors)
            .all(|s| !(s.contains(REGION_NAME_CLASS) && s.contains("::after"))),
        "the short name must be a phone-only rule"
    );
}

// ---- C4 / C11 (R39) -------------------------------------------------------------------------

#[tokio::test]
async fn c4_c11_phone_stepper_shows_run_state_hint_and_history_link() {
    let body = page().await;
    let stepper = body
        .split("<div id=\"spectrum-stepper\">")
        .nth(1)
        .unwrap_or_else(|| panic!("no stepper in {body}"))
        .split("<div id=\"trace\">")
        .next()
        .unwrap();
    let run = text_between(stepper, &format!("<div class=\"{STEPPER_RUN_CLASS}\">"));
    let day = run
        .split("<strong>")
        .nth(2)
        .and_then(|s| s.split("</strong>").next())
        .unwrap_or_else(|| panic!("no date in {run}"));
    assert_eq!(day.len(), 10, "date is YYYY-MM-DD: {run}");
    assert!(day.starts_with("2027-01-"), "{run}");
    assert!(run.contains("Run <strong>12</strong> · <strong>"), "{run}");
    assert!(
        run.contains("</strong> · grade <strong>A</strong>"),
        "{run}"
    );
    assert!(
        stepper.contains(&format!(
            "<span class=\"{STEPPER_STATE_CLASS}\">latest run</span>"
        )),
        "{stepper}"
    );
    assert!(
        body.contains(&format!(
            "<p class=\"{STEPPER_HINT_CLASS}\">{HINT_TEXT}</p>"
        )),
        "hint markup"
    );
    let hint_at = body.find(STEPPER_HINT_CLASS).unwrap();
    let link_at = body.find("Show history as a table").expect("history link");
    assert!(hint_at < link_at, "the hint precedes the history link");

    // Hint hidden on desktop, shown on the phone.
    assert_eq!(
        base_rules()
            .iter()
            .filter(|r| r.selectors.iter().any(|s| s.contains(STEPPER_HINT_CLASS)))
            .flat_map(|r| r.decls.clone())
            .filter(|(k, _)| k == "display")
            .last()
            .map(|(_, v)| v),
        Some("none".to_string())
    );
    assert!(
        phone_value(|s| s.contains(STEPPER_HINT_CLASS), "display").is_some_and(|v| v != "none"),
        "hint shown at phone width"
    );

    // Trace rows 6 px, stepper buttons 44 x 44 with a hairline border.
    let row = phone_vars()
        .into_iter()
        .rev()
        .find(|(k, _)| k == "--spectrum-trace-row")
        .map(|(_, v)| resolve(&v));
    assert_eq!(
        row.as_deref(),
        Some("6px"),
        "--spectrum-trace-row at phone width"
    );
    let btn = |p: &str| phone_value(eq("#spectrum-stepper button"), p);
    assert_eq!(btn("min-width").as_deref(), Some("44px"));
    assert_eq!(btn("min-height").as_deref(), Some("44px"));
    assert!(
        btn("border").is_some_and(|b| b.starts_with("1px solid")),
        "button border: {:?}",
        btn("border")
    );
    assert_eq!(btn("border-radius").as_deref(), Some("2px"));
}
