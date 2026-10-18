//! specs/features/v2-pages, requirement 21 (criterion C5): a guide with JSON-LD in V1 renders the
//! same number of `application/ld+json` blocks; each body is JSON with `<`, `>`, `&` escaped, the
//! same `@type`/`@context` as V1 and no codename in a string value. Guides without V1 JSON-LD
//! render none.

use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages::{self, guides};
use serde_json::Value;
use tower::ServiceExt;

const WORDS: [&str; 6] = [
    "lens",
    "prism",
    "tlsight",
    "spectra",
    "beacon",
    "ifconfig-rs",
];
const OPEN: &str = r#"<script type="application/ld+json">"#;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn render(uri: &str) -> String {
    let res = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "{uri}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// The bodies of every `<script type="application/ld+json">` block, in order.
fn blocks(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(OPEN) {
        let after = &rest[i + OPEN.len()..];
        let end = after.find("</script>").expect("unterminated ld+json block");
        out.push(after[..end].to_string());
        rest = &after[end..];
    }
    out
}

fn v1_file(slug: &str) -> PathBuf {
    let name = if slug == "scoring" { "lens" } else { slug };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../site/guide")
        .join(format!("{name}.html"))
}

fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

fn has_word(text: &str, word: &str) -> bool {
    let t = text.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = t[from..].find(word) {
        let s = from + i;
        let e = s + word.len();
        let ok = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        if ok(t[..s].chars().next_back()) && ok(t[e..].chars().next()) {
            return true;
        }
        from = s + 1;
    }
    false
}

async fn check(uri: &str, v1_path: PathBuf) {
    let v1 = blocks(&std::fs::read_to_string(&v1_path).unwrap());
    let html = render(uri).await;
    let got = blocks(&html);
    assert_eq!(got.len(), v1.len(), "{uri}: ld+json block count");
    for (i, (body, old)) in got.iter().zip(&v1).enumerate() {
        for c in ['<', '>', '&'] {
            assert!(!body.contains(c), "{uri} block {i}: raw `{c}`");
        }
        let new: Value =
            serde_json::from_str(body).unwrap_or_else(|e| panic!("{uri} block {i}: not JSON: {e}"));
        let old: Value = serde_json::from_str(old).unwrap();
        for key in ["@type", "@context"] {
            assert_eq!(new.get(key), old.get(key), "{uri} block {i}: {key}");
        }
        let mut all = Vec::new();
        strings(&new, &mut all);
        for s in &all {
            assert!(
                !s.to_ascii_lowercase().contains("lens.netray.info"),
                "{uri} block {i}: lens.netray.info in {s:?}"
            );
            for w in WORDS {
                assert!(!has_word(s, w), "{uri} block {i}: codename `{w}` in {s:?}");
            }
        }
    }
}

#[tokio::test]
async fn guides_render_their_v1_json_ld_escaped_and_without_codenames() {
    assert!(!guides::GUIDES.is_empty());
    for g in guides::GUIDES {
        check(&format!("/guide/{}", g.slug), v1_file(g.slug)).await;
    }
    check("/guide/", v1_file("index")).await;
}
