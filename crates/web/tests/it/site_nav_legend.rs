//! V2 spec v2-site-nav, Phase 4 (requirement 12): the home grade legend and the inspector line.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_model::Grade;
use netray_store::HistoryStore;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};
use netray_web::grade_legend::descriptor;
use netray_web::pages::result::{ResultState, router};
use tower::ServiceExt;

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        let d = domain.trim().to_lowercase();
        Ok(d.strip_suffix('.').unwrap_or(&d).to_string())
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

async fn app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let state = ResultState::new(Arc::new(store), Arc::new(StubRuns), 3600);
    (router(state), dir)
}

async fn get(app: &Router, path: &str) -> (StatusCode, Option<String>, String) {
    let resp = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let location = resp
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        location,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn all_css() -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "css") {
            out.push(fs::read_to_string(p).unwrap());
        }
    }
    out
}

/// Declaration blocks (squashed) of rules whose selector list holds `selector` exactly.
fn blocks_in(css: &str, selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        if squash(&rest[..open])
            .split(',')
            .any(|s| s == squash(selector))
        {
            out.push(squash(&rest[open + 1..open + close]));
        }
        rest = &rest[open + close + 1..];
    }
    out
}

fn declared(blocks: &[String], decl: &str) -> bool {
    blocks.iter().any(|b| b.split(';').any(|d| d == decl))
}

fn rows() -> [(&'static str, &'static str, Grade); 6] {
    [
        ("A+", "pass", Grade::APlus),
        ("A", "pass", Grade::A),
        ("B", "pass", Grade::B),
        ("C", "warn", Grade::C),
        ("D", "warn", Grade::D),
        ("F", "fail", Grade::F),
    ]
}

#[tokio::test]
async fn c1_the_home_legend_holds_six_rows_from_grade_legend() {
    let (app, _dir) = app().await;
    let (status, _, body) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);

    let mut expected = String::from(
        "<section class=\"grade-legend\" aria-labelledby=\"grade-legend-title\">\n\
         <h2 id=\"grade-legend-title\">What the grades mean</h2>\n\
         <dl class=\"grade-legend__rows\">\n",
    );
    for (letter, tone, grade) in rows() {
        let (descriptor, meaning) = descriptor(grade).split_once(", ").unwrap();
        expected.push_str(&format!(
            "<div class=\"grade-legend__row\" data-tone=\"{tone}\"><dt class=\"grade-legend__letter\">{letter}</dt><dd><span class=\"grade-legend__descriptor\">{descriptor}</span> <span class=\"grade-legend__meaning\">{meaning}</span></dd></div>\n"
        ));
    }
    assert!(body.contains(&expected), "legend markup missing in {body}");

    let form_end = body.find("</form>").expect("no </form>");
    let section = body
        .find("<section class=\"grade-legend\"")
        .expect("no legend");
    let tagline = body
        .find("Every check of a domain, in one spectrum.")
        .expect("no tagline");
    assert!(form_end < section && section < tagline, "{body}");
}

#[tokio::test]
async fn c2_the_legend_lists_the_grades_and_links_the_scoring_guide() {
    let (app, _dir) = app().await;
    let (_, _, body) = get(&app, "/").await;

    let start = body
        .find("<section class=\"grade-legend\"")
        .expect("no legend");
    let end = start + body[start..].find("</section>").expect("legend unclosed");
    let legend = &body[start..end];
    let mut from = 0;
    for letter in ["A+", "A", "B", "C", "D", "F"] {
        let needle = format!("<dt class=\"grade-legend__letter\">{letter}</dt>");
        let at = legend[from..]
            .find(&needle)
            .unwrap_or_else(|| panic!("{needle} missing or out of order in {legend}"));
        from += at + needle.len();
    }
    assert!(legend.contains(">excellent</span>"), "{legend}");
    assert!(legend.contains(">ahead of most domains</span>"), "{legend}");
    assert!(legend.contains(">critical</span>"), "{legend}");
    assert!(legend.contains(">fix today</span>"), "{legend}");
    assert!(
        legend.contains("<p><a href=\"/guide/scoring\">How scoring works</a></p>"),
        "{legend}"
    );
}

#[tokio::test]
async fn c3_the_inspector_line_links_email_beside_the_others() {
    let (app, _dir) = app().await;
    let (_, _, page) = get(&app, "/").await;
    // The inspector line itself, not the menu or the footer.
    let start = page.find("One protocol only:").expect("inspector line");
    let body = &page[start..start + page[start..].find("</p>").unwrap()];

    assert!(
        body.contains("<a href=\"/inspect/email\">Email</a>"),
        "{body}"
    );
    for link in [
        "/inspect/dns",
        "/inspect/tls",
        "/inspect/http",
        "/inspect/ip",
    ] {
        assert!(body.contains(&format!("href=\"{link}\"")), "{link}");
    }
}

#[test]
fn c1_the_legend_styles_resolve_to_the_grade_tones() {
    let css = all_css().join("\n");
    let rows = [
        (".grade-legend__letter", "color:var(--grade-tone)"),
        (
            ".grade-legend__row[data-tone=\"pass\"]",
            "--grade-tone:var(--status-pass-text)",
        ),
        (
            ".grade-legend__row[data-tone=\"warn\"]",
            "--grade-tone:var(--status-warn-text)",
        ),
        (
            ".grade-legend__row[data-tone=\"fail\"]",
            "--grade-tone:var(--status-fail-text)",
        ),
        (".grade-legend__descriptor", "font-weight:600"),
        (".grade-legend__meaning", "color:var(--color-muted)"),
    ];
    for (selector, decl) in rows {
        assert!(
            declared(&blocks_in(&css, selector), decl),
            "{selector} must declare {decl}"
        );
    }
}
