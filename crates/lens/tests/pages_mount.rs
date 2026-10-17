/// `lens::frontend_router()` mounts the shared pages next to lens's SPA
/// (v2-pages Phase 3, requirement 33): pages routes carry the pages CSP, every
/// other route keeps the SPA shell and the shared security-headers layer's CSP.
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

/// What the shared security-headers layer emits for lens on a non-`/docs` path.
const LENS_CSP: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; object-src 'none'; form-action 'self'";

/// Marker of the embedded Vite shell (`frontend/dist/index.html`).
const SPA_MARKER: &str = "<div id=\"root\"";

async fn get(path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let resp = lens::frontend_router()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn csp(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("content-security-policy")
        .expect("Content-Security-Policy header")
        .to_str()
        .unwrap()
        .to_owned()
}

fn script_src(csp: &str) -> &str {
    csp.split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src "))
        .expect("script-src directive")
}

#[tokio::test]
async fn pages_route_gets_pages_csp() {
    let (status, headers, _) = get("/static/app.css").await;
    assert_eq!(status, StatusCode::OK);
    let csp = csp(&headers);
    let src = script_src(&csp);
    assert!(src.starts_with("script-src 'self' 'sha256-"), "got: {src}");
    assert!(!src.contains("'unsafe-inline'"), "got: {src}");
}

#[tokio::test]
async fn spa_routes_keep_shell_and_lens_csp() {
    for path in ["/", "/some-client-route", "/privacy", "/r/abc12345"] {
        let (status, headers, body) = get(path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(body.contains(SPA_MARKER), "{path}: not the SPA shell");
        assert_eq!(csp(&headers), LENS_CSP, "{path}");
    }
}

#[tokio::test]
async fn robots_is_lenss_own_with_lens_csp() {
    let (status, headers, body) = get("/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    assert_eq!(body, "User-agent: *\nAllow: /\n");
    assert_eq!(csp(&headers), LENS_CSP);
}
