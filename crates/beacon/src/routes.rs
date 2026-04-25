use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::response::sse::KeepAlive;
use axum::response::{Html, IntoResponse, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::OwnedSemaphorePermit;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tracing::Instrument;
use utoipa::{OpenApi, ToSchema};

use crate::checks;
use crate::error::{ErrorResponse, MailError};
use crate::quality::types::{CheckResult, Grade, SubCheck, Verdict};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// SSE cancellation adapter
// ---------------------------------------------------------------------------

pin_project_lite::pin_project! {
    /// Wraps the SSE receiver stream so that dropping the response body (client
    /// disconnect) aborts the inspection task and releases the concurrency
    /// permit. Axum `Sse<S>` moves the stream into its body, so `PinnedDrop`
    /// fires exactly once when the client disconnects.
    struct AbortOnDropStream<S> {
        #[pin]
        inner: S,
        handle: JoinHandle<()>,
        _permit: OwnedSemaphorePermit,
    }

    impl<S> PinnedDrop for AbortOnDropStream<S> {
        fn drop(this: Pin<&mut Self>) {
            this.handle.abort();
            metrics::gauge!("beacon_sse_clients_active").decrement(1.0);
        }
    }
}

impl<S: Stream> Stream for AbortOnDropStream<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyResponse {
    pub status: &'static str,
}

// `MetaResponse` is now `netray_common::ecosystem::EcosystemMeta`.
// Breaking change: the legacy `service` key is renamed to `site_name`.

#[derive(Debug, Deserialize, ToSchema)]
pub struct InspectRequest {
    #[schema(example = "example.com")]
    pub domain: String,
    #[serde(default)]
    #[schema(example = json!(["google"]))]
    pub dkim_selectors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    #[serde(default)]
    pub selector: Vec<String>,
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(
        title = "beacon",
        description = "Email security inspector — DNS-only mail posture analysis"
    ),
    paths(
        health_handler,
        ready_handler,
        meta_handler,
        inspect_post_handler,
        inspect_get_handler,
    ),
    components(schemas(
        HealthResponse,
        ReadyResponse,
        netray_common::ecosystem::EcosystemMeta,
        netray_common::ecosystem::EcosystemUrls,
        netray_common::ecosystem::RateLimitSummary,
        InspectRequest,
        CheckResult,
        SubCheck,
        Verdict,
        Grade,
        ErrorResponse,
    ))
)]
pub struct ApiDoc;

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

pub fn health_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .with_state(state)
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/inspect", post(inspect_post_handler))
        .route("/inspect/{domain}", get(inspect_get_handler))
        .route("/api/meta", get(meta_handler))
        .route("/api-docs/openapi.json", get(openapi_handler))
        .route("/docs", get(docs_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Shared inspect logic
// ---------------------------------------------------------------------------

/// Run the per-request pre-flight that is identical across the POST and GET
/// inspect handlers: validate DKIM selectors, apply the per-IP rate limit, and
/// return the resolved client IP.
async fn validate_and_rate_limit(
    state: &AppState,
    selectors: &[String],
    headers: &axum::http::HeaderMap,
    peer: SocketAddr,
) -> Result<std::net::IpAddr, MailError> {
    if selectors.len() > state.config.dkim.max_user_selectors {
        return Err(MailError::TooManySelectors {
            max: state.config.dkim.max_user_selectors,
        });
    }

    for s in selectors {
        crate::input::validate_dkim_selector(s)?;
    }

    let client_ip = state.ip_extractor.extract(headers, peer);
    if let Err(e) = state.rate_limiter.check(client_ip) {
        metrics::counter!(
            "beacon_rate_limit_rejections_total",
            "scope" => "per_ip",
        )
        .increment(1);
        return Err(e);
    }

    Ok(client_ip)
}

#[tracing::instrument(
    skip(state),
    fields(client_ip = tracing::field::Empty, domain = tracing::field::Empty)
)]
async fn do_inspect(
    domain: String,
    selectors: Vec<String>,
    client_ip: std::net::IpAddr,
    state: AppState,
) -> Result<
    Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    MailError,
