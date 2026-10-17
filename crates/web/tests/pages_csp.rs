//! V2 spec v2-pages, Phase 3 (requirements 11, 12; criteria C4, C5, C15, C16, C17):
//! every route of the composed router carries the security headers, and the CSP's script
//! hash is the hash of the speculation rules.
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response};
use axum::routing::get;
use base64::Engine;
use http_body_util::BodyExt;
use netray_web::pages;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const RULES: &str = r#"{"prerender":[{"where":{"or":[{"href_matches":"/guide/*"},{"href_matches":"/r/*"}]},"eagerness":"moderate"}]}"#;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .route("/robots.txt", get(pages::robots))
        .merge(pages::router())
        .fallback(pages::not_found)
}

fn hash(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(text.as_bytes()))
}

fn csp_for(h: &str) -> String {
    format!(
        "default-src 'self'; script-src 'self' 'sha256-{h}'; style-src 'self' 'unsafe-inline'; \
         font-src 'self'; img-src 'self' data:; connect-src 'self' https://stats.uptimerobot.com; \
         frame-ancestors 'none'; base-uri 'self'; object-src 'none'; \
         form-action 'self' https://*.netray.info"
    )
}

async fn get_path(path: &str) -> Response<Body> {
    app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn header(res: &Response<Body>, name: &str) -> String {
    res.headers()
        .get(name)
        .unwrap_or_else(|| panic!("missing header {name}"))
        .to_str()
        .unwrap()
        .to_string()
}

async fn body(res: Response<Body>) -> String {
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

#[test]
fn speculation_rules_constant_is_exact() {
    assert_eq!(pages::SPECULATION_RULES, RULES);
}

#[tokio::test]
async fn every_route_carries_the_security_headers() {
    let expected_csp = csp_for(&hash(pages::SPECULATION_RULES));
    let routes = [
        ("/nope", 404),
        ("/?d=foo", 400),
        ("/?d=example.com", 301),
        ("/static/app.css", 200),
        ("/static/islands.js", 200),
        ("/static/logo.svg", 200),
        ("/robots.txt", 200),
    ];
    for (path, status) in routes {
        let res = get_path(path).await;
        assert_eq!(res.status().as_u16(), status, "{path}: status");
        let csp = header(&res, "content-security-policy");
        assert_eq!(csp, expected_csp, "{path}: csp");
        let script_src = csp
            .split("; ")
            .find(|d| d.starts_with("script-src"))
            .unwrap();
        assert!(!script_src.contains("'unsafe-inline'"), "{path}");
        assert!(!script_src.contains("'unsafe-eval'"), "{path}");
        for (name, value) in [
            ("cross-origin-resource-policy", "same-origin"),
            ("cross-origin-opener-policy", "same-origin"),
            (
                "strict-transport-security",
                "max-age=31536000; includeSubDomains; preload",
            ),
            ("x-frame-options", "DENY"),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "strict-origin-when-cross-origin"),
            (
                "permissions-policy",
                "camera=(), microphone=(), geolocation=(), payment=()",
            ),
        ] {
            assert_eq!(header(&res, name), value, "{path}: {name}");
        }
    }
}

#[tokio::test]
async fn the_rendered_rules_hash_is_in_the_csp() {
    let res = get_path("/nope").await;
    let csp = header(&res, "content-security-policy");
    let html = body(res).await;
    let open = "<script type=\"speculationrules\">";
    let start = html.find(open).expect("speculationrules script") + open.len();
    let end = start + html[start..].find("</script>").expect("closing tag");
    let rules = &html[start..end];
    assert_eq!(rules, RULES);
    assert!(
        csp.contains(&format!("'sha256-{}'", hash(rules))),
        "csp lacks the hash of the rendered body: {csp}"
    );
}

/// Glob with `*` matching any sequence, `/` included.
fn glob(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((head, tail)) => text.strip_prefix(head).is_some_and(|rest| {
            (0..=rest.len()).any(|i| rest.is_char_boundary(i) && glob(tail, &rest[i..]))
        }),
    }
}

fn patterns(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                if k == "href_matches" {
                    out.push(x.as_str().unwrap().to_string());
                } else {
                    patterns(x, out);
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| patterns(x, out)),
        _ => {}
    }
}

#[test]
fn the_rules_prerender_only_guide_and_report_pages() {
    let v: serde_json::Value = serde_json::from_str(pages::SPECULATION_RULES).unwrap();
    let mut pats = Vec::new();
    patterns(&v, &mut pats);
    assert_eq!(pats.len(), 2);
    for path in [
        "/",
        "/example.com",
        "/example.com/run",
        "/example.com/tls",
        "/compare",
    ] {
        assert!(!pats.iter().any(|p| glob(p, path)), "{path} must not match");
    }
    for path in ["/guide/dnssec", "/r/abc12345"] {
        assert_eq!(pats.iter().filter(|p| glob(p, path)).count(), 1, "{path}");
    }
}
