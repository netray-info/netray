//! Host routing (spec v2-cutover, requirement 2; SDD §5.1 and §7.3): one router that picks what
//! answers by the request's Host. The probes answer first on any Host or none; the apex serves
//! the pages, `mta-sts.<apex>` only the policy, the inspector hosts their own routers (a GET or
//! HEAD beyond `/` and `/static/*` moves to the apex, where the pages' links point), and every
//! other name a 404.

pub mod apex;
pub mod layers;
mod legacy;
mod probes;

pub use apex::{Apex, build};
pub use layers::layers;
pub use probes::probes;

use std::convert::Infallible;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, HOST, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tower::ServiceExt;

const FAVICON: &[u8] = include_bytes!("../../static/favicon.svg");
const BIMI_LOGO: &[u8] = include_bytes!("../../static/bimi-logo.svg");
const MTA_STS: &[u8] = include_bytes!("../../static/mta-sts.txt");

/// What answers on each host. `None` for an inspector host answers 404.
pub struct HostRouters {
    /// The apex name, e.g. `netray.info`; compared ASCII-lowercase.
    pub apex: String,
    /// `/health` and `/ready`, answered before any host is looked at.
    pub probes: Router,
    /// Everything the apex serves beyond its root files and reserved paths.
    pub apex_router: Router,
    pub dns: Option<Router>,
    pub tls: Option<Router>,
    pub http: Option<Router>,
    pub ip: Option<Router>,
    /// `ip.`'s V1 command-line paths (`netray_web::api::ip_host`), tried before its `410` rows;
    /// `None` leaves them to the rows.
    pub ip_api: Option<crate::api::ApiState>,
}

struct Hosts {
    apex: String,
    probes: Router,
    apex_router: Router,
    apex_files: Router,
    mta_sts: Router,
    dns: Option<Router>,
    tls: Option<Router>,
    http: Option<Router>,
    ip: Option<Router>,
    ip_api: Option<crate::api::ApiState>,
}

fn file(content_type: &'static str, cache_control: &'static str, body: &'static [u8]) -> Response {
    (
        [
            (CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (CACHE_CONTROL, HeaderValue::from_static(cache_control)),
            (X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        ],
        body,
    )
        .into_response()
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

/// The routers for host dispatch: probes first, then the Host decides.
pub fn router(routers: HostRouters) -> Router {
    let hosts = Hosts {
        apex: routers.apex.to_ascii_lowercase(),
        probes: routers.probes,
        apex_router: routers.apex_router,
        apex_files: Router::new()
            .route(
                "/favicon.svg",
                get(|| async { file("image/svg+xml", "public, max-age=3600", FAVICON) }),
            )
            .route(
                "/bimi-logo.svg",
                get(|| async { file("image/svg+xml", "public, max-age=3600", BIMI_LOGO) }),
            ),
        mta_sts: Router::new()
            .route(
                "/.well-known/mta-sts.txt",
                get(|| async { file("text/plain; charset=utf-8", "no-cache", MTA_STS) }),
            )
            .fallback(|| async { not_found() }),
        dns: routers.dns,
        tls: routers.tls,
        http: routers.http,
        ip: routers.ip,
        ip_api: routers.ip_api,
    };
    Router::new().fallback(dispatch).with_state(Arc::new(hosts))
}

/// The Host header (else the URI authority) without its port, ASCII-lowercase.
fn host_of(req: &Request) -> Option<String> {
    let raw = match req.headers().get(HOST) {
        Some(value) => value.to_str().ok()?,
        None => req.uri().authority()?.as_str(),
    };
    let name = if raw.starts_with('[') {
        &raw[..raw.find(']')? + 1]
    } else {
        match raw.rsplit_once(':') {
            Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
            _ => raw,
        }
    };
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

async fn call(router: &Router, req: Request) -> Response {
    router
        .clone()
        .oneshot(req)
        .await
        .unwrap_or_else(|e: Infallible| match e {})
}

async fn inspector(router: &Option<Router>, req: Request<Body>) -> Response {
    match router {
        Some(router) => call(router, req).await,
        None => not_found(),
    }
}

async fn dispatch(State(hosts): State<Arc<Hosts>>, req: Request) -> Response {
    let path = req.uri().path();
    if path == "/health" || path == "/ready" {
        return call(&hosts.probes, req).await;
    }
    let Some(host) = host_of(&req) else {
        return not_found();
    };
    let legacy = legacy::Legacy {
        apex: &hosts.apex,
        method: req.method(),
        uri: req.uri(),
    };
    if host == hosts.apex {
        if path == "/"
            && matches!(*req.method(), Method::GET | Method::HEAD)
            && let Some(response) = crate::pages::query_redirect_absolute(&hosts.apex, req.uri())
        {
            return response;
        }
        if let Some(response) = legacy.apex_legacy() {
            return response;
        }
        return match path {
            "/favicon.svg" | "/bimi-logo.svg" => call(&hosts.apex_files, req).await,
            "/favicon.ico" | "/r" | "/r/" | "/inspect" | "/inspect/" | "/metrics" => {
                crate::pages::not_found().await
            }
            _ if path == "/.well-known" || path.starts_with("/.well-known/") => {
                crate::pages::not_found().await
            }
            _ => call(&hosts.apex_router, req).await,
        };
    }
    let label = host
        .strip_suffix(hosts.apex.as_str())
        .and_then(|rest| rest.strip_suffix('.'));
    match label {
        Some("lens") => legacy.lens_host(),
        Some("email") => legacy.email_host(),
        Some("dns") => match legacy.subdomain_common().or_else(|| legacy.to_apex()) {
            Some(response) => response,
            None => inspector(&hosts.dns, req).await,
        },
        Some("tls") => match legacy.subdomain_common().or_else(|| legacy.to_apex()) {
            Some(response) => response,
            None => inspector(&hosts.tls, req).await,
        },
        Some("http") => match legacy.subdomain_common().or_else(|| legacy.to_apex()) {
            Some(response) => response,
            None => inspector(&hosts.http, req).await,
        },
        Some("ip") => {
            let req = match &hosts.ip_api {
                Some(state) => match crate::api::ip_host(state, req).await {
                    Ok(response) => return response,
                    Err(req) => req,
                },
                None => req,
            };
            let legacy = legacy::Legacy {
                apex: &hosts.apex,
                method: req.method(),
                uri: req.uri(),
            };
            match legacy
                .ip_successor()
                .or_else(|| legacy.subdomain_common())
                .or_else(|| legacy.to_apex())
            {
                Some(response) => response,
                None => inspector(&hosts.ip, req).await,
            }
        }
        Some("mta-sts") => call(&hosts.mta_sts, req).await,
        _ => not_found(),
    }
}
