//! v2-ui-polish, requirements 2 and 3: the DNS builder is three labelled rows of toggle labels
//! (C1/C7) and the `+check` note is server-rendered, hidden unless the query carries it (C2/C8).
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
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
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
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

fn all_css() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let mut names: Vec<_> = fs::read_dir(dir)
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

/// Bodies of every rule whose selector list contains `needle`.
fn rule_bodies(css: &str, needle: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in css.split('}') {
        if let Some((sel, body)) = chunk.rsplit_once('{')
            && sel.contains(needle)
        {
            out.push(body.to_string());
        }
    }
    out
}

#[tokio::test]
async fn c1_c7_the_builder_is_three_labelled_rows_of_toggle_labels() {
    let body = get("/inspect/dns").await;

    let root = tag_with(&body, r#"data-island="dns""#).expect("island");
    assert!(root.starts_with("<div"), "{root}");
    assert!(root.contains(r#"class="dns-builder""#), "{root}");

    assert_eq!(
        body.matches(r#"class="dns-builder__row""#).count(),
        3,
        "{body}"
    );
    for label in ["Types", "Job", "Resolver"] {
        let span = format!(r#"class="dns-builder__label""#);
        let at = body
            .find(&format!(">{label}</span>"))
            .unwrap_or_else(|| panic!("no label {label}"));
        let open = body[..at].rfind("<span").unwrap();
        let tag = &body[open..at];
        assert!(tag.contains(&span), "{label}: {tag}");
        let id = tag
            .split(r#"id=""#)
            .nth(1)
            .and_then(|s| s.split('"').next())
            .unwrap_or_else(|| panic!("{label}: label span has no id: {tag}"));
        assert!(
            body.contains(&format!(r#"aria-labelledby="{id}""#)),
            "{label}: no row points at {id}"
        );
    }
    assert_eq!(body.matches(r#"role="group""#).count() >= 3, true, "{body}");

    assert!(body.contains(r#"<select"#));
    let select = tag_with(&body, r#"name="dns-resolver""#).expect("select");
    assert!(select.contains("dns-builder__select"), "{select}");

    let toggles = body
        .matches(r#"<label class="dns-builder__toggle""#)
        .count();
    let inputs =
        body.matches(r#"name="dns-type""#).count() + body.matches(r#"name="dns-job""#).count();
    assert!(inputs >= 8, "{inputs}");
    assert_eq!(
        toggles, inputs,
        "every checkbox and radio sits in a toggle label"
    );
    let query = tag_with(&body, r#"value="query""#).expect("query radio");
    assert!(query.contains("checked"), "{query}");

    let css = all_css();
    let checked = rule_bodies(&css, ".dns-builder__toggle:has(input:checked)");
    assert!(
        checked.iter().any(|b| b
            .replace(' ', "")
            .contains("border-inline-start-width:var(--line-marker)")),
        "no checked-marker rule: {checked:?}"
    );
    assert!(
        !rule_bodies(&css, ".dns-builder__toggle:has(input:focus-visible)").is_empty(),
        "no focus-visible rule"
    );
}

#[tokio::test]
async fn c2_c8_the_check_note_is_hidden_until_the_query_carries_check() {
    let note = "+check is not a separate job: the DNS band covers its checks.";

    let idle = get("/inspect/dns").await;
    let tag = tag_with(&idle, "dns-builder__note").expect("note on idle page");
    assert!(tag.starts_with("<p") && tag.contains("hidden"), "{tag}");
    assert!(idle.contains(note), "{idle}");

    let flagged = get("/inspect/dns?q=example.com%20%2Bcheck").await;
    let tag = tag_with(&flagged, "dns-builder__note").expect("note on +check page");
    assert!(!tag.contains("hidden"), "{tag}");
    assert!(flagged.contains(note), "{flagged}");

    let tls = get("/inspect/tls").await;
    assert!(!tls.contains("dns-builder__note"), "{tls}");
}