> {
    tracing::Span::current().record("domain", domain.as_str());
    tracing::Span::current().record("client_ip", client_ip.to_string().as_str());

    let permit = match state.inspect_semaphore.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            metrics::counter!(
                "beacon_rate_limit_rejections_total",
                "scope" => "concurrency",
            )
            .increment(1);
            return Err(MailError::TooManyConcurrent);
        }
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<crate::quality::SseEvent>(32);

    metrics::gauge!("beacon_sse_clients_active").increment(1.0);

    let span = tracing::Span::current();
    let handle = tokio::spawn(
        checks::run_all_checks(
            domain,
            selectors,
            state.config.clone(),
            state.dns_resolver.clone(),
            state.dnsbl_resolver.clone(),
            state.http_client.clone(),
            state.http_client_follow.clone(),
            state.enrichment_client.clone(),
            tx,
        )
        .instrument(span),
    );

    let stream = AbortOnDropStream {
        inner: ReceiverStream::new(rx),
        handle,
        _permit: permit,
    }
    .map(|event| {
        let sse_event: axum::response::sse::Event = event.into();
        Ok::<_, Infallible>(sse_event)
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keepalive"),
    ))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/health",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse),
    )
)]
async fn health_handler() -> impl IntoResponse {
    (
        [(axum::http::header::CACHE_CONTROL, "no-cache")],
        Json(HealthResponse { status: "ok" }),
    )
}

#[utoipa::path(
    get,
    path = "/ready",
    tag = "Probes",
    responses(
        (status = 200, description = "Service is ready", body = ReadyResponse),
        (status = 503, description = "Service not ready"),
    )
)]
async fn ready_handler(State(state): State<AppState>) -> impl IntoResponse {
    if state.dns_resolver.is_initialized() {
        (
            axum::http::StatusCode::OK,
            [(axum::http::header::CACHE_CONTROL, "no-cache")],
            Json(ReadyResponse { status: "ready" }),
        )
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            [(axum::http::header::CACHE_CONTROL, "no-cache")],
            Json(ReadyResponse {
                status: "not ready",
            }),
        )
    }
}

#[utoipa::path(
    get,
    path = "/api/meta",
    tag = "Meta",
    responses(
        (status = 200, description = "Service metadata", body = netray_common::ecosystem::EcosystemMeta),
    )
)]
async fn meta_handler(
    State(state): State<AppState>,
) -> Json<netray_common::ecosystem::EcosystemMeta> {
    use netray_common::ecosystem::{EcosystemMeta, EcosystemUrls, RateLimitSummary};
    use serde_json::{Map, Value};

    let cfg = &state.config;

    let mut features = Map::new();
    features.insert(
        "ip_enrichment".into(),
        Value::Bool(state.enrichment_client.is_some()),
    );
    features.insert(
        "dnsbl_zones_count".into(),
        Value::from(cfg.dnsbl.zones.len()),
    );

    let mut limits = Map::new();
    limits.insert(
        "max_concurrent_inspections".into(),
        Value::from(cfg.server.max_concurrent_inspections),
    );
    limits.insert(
        "max_user_dkim_selectors".into(),
        Value::from(cfg.dkim.max_user_selectors),
    );
    limits.insert("dns_timeout_ms".into(), Value::from(cfg.dns.timeout_ms));

    // Beacon's RateLimitConfig stores per_ip as a "<n>/<unit>" string. Parse
    // it into a per-minute rate; fall back to 0 on unrecognised formats.
    let (per_ip_per_minute, per_ip_burst) = parse_per_ip_rate(&cfg.rate_limit.per_ip);

    Json(EcosystemMeta {
        site_name: "beacon".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ecosystem: EcosystemUrls::from(&cfg.ecosystem),
        features,
        limits,
        rate_limit: RateLimitSummary {
            per_ip_per_minute,
            per_ip_burst,
            global_per_minute: 0,
            global_burst: 0,
        },
    })
}

