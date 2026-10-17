//! Each rendered `/guide/<slug>` keeps what its V1 page `site/guide/<name>.html` said
//! (v2-pages requirements 17-22: anchors, tool entries as query inputs, related
//! guides, rewritten API references).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages::{self, guides};
use std::collections::BTreeSet;
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn render(slug: &str) -> String {
    let res = app()
        .oneshot(
            Request::builder()
                .uri(format!("/guide/{slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "/guide/{slug}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn v1_name(slug: &str) -> &str {
    if slug == "scoring" { "lens" } else { slug }
}

fn v1_source(slug: &str) -> String {
    let path = format!(
        "{}/../../site/guide/{}.html",
        env!("CARGO_MANIFEST_DIR"),
        v1_name(slug)
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The V1 `<main ...>...</main>` inner markup.
fn main_of(html: &str) -> &str {
    let start = html.find("<main").expect("<main");
    let start = start + html[start..].find('>').unwrap() + 1;
    let end = start + html[start..].find("</main>").expect("</main>");
    &html[start..end]
}

/// Value of `name="..."` inside one tag's text.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let mut from = 0;
    while let Some(i) = tag[from..].find(&key) {
        let at = from + i;
        let boundary = tag[..at].chars().last().is_none_or(|c| c.is_whitespace());
        if boundary {
            let v = at + key.len();
            return Some(&tag[v..v + tag[v..].find('"')?]);
        }
        from = at + key.len();
    }
    None
}

/// Every `<tag ...>` start tag text (without the angle brackets) in order.
fn tags<'a>(html: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let mut out = vec![];
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        if after.starts_with(|c: char| c.is_whitespace() || c == '>') {
            let end = after.find('>').unwrap_or(after.len());
            out.push(&after[..end]);
        }
        rest = after;
    }
    out
}

fn ids(html: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = html;
    while let Some(i) = rest.find(" id=\"") {
        let v = &rest[i + 5..];
        let end = v.find('"').unwrap();
        out.insert(v[..end].to_string());
        rest = &v[end..];
    }
    out
}

/// (action, name, placeholder) of every form whose opening tag has `class`.
fn forms(html: &str, class: &str) -> Vec<(String, String, String)> {
    let mut out = vec![];
    let mut rest = html;
    while let Some(i) = rest.find("<form") {
        let seg = &rest[i..];
        let end = seg.find("</form>").unwrap_or(seg.len());
        let body = &seg[..end];
        let open = &body[..body.find('>').unwrap_or(body.len())];
        if attr(open, "class") == Some(class) {
            let input = tags(body, "input")
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("form without input: {open}"));
            out.push((
                attr(open, "action").unwrap_or_default().to_string(),
                attr(input, "name").unwrap_or_default().to_string(),
                attr(input, "placeholder").unwrap_or_default().to_string(),
            ));
        }
        rest = &seg[5..];
    }
    out
}

#[tokio::test]
async fn rendered_guides_keep_the_content_of_their_v1_pages() {
    let mut failures: Vec<String> = vec![];
    assert_eq!(guides::GUIDES.len(), 32);

    for g in guides::GUIDES {
        let slug = g.slug;
        let v1 = v1_source(slug);
        let v1_main = main_of(&v1);
        let page = render(slug).await;

        // C10: every anchor id of V1's main exists in the rendered page.
        let have = ids(&page);
        let missing: Vec<String> = ids(v1_main).difference(&have).cloned().collect();
        if !missing.is_empty() {
            failures.push(format!("{slug}: missing ids {missing:?}"));
        }

        // Tool entries become query inputs.
        let rendered: Vec<_> = forms(&page, "query-input");
        for (action, name, placeholder) in forms(v1_main, "tool-entry__form") {
            let (action, name) = if action == "https://lens.netray.info/" {
                ("/".to_string(), "d".to_string())
            } else {
                (action, name)
            };
            if !rendered.contains(&(action.clone(), name.clone(), placeholder.clone())) {
                failures.push(format!(
                    "{slug}: no query-input form action={action} name={name} placeholder={placeholder}"
                ));
            }
        }
        for gone in [
            "tool-entry",
            "page-footer__suite",
            "lens.netray.info",
            "/api/lens",
            "/api/check/",
        ] {
            if page.contains(gone) {
                failures.push(format!("{slug}: still holds {gone}"));
            }
        }

        // Related guides: every V1 guide link resolves to /guide/<slug>.
        for a in tags(v1_main, "a") {
            let Some(href) = attr(a, "href") else {
                continue;
            };
            let target = if let Some(s) = href.strip_prefix("/guide/") {
                s
            } else if !href.is_empty() && !href.contains(['/', ':', '#', '.']) {
                href
            } else {
                continue;
            };
            let target = if target == "lens" { "scoring" } else { target };
            let want = format!("href=\"/guide/{target}\"");
            if !page.contains(&want) {
                failures.push(format!("{slug}: lost link {want}"));
            }
        }
    }

    // Rewritten references.
    let scoring = render("scoring").await;
    if !scoring.contains(r#"<a href="/api/v1/openapi.json">netray API reference</a>"#) {
        failures.push(
            "scoring: no link href=\"/api/v1/openapi.json\" with text netray API reference".into(),
        );
    }
    let audit = render("domain-security-audit").await;
    if !audit.contains(r#"href="/""#) {
        failures.push("domain-security-audit: no href=\"/\"".into());
    }
    if !audit.contains("https://netray.info/api/v1/domains/example.com/runs") {
        failures.push("domain-security-audit: curl samples lack the runs URL".into());
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
