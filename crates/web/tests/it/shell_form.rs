//! v2-inspector-shell, requirement 4 (C4): the query form of every inspector page is a card
//! with a labelled 44 px mono input, a clear button and an "Inspect" submit.
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use netray_web::tokens::CSS;
use tower::ServiceExt;

struct Stub;

impl InspectorJobs for Stub {
    fn admit(&self, _tool: Tool, _client: IpAddr, _input: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _req: JobRequest) -> Result<Job, String> {
        Err("stub".into())
    }
}

async fn get(uri: &str) -> String {
    let app: Router = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
    let res = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "{uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The opening tag of the first element whose tag text contains `needle`.
fn tag_with<'a>(html: &'a str, needle: &str) -> Option<&'a str> {
    let at = html.find(needle)?;
    let start = html[..at].rfind('<')?;
    let end = at + html[at..].find('>')?;
    Some(&html[start..=end])
}

#[tokio::test]
async fn c4_query_form_is_a_card_with_label_clear_button_and_inspect_submit() {
    let rows = [
        ("dns", "q", "Query"),
        ("tls", "h", "Host"),
        ("http", "url", "URL"),
        ("ip", "ip", "Address"),
    ];
    for (tool, field, label) in rows {
        let body = get(&format!("/inspect/{tool}")).await;
        let form = tag_with(&body, "query-input--card")
            .unwrap_or_else(|| panic!("{tool}: no card form: {body}"));
        assert!(form.starts_with("<form"), "{tool}: {form}");
        assert!(
            form.contains("class=\"query-input query-input--card\""),
            "{tool}: {form}"
        );
        assert!(
            body.contains(&format!(">{label}</label>")),
            "{tool}: no label {label}"
        );
        assert!(
            body.contains(&format!("name=\"{field}\"")),
            "{tool}: no field"
        );

        let clear = tag_with(&body, "query-input__clear")
            .unwrap_or_else(|| panic!("{tool}: no clear button"));
        assert!(clear.starts_with("<button"), "{tool}: {clear}");
        assert!(clear.contains("type=\"button\""), "{tool}: {clear}");
        assert!(
            clear.contains("hidden"),
            "{tool}: empty input, clear shown: {clear}"
        );
        assert!(body.contains(">×</button>"), "{tool}: no × glyph");

        let submit = tag_with(&body, "query-input__submit").expect("submit");
        let at = body.find(submit).unwrap() + submit.len();
        assert!(
            body[at..].trim_start().starts_with("Inspect</button>"),
            "{tool}: submit text"
        );
    }
}

#[tokio::test]
async fn c4_clear_button_is_shown_when_the_page_is_prefilled() {
    let bare = get("/inspect/tls").await;
    let filled = get("/inspect/tls?h=example.com").await;

    let hidden = tag_with(&bare, "query-input__clear").expect("clear on bare page");
    let shown = tag_with(&filled, "query-input__clear").expect("clear on prefilled page");
    assert!(hidden.contains("hidden"), "{hidden}");
    assert!(!shown.contains("hidden"), "{shown}");
    assert!(filled.contains("value=\"example.com\""), "{filled}");
}

fn all_css() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut names: Vec<_> = fs::read_dir(dir.clone())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "css"))
        .collect();
    names.sort();
    names
        .iter()
        .map(|p| fs::read_to_string(p).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The last value `prop` takes in rules whose selector list holds exactly `selector`.
fn decl(css: &str, selector: &str, prop: &str) -> Option<String> {
    let mut found = None;
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let prelude: Vec<&str> = rest[..open].split(',').map(str::trim).collect();
        if prelude
            .iter()
            .any(|s| s.ends_with(selector) && s.split_whitespace().last() == Some(selector))
        {
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
    found
}

/// A `var(--x)` value resolved through the light `:root` tokens; other values unchanged.
fn resolve(v: &str) -> String {
    let Some(name) = v.strip_prefix("var(").and_then(|s| s.strip_suffix(')')) else {
        return v.to_string();
    };
    let name = name.split(',').next().unwrap().trim();
    for line in CSS.split(';') {
        if let Some((k, val)) = line.split_once(':')
            && k.trim().ends_with(name)
        {
            return val.trim().to_string();
        }
    }
    v.to_string()
}

#[test]
fn c4_card_and_field_css_carry_the_board_values() {
    let css = all_css();

    let padding = decl(&css, ".query-input--card", "padding").expect("card padding");
    assert_eq!(resolve(&padding), "18px", "card padding: {padding}");
    let bg = decl(&css, ".query-input--card", "background").expect("card background");
    assert_eq!(bg, "var(--color-surface)");

    let height = decl(&css, ".query-input__field", "height").expect("field height");
    assert_eq!(resolve(&height), "44px", "field height: {height}");
    let font = decl(&css, ".query-input__field", "font-family").expect("field font");
    assert_eq!(font, "var(--font-mono)");
    let size = decl(&css, ".query-input__field", "font-size").expect("field size");
    assert_eq!(resolve(&size), "16px", "field size: {size}");
}
