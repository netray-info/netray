//! Guides through the composed router (v2-pages requirements 17 and 18;
//! criteria C1 list half, C2, C7, C8).

use std::collections::HashSet;
use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages::{self, guides};
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn get_uri(uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn v1_guide_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../site/guide")
}

fn v1_to_v2(slug: &str) -> String {
    if slug == "lens" {
        "scoring".into()
    } else {
        slug.into()
    }
}

/// Escapes the way askama does: `& < > " '`.
fn esc(s: &str) -> String {
    s.replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn unescape_html(s: &str) -> String {
    s.replace("&amp;", "&")
}

/// V1 index: (category, [slug]) in document order, `lens` mapped to `scoring`.
fn v1_index() -> Vec<(String, Vec<String>)> {
    let html = std::fs::read_to_string(v1_guide_dir().join("index.html")).unwrap();
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in html.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("<h2>") {
            let name = rest.strip_suffix("</h2>").unwrap();
            out.push((unescape_html(name), Vec::new()));
        } else if let Some(rest) = line.strip_prefix("<li><a href=\"") {
            let slug = rest.split('"').next().unwrap();
            out.last_mut().unwrap().1.push(v1_to_v2(slug));
        }
    }
    out
}

#[test]
fn guides_list_matches_the_v1_files_and_is_well_formed() {
    let mut expected: Vec<String> = std::fs::read_dir(v1_guide_dir())
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            let slug = name.strip_suffix(".html")?.to_string();
            (slug != "index").then(|| v1_to_v2(&slug))
        })
        .collect();
    expected.sort();
    assert_eq!(expected.len(), 32);

    let mut actual: Vec<&str> = guides::GUIDES.iter().map(|g| g.slug).collect();
    actual.sort();
    assert_eq!(actual, expected);

    let unique: HashSet<&str> = guides::GUIDES.iter().map(|g| g.slug).collect();
    assert_eq!(unique.len(), guides::GUIDES.len(), "slugs unique");

    assert_eq!(guides::find("dnssec").unwrap().slug, "dnssec");
    assert!(guides::find("nope").is_none());

    for g in guides::GUIDES {
        for r in g.related {
            assert!(unique.contains(r), "{}: related {r} is not a guide", g.slug);
        }
        assert!(!g.title.is_empty(), "{}: title", g.slug);
        assert!(!g.description.is_empty(), "{}: description", g.slug);
        assert!(!g.category.is_empty(), "{}: category", g.slug);
    }
}

#[test]
fn guides_order_and_categories_equal_the_v1_index() {
    let expected: Vec<(String, String)> = v1_index()
        .into_iter()
        .flat_map(|(cat, slugs)| slugs.into_iter().map(move |s| (s, cat.clone())))
        .collect();
    assert_eq!(expected.len(), 32);
    let actual: Vec<(String, String)> = guides::GUIDES
        .iter()
        .map(|g| (g.slug.to_string(), g.category.to_string()))
        .collect();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn every_guide_answers_200_on_the_frame_with_its_metadata() {
    for g in guides::GUIDES {
        let uri = format!("/guide/{}", g.slug);
        let (status, headers, body) = get_uri(&uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: status");
        let ct = headers[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ct.starts_with("text/html"), "{uri}: content-type {ct}");
        assert!(body.contains(r#"<html lang="en">"#), "{uri}: frame");
        let title = esc(g.title);
        assert!(
            body.contains("<title>") && body.contains(&title),
            "{uri}: title {title}"
        );
        let title_tag = body
            .split("<title>")
            .nth(1)
            .and_then(|s| s.split("</title>").next())
            .unwrap_or("");
        assert!(title_tag.contains(&title), "{uri}: <title> is {title_tag}");
        let desc = format!(
            r#"<meta name="description" content="{}">"#,
            esc(g.description)
        );
        assert!(body.contains(&desc), "{uri}: {desc}");
        let canon = format!(
            r#"<link rel="canonical" href="https://netray.info/guide/{}">"#,
            g.slug
        );
        assert!(body.contains(&canon), "{uri}: {canon}");
        let og = format!(
            r#"<meta property="og:image" content="https://netray.info/static/og/{}">"#,
            g.og_image
        );
        assert!(body.contains(&og), "{uri}: {og}");
    }
}

#[tokio::test]
async fn trailing_slash_and_unknown_guides_are_404_and_bare_guide_redirects() {
    for uri in ["/guide/dnssec/", "/guide/nope"] {
        let (status, headers, body) = get_uri(uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: status");
        let ct = headers[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ct.starts_with("text/html"), "{uri}: content-type {ct}");
        assert!(body.contains(r#"<html lang="en">"#), "{uri}: frame");
    }
    let (status, headers, _) = get_uri("/guide").await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(headers[header::LOCATION], "/guide/");
}

#[tokio::test]
async fn guide_index_lists_every_guide_grouped_in_v1_order() {
    let (status, _, body) = get_uri("/guide/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"<html lang="en">"#), "frame");

    let mut cursor = 0usize;
    for (cat, slugs) in v1_index() {
        let heading_pos = ["<h2", "<h3"]
            .iter()
            .flat_map(|tag| {
                body[cursor..]
                    .match_indices(tag)
                    .map(|(i, _)| i + cursor)
                    .collect::<Vec<_>>()
            })
            .filter(|&i| {
                let end = body[i..].find("</h").map(|e| i + e).unwrap_or(body.len());
                let text = &body[i..end];
                text.contains(&esc(&cat))
                    || text.contains(&cat)
                    || text.contains(&unescape_html(&cat).replace('&', "&amp;"))
            })
            .min()
            .unwrap_or_else(|| panic!("heading for category {cat:?} after byte {cursor}"));
        cursor = heading_pos;
        for slug in slugs {
            let link = format!(r#"href="https://netray.info/guide/{slug}""#);
            let pos = body[cursor..]
                .find(&link)
                .unwrap_or_else(|| panic!("{link} under {cat:?}, in order"));
            assert_eq!(body.matches(&link).count(), 1, "{link}: exactly one link");
            cursor += pos;
        }
    }
}