/// Parse beacon's `"<n>/<unit>"` rate-limit string (e.g. `"10/min"`).
/// Returns `(per_minute, burst)`; burst is 0 since the string form has no
/// burst component.
fn parse_per_ip_rate(s: &str) -> (u32, u32) {
    let mut parts = s.split('/');
    let n: u32 = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
    let per_minute = match parts.next().map(str::trim) {
        Some("min") => n,
        Some("sec") => n.saturating_mul(60),
        Some("hour") => n / 60,
        _ => 0,
    };
    (per_minute, 0)
}

// ---------------------------------------------------------------------------
// SSE stream event schema
// ---------------------------------------------------------------------------
//
// The `/inspect` endpoints return a `text/event-stream` response. Each event's
// `data:` field contains a JSON object with a `type` discriminator. Three
// event types may appear on the stream:
//
// 1. `category` — one per completed check category. Payload is a
//    `CheckResult` with fields:
//      - `category`: the category identifier (e.g. "spf", "dmarc").
//      - `verdict`: the aggregated `Verdict` (`pass`, `info`, `warn`,
//        `fail`, or `skip`).
//      - `title`: short human-readable label.
//      - `detail`: one-line summary of the result.
//      - `sub_checks`: array of `{ name, verdict, detail }` entries.
//
// 2. `summary` — exactly one at the end of a successful stream. Payload:
//      - `grade`: overall `Grade` for the inspection
//        (`A` | `B` | `C` | `D` | `F` | `skipped`).
//      - `verdicts`: `HashMap<String, Verdict>` keyed by category.
//      - `duration_ms`: total inspection duration as `u64` milliseconds.
//
// 3. `error` — emitted when the inspection is aborted mid-stream. Payload:
//      - `code`: stable error code string (e.g. `"TOO_MANY_INSPECTIONS"`).
//      - `message`: human-readable detail.
//
// Clients should treat unknown `type` values as forward-compatible and skip
// them rather than erroring.
#[utoipa::path(
    post,
    path = "/inspect",
    tag = "Inspect",
    request_body = InspectRequest,
    responses(
        (status = 200, description = "SSE stream of check results (event types: category, summary, error)"),
        (status = 400, description = "Invalid domain, invalid DKIM selector, or TOO_MANY_SELECTORS", body = ErrorResponse),
        (status = 429, description = "Rate limited (RATE_LIMITED per-IP, or TOO_MANY_INSPECTIONS concurrency cap)", body = ErrorResponse),
    )
)]
#[axum::debug_handler]
async fn inspect_post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(body): Json<InspectRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    MailError,
> {
    let domain = crate::input::parse_domain(&body.domain)?;
    let client_ip = validate_and_rate_limit(&state, &body.dkim_selectors, &headers, peer).await?;

    // Record on the outer TraceLayer span (field is pre-declared in main.rs).
    tracing::Span::current().record("client_ip", client_ip.to_string().as_str());

    metrics::counter!("beacon_requests_total", "endpoint" => "inspect", "method" => "post")
        .increment(1);

    do_inspect(domain, body.dkim_selectors, client_ip, state).await
}

