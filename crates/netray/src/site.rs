use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, REFERRER_POLICY,
    STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use percent_encoding::percent_decode_str;

const POLICY_PATH: &str = "/.well-known/mta-sts.txt";

const CSP: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self' https://stats.uptimerobot.com; frame-ancestors 'none'; base-uri 'self'; object-src 'none'; form-action 'self' https://*.netray.info";

const SECURITY_HEADERS: [(HeaderName, &str); 8] = [
    (CONTENT_SECURITY_POLICY, CSP),
    (
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-origin",
    ),
    (
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    ),
    (
        STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains; preload",
    ),
    (X_FRAME_OPTIONS, "DENY"),
    (X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (REFERRER_POLICY, "strict-origin-when-cross-origin"),
    (
        HeaderName::from_static("permissions-policy"),
        "camera=(), microphone=(), geolocation=(), payment=()",
    ),
];

struct Site {
    root: PathBuf,
    not_found: Vec<u8>,
}

pub async fn run(bind: SocketAddr, root: PathBuf) -> anyhow::Result<()> {
    let not_found = tokio::fs::read(root.join("404.html")).await?;
    let root = tokio::fs::canonicalize(&root).await?;
    let state = Arc::new(Site { root, not_found });

    let app = Router::new()
        .fallback(get(serve))
        .layer(map_response(security_headers))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(netray_common::server::shutdown_signal())
        .await?;
    Ok(())
}

async fn security_headers(mut res: Response) -> Response {
    for (name, value) in SECURITY_HEADERS {
        res.headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    res
}

async fn serve(State(site): State<Arc<Site>>, headers: HeaderMap, uri: Uri) -> Response {
    let path = percent_decode_str(uri.path())
        .decode_utf8_lossy()
        .into_owned();
    let mta_sts_host = headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.to_ascii_lowercase().starts_with("mta-sts."));

    let policy = path == POLICY_PATH;
    if (mta_sts_host && !policy) || !is_servable(&path, policy) {
        return not_found(&site);
    }
    let Some(file) = resolve(&site.root, &path).await else {
        return not_found(&site);
    };
    match tokio::fs::read(&file).await {
        Ok(body) => (
            [
                (CONTENT_TYPE, content_type(&file)),
                (CACHE_CONTROL, cache_control(&file, policy).to_string()),
            ],
            body,
        )
            .into_response(),
        Err(_) => not_found(&site),
    }
}

fn is_servable(path: &str, policy: bool) -> bool {
    if policy {
        return true;
    }
    let trimmed = path.trim_matches('/');
    !trimmed.is_empty()
        && !path.contains('\0')
        && trimmed.split('/').all(|seg| !seg.starts_with('.'))
}

/// The error pages are only ever served as a 404 body (nginx `internal`).
const INTERNAL: [&str; 2] = ["404.html", "50x.html"];

/// nginx `try_files $uri $uri.html $uri/`: a path ending in `/` only names a
/// directory index, never a file.
async fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let rel = path.trim_matches('/');
    if INTERNAL.contains(&rel) {
        return None;
    }
    let candidates = if path.ends_with('/') {
        vec![root.join(rel).join("index.html")]
    } else {
        vec![
            root.join(rel),
            root.join(format!("{rel}.html")),
            root.join(rel).join("index.html"),
        ]
    };
    for candidate in candidates {
        let Ok(real) = tokio::fs::canonicalize(&candidate).await else {
            continue;
        };
        if real.starts_with(root) && real.is_file() {
            return Some(real);
        }
    }
    None
}

fn content_type(file: &Path) -> String {
    let mime = mime_guess::from_path(file).first_or_octet_stream();
    if mime.type_() == mime_guess::mime::TEXT {
        format!("{mime}; charset=utf-8")
    } else {
        mime.to_string()
    }
}

fn cache_control(file: &Path, policy: bool) -> &'static str {
    if policy {
        return "no-cache";
    }
    match file.extension().and_then(|e| e.to_str()) {
        Some("css" | "svg" | "ico" | "xml" | "txt") => "public, max-age=604800, immutable",
        _ => "public, max-age=3600",
    }
}

fn not_found(site: &Site) -> Response {
    (
        StatusCode::NOT_FOUND,
        [(CONTENT_TYPE, "text/html; charset=utf-8")],
        site.not_found.clone(),
    )
        .into_response()
}