// See the `inspect_post_handler` doc block above for the SSE event-type
// schema (`category`, `summary`, `error`).
#[utoipa::path(
    get,
    path = "/inspect/{domain}",
    tag = "Inspect",
    params(
        ("domain" = String, Path, description = "Domain to inspect", example = "example.com"),
        ("selector" = Vec<String>, Query, description = "Optional DKIM selectors to probe (repeatable)"),
    ),
    responses(
        (status = 200, description = "SSE stream of check results (event types: category, summary, error)"),
        (status = 400, description = "Invalid domain, invalid DKIM selector, or TOO_MANY_SELECTORS", body = ErrorResponse),
        (status = 429, description = "Rate limited (RATE_LIMITED per-IP, or TOO_MANY_INSPECTIONS concurrency cap)", body = ErrorResponse),
    )
)]
async fn inspect_get_handler(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(raw_domain): Path<String>,
    Query(query): Query<InspectQuery>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    MailError,
> {
    let decoded = percent_encoding::percent_decode_str(&raw_domain)
        .decode_utf8_lossy()
        .to_string();
    let domain = crate::input::parse_domain(&decoded)?;
    let client_ip = validate_and_rate_limit(&state, &query.selector, &headers, peer).await?;

    // Record on the outer TraceLayer span (field is pre-declared in main.rs).
    tracing::Span::current().record("client_ip", client_ip.to_string().as_str());

    metrics::counter!("beacon_requests_total", "endpoint" => "inspect", "method" => "get")
        .increment(1);

    do_inspect(domain, query.selector, client_ip, state).await
}

async fn openapi_handler() -> impl IntoResponse {
    let mut doc = ApiDoc::openapi();
    doc.info.version = env!("CARGO_PKG_VERSION").to_string();
    Json(doc)
}

async fn docs_handler() -> Html<&'static str> {
    Html(include_str!("scalar_docs.html"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use tower::ServiceExt;

    async fn test_router() -> Router {
        test_router_with_config(crate::config::Config::load(None).unwrap()).await
    }

    /// SDD E9: build a router backed by `AppState::with_overrides` so
    /// `/health`, `/ready`, `/api/meta`, and the OpenAPI endpoint can be
    /// exercised without hitting live DNS or the network. The `DnsResolver`
    /// instances still go through `DnsResolver::new` (no trait injection
    /// landed this wave), but constructing them with the `"system"` resolver
    /// is cheap and does not issue a query unless a check function calls
    /// into it — which none of the tests on this router path do.
    async fn test_router_with_config(config: crate::config::Config) -> Router {
        let dns = crate::dns::DnsResolver::new(&["system".to_string()], 1000)
            .await
            .expect("build test DNS resolver");
        let dnsbl = crate::dns::DnsResolver::new(&["system".to_string()], 1000)
            .await
            .expect("build test DNSBL resolver");
        let state = AppState::with_overrides(config, dns, dnsbl);
        health_router(state.clone()).merge(api_router(state))
    }

    async fn do_get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(HttpRequest::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        (status, body)
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = test_router().await;
        let (status, body) = do_get(&app, "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn meta_returns_version() {
        let app = test_router().await;
        let (status, body) = do_get(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["version"].is_string());
        assert_eq!(body["site_name"], "beacon");
        // The legacy `service` key must not appear after the 0.3 migration.
        assert!(body.get("service").is_none());
    }

    #[tokio::test]
    async fn meta_includes_ecosystem_when_configured() {
        let mut config = crate::config::Config::load(None).unwrap();
        config.ecosystem = netray_common::ecosystem::EcosystemConfig {
            ip_base_url: Some("https://ip.example.com".to_string()),
            dns_base_url: Some("https://dns.example.com".to_string()),
            ..Default::default()
        };
        let app = test_router_with_config(config).await;
        let (status, body) = do_get(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["site_name"], "beacon");
        let eco = &body["ecosystem"];
        assert!(
            eco.is_object(),
            "ecosystem should be present (with empty strings when unset)"
        );
        assert_eq!(eco["ip_base_url"], "https://ip.example.com");
        assert_eq!(eco["dns_base_url"], "https://dns.example.com");
        // Unconfigured fields are now always present as empty strings
        // (uniform shape across services).
        assert_eq!(eco["tls_base_url"], "");
        assert_eq!(eco["http_base_url"], "");
        assert_eq!(eco["email_base_url"], "");
        assert_eq!(eco["lens_base_url"], "");
    }

    #[tokio::test]
    async fn meta_emits_uniform_shape_when_unconfigured() {
        let app = test_router().await;
        let (_, body) = do_get(&app, "/api/meta").await;
        // The shape is uniform across services: `ecosystem` is always
        // present, with empty strings for any sibling URL not configured.
        let eco = &body["ecosystem"];
        assert!(eco.is_object(), "ecosystem must be present even when unset");
        for key in [
            "ip_base_url",
            "dns_base_url",
            "tls_base_url",
            "http_base_url",
            "email_base_url",
            "lens_base_url",
        ] {
            assert_eq!(eco[key], "", "{key} must default to empty string");
        }
    }

    #[tokio::test]
    async fn openapi_returns_json() {
        let app = test_router().await;
        let (status, body) = do_get(&app, "/api-docs/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["openapi"], "3.1.0");
        assert_eq!(body["info"]["title"], "beacon");
    }
}
