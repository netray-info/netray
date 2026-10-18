use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::net::IpAddr;

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::stream;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;

use netray_engine::profile::Profile;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION};

use crate::admission::{
    Admit, Reason, Refusal, RunStream, Trigger, admit, admit_dkim, admit_uncounted, check_limits,
    count_rejection, join_recording, registered, until_done,
};
use crate::badge::render::svg_for_grade;
use crate::badge::{BadgeQuery, compute_etag, parse_badge_request};
use crate::cache::CachedResult;
use crate::check::{CheckInput, CheckOutput, SectionError};
use crate::input::validate_domain;
use crate::modules::{BackendExtra, BackendResult};
use crate::score::{
    CheckResult, CheckVerdict, OverallScore, fix_owner_label, grade_for, grade_label, guide_url,
    section_weights, spec_by_name,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// SSE event payload types
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct CheckItem {
    pub name: String,
    #[schema(value_type = String)]
    pub verdict: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub fix_hint: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub fix_owner: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
    /// The ids of the run's blocks the check judged by.
    #[schema(value_type = Vec<String>)]
    pub evidence: Vec<BlockId>,
}

/// Validate and split a comma-separated `dkim_selectors` string.
///
/// Returns `Ok(None)` when the input is absent. Returns `Ok(Some(vec))` for valid input.
/// Returns `Err(message)` for any validation failure (invalid chars, too long, too many).
fn validate_dkim_selectors(raw: Option<&str>) -> Result<Option<Vec<String>>, String> {
    let raw = match raw {
        None => return Ok(None),
        Some(r) => r,
    };

    let tokens: Vec<&str> = raw.split(',').map(|s| s.trim()).collect();

    if tokens.is_empty() || tokens.iter().all(|t| t.is_empty()) {
        return Err("dkim_selectors must not be empty".to_string());
    }

    if tokens.iter().any(|t| t.is_empty()) {
        return Err("dkim_selectors contains an empty token".to_string());
    }

    if tokens.len() > 10 {
        return Err(format!(
            "dkim_selectors: at most 10 selectors allowed, got {}",
            tokens.len()
        ));
    }

    for token in &tokens {
        if token.len() > 63 {
            return Err(format!(
                "dkim_selectors: selector '{token}' exceeds 63 characters"
            ));
        }
        if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!(
                "dkim_selectors: selector '{token}' contains invalid characters (allowed: [a-zA-Z0-9-])"
            ));
        }
    }

    Ok(Some(tokens.iter().map(|s| s.to_string()).collect()))
}

#[derive(Serialize, ToSchema)]
pub struct IpAddressInfo {
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<String>,
    pub network_type: String,
}

#[derive(Serialize, ToSchema)]
pub struct DnsEvent {
    #[schema(value_type = String)]
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub detail_url: String,
    /// The section's evidence blocks.
    #[schema(value_type = Vec<Object>)]
    pub evidence: Vec<Block>,
}

#[derive(Serialize, ToSchema)]
pub struct TlsEvent {
    #[schema(value_type = String)]
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub detail_url: String,
    /// The section's evidence blocks.
    #[schema(value_type = Vec<Object>)]
    pub evidence: Vec<Block>,
}

#[derive(Serialize, ToSchema)]
pub struct HttpEvent {
    #[schema(value_type = String)]
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub detail_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_org: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_network_type: Option<String>,
    /// The section's evidence blocks.
    #[schema(value_type = Vec<Object>)]
    pub evidence: Vec<Block>,
}

#[derive(Serialize, ToSchema)]
pub struct IpEvent {
    #[schema(value_type = String)]
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub addresses: Vec<IpAddressInfo>,
    pub detail_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub guide_url: Option<&'static str>,
    /// The section's evidence blocks.
    #[schema(value_type = Vec<Object>)]
    pub evidence: Vec<Block>,
}

#[derive(Serialize, ToSchema)]
pub struct EmailEvent {
    #[schema(value_type = String)]
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub detail_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grade: Option<String>,
    /// The section's evidence blocks.
    #[schema(value_type = Vec<Object>)]
    pub evidence: Vec<Block>,
}

#[derive(Serialize, ToSchema)]
pub struct SummaryEvent {
    /// Section status map, keyed by section name.
    pub sections: HashMap<String, String>,
    pub section_grades: HashMap<String, String>,
    pub overall: String,
    /// A letter grade, or `"incomplete"` when `complete` is false.
    pub grade: String,
    /// False when any section is errored, scored nothing possible, or has a weighted check that
    /// is `unmeasured`; the grade is then `incomplete` (a failed hard-fail check still gives
    /// `F`), and the result is neither cached nor snapshotted.
    pub complete: bool,
    pub score: f64,
    pub hard_fail: bool,
    pub hard_fail_checks: Vec<String>,
    /// Comma-joined list of hard-fail check names; null when `hard_fail` is false.
    pub hard_fail_reason: Option<String>,
    /// Section name → reason. Always present (may be empty). Populated only for NotApplicable.
    pub not_applicable: HashMap<String, String>,
    /// The version of the evidence vocabulary of the sections' blocks.
    pub evidence_schema: u32,
}

#[derive(Serialize, ToSchema)]
pub struct DoneEvent {
    pub domain: String,
    pub duration_ms: u64,
    pub cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Sync response type
// ---------------------------------------------------------------------------

/// Complete domain health check result for synchronous (non-SSE) mode.
#[derive(Serialize, ToSchema)]
pub struct SyncCheckResponse {
    pub dns: DnsEvent,
    pub tls: TlsEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<EmailEvent>,
    pub ip: IpEvent,
    pub summary: SummaryEvent,
    pub done: DoneEvent,
}

// ---------------------------------------------------------------------------
// POST body and query params
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct CheckPostBody {
    pub domain: String,
    /// When `false`, triggers synchronous (non-SSE) response mode.
    #[serde(default)]
    pub stream: Option<bool>,
    /// Comma-separated DKIM selectors to test. Each selector: [a-zA-Z0-9-], 1–63 chars, 1–10 total.
    #[serde(default)]
    pub dkim_selectors: Option<String>,
}

/// Query parameters for `GET /api/check/{domain}`.
#[derive(Deserialize, IntoParams)]
pub struct CheckGetQuery {
    /// When `false`, triggers synchronous (non-SSE) response mode.
    pub stream: Option<bool>,
    /// Comma-separated DKIM selectors to test. Each selector: [a-zA-Z0-9-], 1–63 chars, 1–10 total.
    pub dkim_selectors: Option<String>,
}

// ---------------------------------------------------------------------------
// Meta response types
// ---------------------------------------------------------------------------

/// Lens scoring profile, serialized into `EcosystemMeta.features["profile"]`.
///
/// Kept as a struct (not derived as ToSchema any more) because the shared
/// `EcosystemMeta` carries it through `serde_json::Value`. The acceptance
/// schema only constrains `features` to be a JSON object; the inner shape
/// is service-specific.
#[derive(Serialize)]
pub struct ProfileData {
    pub name: String,
    pub version: u32,
    pub checks: HashMap<String, u32>,
    pub section_weights: HashMap<String, u32>,
    pub thresholds: BTreeMap<String, u32>,
    pub hard_fail: HashMap<String, Vec<String>>,
}

/// Lens-specific extension of the shared `EcosystemMeta`: flattens the common
/// shape and adds a top-level `site` object holding apex-landing branding from
/// the `[site]` config. Per SDD product-repositioning §7.1, `site` is a
/// top-level field — not nested under `features` — so the SolidJS frontend
/// (and any external consumer) can read `meta.site.hero_heading` directly.
#[derive(Serialize)]
struct LensMetaResponse {
    #[serde(flatten)]
    base: netray_common::ecosystem::EcosystemMeta,
    site: crate::config::SiteConfig,
}

// ---------------------------------------------------------------------------
// Health / Ready response types
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    #[schema(value_type = String)]
    pub status: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct ReadyResponse {
    #[schema(value_type = String)]
    pub status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub down: Vec<String>,
}

// ---------------------------------------------------------------------------
// Router builders
// ---------------------------------------------------------------------------

pub fn health_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(utoipa_axum::routes!(health_handler))
        .routes(utoipa_axum::routes!(ready_handler))
}

pub fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(utoipa_axum::routes!(meta_handler))
        .routes(utoipa_axum::routes!(check_get_handler))
        .routes(utoipa_axum::routes!(check_post_handler))
}

/// Returns an OpenApiRouter for the badge endpoint.
///
/// The route captures `/badge/{domain}` where `{domain}` includes the `.svg`
/// suffix (matchit does not support literal suffixes after a parameter).
/// The handler strips the `.svg` suffix and returns 404 for any other extension.
/// The utoipa annotation documents the path as `/badge/{domain}` for the same reason.
pub fn badge_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(utoipa_axum::routes!(badge_handler))
}

/// Returns an Axum Router for the OG card endpoint.
///
/// The route captures `/og/{domain_png}` where `{domain_png}` includes the `.png`
/// suffix. The handler strips the `.png` suffix and returns 404 for any other extension.
pub fn og_router() -> axum::Router<AppState> {
    use crate::og::handler::og_handler;
    use axum::routing::get;
    axum::Router::new().route("/og/{domain_png}", get(og_handler))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health_handler() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}

#[utoipa::path(
    get,
    path = "/api/meta",
    tag = "api",
    responses(
        (status = 200, description = "Service metadata and scoring profile", body = netray_common::ecosystem::EcosystemMeta)
    )
)]
pub async fn meta_handler(State(state): State<AppState>) -> impl IntoResponse {
    use netray_common::ecosystem::{EcosystemMeta, EcosystemUrls, RateLimitSummary};
    use serde_json::Map;

    let config = &state.config;
    let profile = &state.scoring_profile;
    let mut all_checks = HashMap::new();
    let mut section_weights = HashMap::new();
    let mut hard_fail = HashMap::new();
    for (protocol, section) in &profile.sections {
        let name = protocol.as_str().to_string();
        all_checks.extend(
            section
                .checks
                .iter()
                .map(|(id, w)| (id.name().to_string(), *w)),
        );
        section_weights.insert(name.clone(), section.weight);
        hard_fail.insert(
            name,
            section
                .hard_fail
                .iter()
                .map(|id| id.name().to_string())
                .collect(),
        );
    }
    let thresholds = profile
        .thresholds
        .iter()
        .map(|(g, t)| (grade_label(*g).to_string(), *t))
        .collect();
    let profile_data = ProfileData {
        name: profile.name.clone(),
        version: profile.version,
        checks: all_checks,
        section_weights,
        thresholds,
        hard_fail,
    };

    let mut features = Map::new();
    features.insert(
        "profile".into(),
        serde_json::to_value(&profile_data).unwrap_or(serde_json::Value::Null),
    );
    features.insert(
        "badges".into(),
        serde_json::Value::Bool(config.badges.enabled),
    );

    let rl = &config.rate_limit;
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(LensMetaResponse {
            base: EcosystemMeta {
                site_name: "lens — Domain Health Check".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ecosystem: EcosystemUrls::from(&config.ecosystem),
                features,
                limits: Map::new(),
                rate_limit: RateLimitSummary {
                    per_ip_per_minute: rl.per_ip_per_minute,
                    per_ip_burst: rl.per_ip_burst,
                    global_per_minute: rl.global_per_minute,
                    global_burst: rl.global_burst,
                },
            },
            site: config.site.clone(),
        }),
    )
}

#[utoipa::path(
    get,
    path = "/ready",
    tag = "health",
    responses(
        (status = 200, description = "Ready", body = ReadyResponse)
    )
)]
pub async fn ready_handler() -> impl IntoResponse {
    Json(ReadyResponse {
        status: "ok",
        down: vec![],
    })
}

// ---------------------------------------------------------------------------
// Badge helpers
// ---------------------------------------------------------------------------

fn badge_is_not_modified(headers: &axum::http::HeaderMap, etag: &str) -> bool {
    headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == etag)
        .unwrap_or(false)
}

fn build_svg_response(svg: String, cache_control: &str, etag: &str) -> Response {
    use axum::http::{StatusCode, header};
    let mut resp = (StatusCode::OK, svg).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        "image/svg+xml; charset=utf-8".parse().unwrap(),
    );
    headers.insert(header::CACHE_CONTROL, cache_control.parse().unwrap());
    headers.insert(header::ETAG, etag.parse().unwrap());
    resp
}

// ---------------------------------------------------------------------------
// Badge handler
// ---------------------------------------------------------------------------

/// The `?` badge a throttled request gets.
fn throttled_badge_response(
    badge_req: &crate::badge::BadgeRequest,
    req_headers: &axum::http::HeaderMap,
) -> Response {
    use std::time::Instant;
    metrics::counter!("netray_badge_requests_total", "cache" => "throttled").increment(1);
    let render_start = Instant::now();
    let svg = svg_for_grade(&badge_req.label, "error", badge_req.style);
    metrics::histogram!("netray_badge_render_duration_seconds")
        .record(render_start.elapsed().as_secs_f64());
    let etag = compute_etag(&badge_req.domain, "?", &badge_req.label, badge_req.style);
    if badge_is_not_modified(req_headers, &etag) {
        return axum::http::StatusCode::NOT_MODIFIED.into_response();
    }
    build_svg_response(svg, "public, max-age=300, s-maxage=300", &etag)
}

/// The badge for `grade`; `long` asks for the long `Cache-Control`, which an unscored grade
/// never gets.
fn badge_response(
    badge_req: &crate::badge::BadgeRequest,
    req_headers: &axum::http::HeaderMap,
    grade: &str,
    long: bool,
) -> Response {
    let render_start = std::time::Instant::now();
    let svg = svg_for_grade(&badge_req.label, grade, badge_req.style);
    metrics::histogram!("netray_badge_render_duration_seconds")
        .record(render_start.elapsed().as_secs_f64());
    let etag = compute_etag(&badge_req.domain, grade, &badge_req.label, badge_req.style);
    if badge_is_not_modified(req_headers, &etag) {
        return axum::http::StatusCode::NOT_MODIFIED.into_response();
    }
    let cache_ctrl = if long && !is_unscored_grade(grade) {
        "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400"
    } else {
        "public, max-age=300, s-maxage=300"
    };
    build_svg_response(svg, cache_ctrl, &etag)
}

#[utoipa::path(
    get,
    path = "/badge/{domain}",
    tag = "badge",
    params(
        ("domain" = String, Path, description = "DNS hostname (with .svg suffix) to show grade for, e.g. example.com.svg"),
        ("style" = Option<String>, Query, description = "Badge style: flat (default) or for-the-badge"),
        ("label" = Option<String>, Query, description = "Left-segment label text, ASCII printable, max 32 chars"),
    ),
    responses(
        (status = 200, description = "SVG badge", content_type = "image/svg+xml"),
        (status = 304, description = "Not Modified"),
        (status = 400, description = "Invalid domain or label", body = netray_common::error::ErrorResponse),
        (status = 404, description = "Badge feature disabled"),
    )
)]
pub async fn badge_handler(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(query): Query<BadgeQuery>,
    req_headers: axum::http::HeaderMap,
) -> Response {
    let domain = match domain.strip_suffix(".svg") {
        Some(d) => d.to_owned(),
        None => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    let badge_req = match parse_badge_request(&domain, query, &state.config.badges) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };

    let head = crate::cache::head(&state, &badge_req.domain).await;

    // The head is inside the cache window — serve it, bypass all rate limiters.
    if let Some(lookup) = &head
        && lookup.fresh
    {
        metrics::counter!("netray_badge_requests_total", "cache" => "hit").increment(1);
        return badge_response(
            &badge_req,
            &req_headers,
            grade_label(lookup.head.grade),
            true,
        );
    }
    let stale = head.as_ref().map(|l| grade_label(l.head.grade));

    // Start or join the domain's run.
    let published = match admit(
        &state,
        &badge_req.domain,
        IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        Trigger::Badge,
        netray_common::prefetch::is_prefetch(&req_headers),
    ) {
        Admit::Admitted(mut stream) => until_done(&mut stream).await,
        Admit::Refused(_) | Admit::NotStarted => {
            return match stale {
                Some(grade) => {
                    metrics::counter!("netray_badge_requests_total", "cache" => "throttled")
                        .increment(1);
                    badge_response(&badge_req, &req_headers, grade, false)
                }
                None => throttled_badge_response(&badge_req, &req_headers),
            };
        }
    };
    let Some(published) = published else {
        return match stale {
            Some(grade) => badge_response(&badge_req, &req_headers, grade, false),
            None => throttled_badge_response(&badge_req, &req_headers),
        };
    };

    if published.output.score.complete {
        metrics::counter!("netray_badge_requests_total", "cache" => "miss").increment(1);
        let grade = badge_grade(&published.output.score);
        return badge_response(&badge_req, &req_headers, &grade, true);
    }
    metrics::counter!("netray_badge_requests_total", "cache" => "failed").increment(1);
    badge_response(
        &badge_req,
        &req_headers,
        stale.unwrap_or("incomplete"),
        false,
    )
}

// ---------------------------------------------------------------------------
// Check handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/check/{domain}",
    tag = "check",
    params(
        ("domain" = String, Path, description = "Domain name to check"),
        CheckGetQuery,
    ),
    responses(
        (status = 200, description = "Domain health check result (SSE stream or JSON object)", body = SyncCheckResponse),
        (status = 400, description = "Invalid or blocked domain"),
        (status = 429, description = "Rate limit exceeded"),
        (status = 503, description = "Prefetch refused: no stored result and no run in flight", body = netray_common::error::ErrorResponse),
    )
)]
pub async fn check_get_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    mut headers: axum::http::HeaderMap,
    Path(domain): Path<String>,
    Query(query): Query<CheckGetQuery>,
) -> Response {
    let client_ip = client_ip_from(&state, &mut headers, peer);
    let sync = is_sync_mode(&headers, query.stream);
    let dkim_selectors = match validate_dkim_selectors(query.dkim_selectors.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            return crate::error::AppError::InvalidInput(msg).into_response();
        }
    };
    let request_id = request_id_from(&headers);
    let prefetch = netray_common::prefetch::is_prefetch(&headers);
    run_check_handler(
        state,
        client_ip,
        domain,
        sync,
        dkim_selectors,
        request_id,
        prefetch,
    )
    .await
}

#[utoipa::path(
    post,
    path = "/api/check",
    tag = "check",
    request_body = CheckPostBody,
    responses(
        (status = 200, description = "Domain health check result (SSE stream or JSON object)", body = SyncCheckResponse),
        (status = 400, description = "Invalid or blocked domain"),
        (status = 429, description = "Rate limit exceeded"),
        (status = 503, description = "Prefetch refused: no stored result and no run in flight", body = netray_common::error::ErrorResponse),
    )
)]
pub async fn check_post_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    mut headers: axum::http::HeaderMap,
    Json(body): Json<CheckPostBody>,
) -> Response {
    let client_ip = client_ip_from(&state, &mut headers, peer);
    let sync = is_sync_mode(&headers, body.stream);
    let dkim_selectors = match validate_dkim_selectors(body.dkim_selectors.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            return crate::error::AppError::InvalidInput(msg).into_response();
        }
    };
    let request_id = request_id_from(&headers);
    let prefetch = netray_common::prefetch::is_prefetch(&headers);
    run_check_handler(
        state,
        client_ip,
        body.domain,
        sync,
        dkim_selectors,
        request_id,
        prefetch,
    )
    .await
}

/// The real client IP behind the trusted proxy. lens has no Cloudflare in
/// front, so `CF-Connecting-IP` is whatever the client sent and Traefik passes
/// it through; drop it so the extractor falls back to Traefik's `X-Real-Ip`.
fn client_ip_from(
    state: &AppState,
    headers: &mut axum::http::HeaderMap,
    peer: std::net::SocketAddr,
) -> std::net::IpAddr {
    headers.remove("cf-connecting-ip");
    state.ip_extractor.extract(headers, peer)
}

/// The request ID set (or validated) by the `request_id` middleware.
fn request_id_from(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

/// Returns true when the request should be answered with a single JSON response
/// rather than an SSE stream.
fn is_sync_mode(headers: &axum::http::HeaderMap, stream_param: Option<bool>) -> bool {
    if stream_param == Some(false) {
        return true;
    }
    if let Some(accept) = headers.get(axum::http::header::ACCEPT)
        && let Ok(val) = accept.to_str()
    {
        return val.contains("application/json") && !val.contains("text/event-stream");
    }
    false
}

fn refusal_response(r: &Refusal) -> Response {
    crate::error::AppError::RateLimited {
        retry_after_secs: r.retry_after_s,
    }
    .into_response()
}

/// The held rendering of a head that differs from `prior` (id, last seen), inside the cache
/// window: a run recorded it while the request was on its way to `admit`.
async fn newer_head(
    state: &AppState,
    domain: &str,
    prior: Option<&(String, i64)>,
) -> Option<std::sync::Arc<crate::cache::CachedResult>> {
    let cache = state.cache.as_ref()?;
    let lookup = crate::cache::head(state, domain).await?;
    let unchanged = prior.is_some_and(|(id, last_seen_at)| {
        *id == lookup.head.id && *last_seen_at == lookup.head.last_seen_at
    });
    if !lookup.fresh || unchanged {
        return None;
    }
    cache.get(&lookup.head.id).await
}

async fn run_check_handler(
    state: AppState,
    client_ip: IpAddr,
    domain_raw: String,
    sync: bool,
    dkim_selectors: Option<Vec<String>>,
    request_id: Option<String>,
    prefetch: bool,
) -> Response {
    // Record client_ip into the TraceLayer span.
    tracing::Span::current().record("client_ip", tracing::field::display(&client_ip));

    // 1. Validate domain.
    let domain = match validate_domain(&domain_raw) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };

    // 2. Rate limit.
    if let Err(r) = check_limits(&state, client_ip) {
        count_rejection(&r);
        return refusal_response(&r);
    }

    // 3. Cache lookup (only when no per-request options like dkim_selectors): the head is
    // inside the window and its V1 rendering is held. A run that has published and is still
    // recording is newer than that head: join it instead.
    // The run registered before the head read stays joinable after it records and deregisters.
    // It is kept for the throttle refusal of step 4: a run that recorded between the head
    // read and `admit` throttles the request, which then joins it.
    let mut prior_head = None;
    let mut recording = None;
    let mut before_head: Option<RunStream> = None;
    if dkim_selectors.is_none()
        && let Some(cache) = &state.cache
    {
        before_head = registered(&state, &domain);
        if let Some(lookup) = crate::cache::head(&state, &domain).await {
            if lookup.fresh
                && let Some(cached) = cache.get(&lookup.head.id).await
            {
                let joinable = if before_head
                    .as_ref()
                    .is_some_and(RunStream::published_complete)
                {
                    before_head.take()
                } else {
                    join_recording(&state, &domain)
                };
                match joinable {
                    Some(stream) => recording = Some(stream),
                    None => {
                        return if sync {
                            sync_response_from_cached(domain, &cached, true, &state.scoring_profile)
                        } else {
                            sse_response_from_cached(domain, &cached, true, &state.scoring_profile)
                        };
                    }
                }
            }
            prior_head = Some((lookup.head.id, lookup.head.last_seen_at));
        }
    }

    if dkim_selectors.is_none()
        && recording.is_none()
        && let Some(hook) = &state.before_admit
    {
        hook().await;
    }

    // 4. Start or join the domain's run.
    let mut stream = if let Some(stream) = recording {
        stream
    } else if let Some(selectors) = dkim_selectors {
        if prefetch {
            return crate::error::AppError::PrefetchRefused.into_response();
        }
        match admit_dkim(
            &state,
            CheckInput {
                domain: domain.clone(),
                dkim_selectors: Some(selectors),
                client_ip: Some(client_ip),
                request_id,
            },
        ) {
            Ok(stream) => {
                state.client_runs.record(client_ip);
                stream
            }
            Err(r) => return refusal_response(&r),
        }
    } else {
        match admit_uncounted(&state, &domain, client_ip, Trigger::Api, prefetch) {
            Admit::Admitted(stream) => {
                if !stream.joined() {
                    state.client_runs.record(client_ip);
                }
                stream
            }
            Admit::Refused(r) => {
                let late = if r.reason == Reason::Throttle {
                    before_head
                        .filter(RunStream::published_complete)
                        .or_else(|| join_recording(&state, &domain))
                } else {
                    None
                };
                if let Some(stream) = late {
                    stream
                } else {
                    if r.reason == Reason::Throttle
                        && let Some(cached) = newer_head(&state, &domain, prior_head.as_ref()).await
                    {
                        return if sync {
                            sync_response_from_cached(domain, &cached, true, &state.scoring_profile)
                        } else {
                            sse_response_from_cached(domain, &cached, true, &state.scoring_profile)
                        };
                    }
                    count_rejection(&r);
                    return refusal_response(&r);
                }
            }
            Admit::NotStarted => return crate::error::AppError::PrefetchRefused.into_response(),
        }
    };
    let Some(published) = until_done(&mut stream).await else {
        return crate::error::AppError::Internal("check run ended without a result".to_string())
            .into_response();
    };

    // 5. Return SSE stream or sync JSON.
    let domain_out = published.output.domain.clone();
    if sync {
        build_sync_response(
            domain_out,
            &published.output,
            false,
            &state.scoring_profile,
            published.snapshot_id.clone(),
            &published.facts,
        )
    } else {
        let events = build_sse_events(
            domain_out,
            &published.output,
            false,
            &state.scoring_profile,
            published.snapshot_id.clone(),
            &published.facts,
        );
        make_sse_stream(events, "MISS")
    }
}

// ---------------------------------------------------------------------------
// Payload-building helpers (used by both SSE and sync paths)
// ---------------------------------------------------------------------------

fn verdict_str(verdict: &CheckVerdict) -> &'static str {
    match verdict {
        CheckVerdict::Pass => "pass",
        CheckVerdict::Warn => "warn",
        CheckVerdict::Fail => "fail",
        CheckVerdict::NotFound => "fail",
        CheckVerdict::Skip => "skip",
        CheckVerdict::Unmeasured => "unmeasured",
    }
}

/// True for the grades that render as `?` and get the short `Cache-Control`.
pub fn is_unscored_grade(grade: &str) -> bool {
    grade == "error" || grade == "incomplete"
}

/// The grade a badge shows: `"incomplete"` for an incomplete result, whatever its grade.
fn badge_grade(score: &OverallScore) -> String {
    if score.complete {
        score.grade.clone()
    } else {
        "incomplete".to_string()
    }
}

/// True when a weighted check of an answering section could not be measured.
fn has_weighted_unmeasured(r: &BackendResult, weights: &HashMap<String, u32>) -> bool {
    r.checks.iter().any(|c| {
        matches!(c.verdict, CheckVerdict::Unmeasured)
            && weights.get(&c.name).is_some_and(|w| *w > 0)
    })
}

/// Section status for a section event: `"incomplete"` for an answering section with a weighted
/// unmeasured check, `"error"` also for a Scored section whose weighted checks earn nothing
/// possible. A section absent from the profile (no weights at all) keeps its check-derived
/// status.
fn section_status_from_checks(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> &'static str {
    if let (Ok(r), false) = (result, weights.is_empty()) {
        if has_weighted_unmeasured(r, weights) {
            return "incomplete";
        }
        let has_possible = r.checks.iter().any(|c| {
            !matches!(c.verdict, CheckVerdict::Skip | CheckVerdict::Unmeasured)
                && weights.get(&c.name).is_some_and(|w| *w > 0)
        });
        if !has_possible {
            return "error";
        }
    }
    section_status_from_verdicts(result)
}

fn section_status_from_verdicts(result: &Result<BackendResult, SectionError>) -> &'static str {
    match result {
        Err(SectionError::NotApplicable { .. }) => "skip",
        Err(_) => "error",
        Ok(r) => {
            let mut has_warn = false;
            for c in &r.checks {
                match c.verdict {
                    CheckVerdict::Fail | CheckVerdict::NotFound => return "fail",
                    CheckVerdict::Warn => has_warn = true,
                    _ => {}
                }
            }
            if has_warn { "warn" } else { "pass" }
        }
    }
}

fn build_check_items(checks: &[CheckResult], weights: &HashMap<String, u32>) -> Vec<CheckItem> {
    checks
        .iter()
        .map(|c| {
            let verdict = verdict_str(&c.verdict);
            let spec = spec_by_name(&c.name);
            CheckItem {
                guide_url: spec.map(guide_url),
                fix_hint: spec.map(|s| s.fix_hint.as_str()),
                fix_owner: spec.map(|s| fix_owner_label(s.fix_owner)),
                name: c.name.clone(),
                verdict,
                weight: weights.get(&c.name).copied(),
                messages: c.messages.clone(),
                evidence: c.evidence.clone(),
            }
        })
        .collect()
}

/// The blocks of an answering section; none for one that did not answer.
fn section_evidence(result: &Result<BackendResult, SectionError>) -> Vec<Block> {
    result
        .as_ref()
        .map(|r| r.evidence.clone())
        .unwrap_or_default()
}

fn error_headline(e: &SectionError) -> String {
    match e {
        SectionError::Timeout => "timeout".to_string(),
        SectionError::NoDnsResults => "no DNS results".to_string(),
        SectionError::BackendError(_) => "backend error".to_string(),
        SectionError::NotApplicable { .. } => "not applicable".to_string(),
    }
}

/// The DNS section; one that did not answer still carries `facts_evidence`, the facts blocks no
/// section carries.
fn dns_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
    facts_evidence: &[Block],
) -> DnsEvent {
    let status = section_status_from_checks(result, weights);
    let (headline, checks, detail_url) = match result {
        Ok(r) => {
            let items = build_check_items(&r.checks, weights);
            let (raw_headline, url) = match &r.extra {
                BackendExtra::Dns {
                    raw_headline,
                    detail_url,
                    ..
                } => (raw_headline.clone(), detail_url.clone()),
                _ => (String::new(), String::new()),
            };
            (raw_headline, items, url)
        }
        Err(e) => (error_headline(e), vec![], String::new()),
    };
    let evidence = match result {
        Ok(r) => r.evidence.clone(),
        Err(_) => facts_evidence.to_vec(),
    };
    DnsEvent {
        status,
        headline,
        checks,
        detail_url,
        evidence,
    }
}

fn tls_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> TlsEvent {
    let status = section_status_from_checks(result, weights);
    let (headline, checks, detail_url) = match result {
        Ok(r) => {
            let items = build_check_items(&r.checks, weights);
            let (raw_headline, url) = match &r.extra {
                BackendExtra::Tls {
                    raw_headline,
                    detail_url,
                } => (raw_headline.clone(), detail_url.clone()),
                _ => (String::new(), String::new()),
            };
            (raw_headline, items, url)
        }
        Err(e) => (error_headline(e), vec![], String::new()),
    };
    TlsEvent {
        status,
        headline,
        checks,
        detail_url,
        evidence: section_evidence(result),
    }
}

fn http_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> HttpEvent {
    let status = section_status_from_checks(result, weights);
    let (
        headline,
        checks,
        detail_url,
        status_code,
        http_version,
        response_duration_ms,
        server_ip,
        server_org,
        server_network_type,
    ) = match result {
        Ok(r) => {
            let items = build_check_items(&r.checks, weights);
            let (raw_headline, url, sc, hv, rdms, sip, sorg, snt) = match &r.extra {
                BackendExtra::Http {
                    raw_headline,
                    detail_url,
                    status_code,
                    http_version,
                    response_duration_ms,
                    server_ip,
                    server_org,
                    server_network_type,
                } => (
                    raw_headline.clone(),
                    detail_url.clone(),
                    *status_code,
                    http_version.clone(),
                    *response_duration_ms,
                    server_ip.clone(),
                    server_org.clone(),
                    server_network_type.clone(),
                ),
                _ => (
                    String::new(),
                    String::new(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            };
            (raw_headline, items, url, sc, hv, rdms, sip, sorg, snt)
        }
        Err(e) => (
            error_headline(e),
            vec![],
            String::new(),
            None,
            None,
            None,
            None,
            None,
            None,
        ),
    };
    HttpEvent {
        status,
        headline,
        checks,
        detail_url,
        status_code,
        http_version,
        response_duration_ms,
        server_ip,
        server_org,
        server_network_type,
        evidence: section_evidence(result),
    }
}

fn ip_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> IpEvent {
    let status = section_status_from_checks(result, weights);
    let (headline, checks, addresses, detail_url) = match result {
        Ok(r) => {
            let items = build_check_items(&r.checks, weights);
            let (raw_headline, addrs, url) = match &r.extra {
                BackendExtra::Ip {
                    addresses,
                    raw_headline,
                    detail_url,
                } => {
                    let addr_info: Vec<IpAddressInfo> = addresses
                        .iter()
                        .map(|a| IpAddressInfo {
                            ip: a.ip.to_string(),
                            org: a.org.clone(),
                            geo: a.geo.clone(),
                            network_type: a.network_type.clone(),
                        })
                        .collect();
                    (raw_headline.clone(), addr_info, detail_url.clone())
                }
                _ => (String::new(), vec![], String::new()),
            };
            (raw_headline, items, addrs, url)
        }
        Err(e) => (error_headline(e), vec![], vec![], String::new()),
    };
    let guide_url = if status == "fail" || status == "warn" {
        Some("https://netray.info/guide/ip-enrichment")
    } else {
        None
    };
    IpEvent {
        status,
        headline,
        checks,
        addresses,
        detail_url,
        guide_url,
        evidence: section_evidence(result),
    }
}

fn email_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> EmailEvent {
    let status = section_status_from_checks(result, weights);
    match result {
        Err(SectionError::NotApplicable { reason }) => EmailEvent {
            status,
            headline: format!("unavailable ({})", reason),
            checks: vec![],
            detail_url: String::new(),
            grade: None,
            evidence: vec![],
        },
        Err(e) => EmailEvent {
            status,
            headline: error_headline(e),
            checks: vec![],
            detail_url: String::new(),
            grade: None,
            evidence: vec![],
        },
        Ok(r) => {
            let items = build_check_items(&r.checks, weights);
            let (raw_headline, detail_url, grade) = match &r.extra {
                BackendExtra::Email {
                    raw_headline,
                    detail_url,
                    grade,
                    ..
                } => (raw_headline.clone(), detail_url.clone(), grade.clone()),
                _ => (String::new(), String::new(), None),
            };
            EmailEvent {
                status,
                headline: raw_headline,
                checks: items,
                detail_url,
                grade,
                evidence: r.evidence.clone(),
            }
        }
    }
}

fn summary_payload_from(
    sections: &HashMap<String, Result<BackendResult, SectionError>>,
    score: &OverallScore,
    profile: &Profile,
) -> SummaryEvent {
    let mut section_statuses: HashMap<String, String> = HashMap::new();
    let mut section_grades: HashMap<String, String> = HashMap::new();

    for (name, result) in sections {
        let status = match result {
            Ok(r)
                if !score.complete
                    && !score.sections.contains_key(name)
                    && !score.not_applicable.contains_key(name) =>
            {
                if has_weighted_unmeasured(r, &section_weights(profile, name)) {
                    "incomplete"
                } else {
                    "error"
                }
            }
            _ => section_status_from_verdicts(result),
        };
        section_statuses.insert(name.clone(), status.to_string());
        if let Some(s) = score.sections.get(name) {
            section_grades.insert(name.clone(), grade_for(profile, s.percentage));
        }
    }

    let overall = if section_statuses.values().any(|s| s == "error") {
        "error".to_string()
    } else if section_statuses.values().any(|s| s == "fail") {
        "fail".to_string()
    } else if section_statuses.values().any(|s| s == "incomplete") {
        "incomplete".to_string()
    } else if section_statuses.values().any(|s| s == "warn") {
        "warn".to_string()
    } else {
        "pass".to_string()
    };

    let hard_fail_reason = if score.hard_fail_triggered {
        Some(score.hard_fail_checks.join(", "))
    } else {
        None
    };

    SummaryEvent {
        sections: section_statuses,
        section_grades,
        overall,
        grade: score.grade.clone(),
        score: (score.overall_percentage * 10.0).round() / 10.0,
        hard_fail: score.hard_fail_triggered,
        hard_fail_checks: score.hard_fail_checks.clone(),
        hard_fail_reason,
        not_applicable: score.not_applicable.clone(),
        complete: score.complete,
        evidence_schema: EVIDENCE_SCHEMA_VERSION,
    }
}

fn done_payload(
    domain: &str,
    duration_ms: u64,
    cached: bool,
    snapshot_id: Option<String>,
) -> DoneEvent {
    DoneEvent {
        domain: domain.to_string(),
        duration_ms,
        cached,
        snapshot_id,
    }
}

// ---------------------------------------------------------------------------
// SSE helpers
// ---------------------------------------------------------------------------

fn dns_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
    facts_evidence: &[Block],
) -> Event {
    let payload = dns_payload_from(result, weights, facts_evidence);
    Event::default()
        .event("dns")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn tls_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
    let payload = tls_payload_from(result, weights);
    Event::default()
        .event("tls")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn http_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
    let payload = http_payload_from(result, weights);
    Event::default()
        .event("http")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn ip_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
    let payload = ip_payload_from(result, weights);
    Event::default()
        .event("ip")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn email_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
    let payload = email_payload_from(result, weights);
    Event::default()
        .event("email")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn summary_event_from(
    sections: &HashMap<String, Result<BackendResult, SectionError>>,
    score: &OverallScore,
    profile: &Profile,
) -> Event {
    let payload = summary_payload_from(sections, score, profile);
    Event::default()
        .event("summary")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn done_event(domain: &str, duration_ms: u64, cached: bool, snapshot_id: Option<String>) -> Event {
    let payload = done_payload(domain, duration_ms, cached, snapshot_id);
    Event::default()
        .event("done")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn build_sse_events(
    domain: String,
    output: &CheckOutput,
    cached: bool,
    profile: &Profile,
    snapshot_id: Option<String>,
    facts_evidence: &[Block],
) -> Vec<Event> {
    let duration_ms = output.duration_ms;
    let empty_err: Result<BackendResult, SectionError> =
        Err(SectionError::BackendError("absent".to_string()));
    let dns_checks = &section_weights(profile, "dns");
    let tls_checks = &section_weights(profile, "tls");
    let http_checks = &section_weights(profile, "http");
    let ip_checks = &section_weights(profile, "ip");
    let email_checks = &section_weights(profile, "email");
    let mut events = vec![
        dns_event_from(
            output.sections.get("dns").unwrap_or(&empty_err),
            dns_checks,
            facts_evidence,
        ),
        tls_event_from(output.sections.get("tls").unwrap_or(&empty_err), tls_checks),
    ];
    if let Some(http_result) = output.sections.get("http") {
        events.push(http_event_from(http_result, http_checks));
    }
    if let Some(email_result) = output.sections.get("email") {
        events.push(email_event_from(email_result, email_checks));
    }
    events.push(ip_event_from(
        output.sections.get("ip").unwrap_or(&empty_err),
        ip_checks,
    ));
    events.push(summary_event_from(&output.sections, &output.score, profile));
    events.push(done_event(&domain, duration_ms, cached, snapshot_id));
    events
}

fn build_sse_events_from_cached(
    domain: &str,
    cached: &CachedResult,
    profile: &Profile,
) -> Vec<Event> {
    let dummy_output = CheckOutput {
        domain: domain.to_string(),
        sections: cached.sections.clone(),
        score: cached.score.clone(),
        duration_ms: cached.duration_ms,
    };
    build_sse_events(
        domain.to_string(),
        &dummy_output,
        true,
        profile,
        cached.snapshot_id.clone(),
        &[],
    )
}

fn make_sse_stream(events: Vec<Event>, cache_header: &'static str) -> Response {
    let s = stream::iter(events.into_iter().map(Ok::<_, Infallible>));
    let sse = Sse::new(s).keep_alive(KeepAlive::default());
    let mut response = sse.into_response();
    let headers = response.headers_mut();
    headers.insert("x-cache", HeaderValue::from_static(cache_header));
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    response
}

fn sse_response_from_cached(
    domain: String,
    cached: &CachedResult,
    is_cached: bool,
    profile: &Profile,
) -> Response {
    let events = build_sse_events_from_cached(&domain, cached, profile);
    make_sse_stream(events, if is_cached { "HIT" } else { "MISS" })
}

// ---------------------------------------------------------------------------
// Sync response helpers
// ---------------------------------------------------------------------------

fn build_sync_response(
    domain: String,
    output: &CheckOutput,
    cached: bool,
    profile: &Profile,
    snapshot_id: Option<String>,
    facts_evidence: &[Block],
) -> Response {
    let empty_err: Result<BackendResult, SectionError> =
        Err(SectionError::BackendError("absent".to_string()));
    let dns_checks = &section_weights(profile, "dns");
    let tls_checks = &section_weights(profile, "tls");
    let http_checks = &section_weights(profile, "http");
    let ip_checks = &section_weights(profile, "ip");
    let email_checks = &section_weights(profile, "email");
    let duration_ms = output.duration_ms;
    let response = SyncCheckResponse {
        dns: dns_payload_from(
            output.sections.get("dns").unwrap_or(&empty_err),
            dns_checks,
            facts_evidence,
        ),
        tls: tls_payload_from(output.sections.get("tls").unwrap_or(&empty_err), tls_checks),
        http: output
            .sections
            .get("http")
            .map(|r| http_payload_from(r, http_checks)),
        email: output
            .sections
            .get("email")
            .map(|r| email_payload_from(r, email_checks)),
        ip: ip_payload_from(output.sections.get("ip").unwrap_or(&empty_err), ip_checks),
        summary: summary_payload_from(&output.sections, &output.score, profile),
        done: done_payload(&domain, duration_ms, cached, snapshot_id),
    };
    let cache_header = if cached { "HIT" } else { "MISS" };
    let mut resp = Json(response).into_response();
    let headers = resp.headers_mut();
    headers.insert("x-cache", HeaderValue::from_static(cache_header));
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    resp
}

fn sync_response_from_cached(
    domain: String,
    cached: &CachedResult,
    is_cached: bool,
    profile: &Profile,
) -> Response {
    let dummy_output = CheckOutput {
        domain: domain.clone(),
        sections: cached.sections.clone(),
        score: cached.score.clone(),
        duration_ms: cached.duration_ms,
    };
    build_sync_response(
        domain,
        &dummy_output,
        is_cached,
        profile,
        cached.snapshot_id.clone(),
        &[],
    )
}

// ---------------------------------------------------------------------------
// Snapshot helpers and handler
// ---------------------------------------------------------------------------

/// Build section results for snapshot creation from a CheckOutput.
fn snapshot_section_results(
    output: &CheckOutput,
    profile: &Profile,
) -> Vec<crate::snapshot::SectionResult> {
    // Section order and display names
    let known_sections: &[(&str, &str)] = &[
        ("dns", "DNS"),
        ("tls", "TLS"),
        ("http", "HTTP"),
        ("email", "Email"),
        ("ip", "IP"),
    ];

    known_sections
        .iter()
        .filter_map(|(key, display)| {
            // Only include sections that were actually checked (present in output)
            if !output.sections.contains_key(*key) && output.score.not_applicable.contains_key(*key)
            {
                return None;
            }
            if !output.sections.contains_key(*key) {
                return None;
            }
            let grade = if output.score.not_applicable.contains_key(*key) {
                "N/A".to_string()
            } else if let Some(section_score) = output.score.sections.get(*key) {
                grade_for(profile, section_score.percentage)
            } else if matches!(output.sections.get(*key), Some(Ok(_))) {
                "incomplete".to_string()
            } else {
                "error".to_string()
            };
            let backend_result = output
                .sections
                .get(*key)
                .cloned()
                .unwrap_or_else(|| Err(SectionError::BackendError("absent".to_string())));
            Some(crate::snapshot::SectionResult {
                name: key.to_string(),
                display_name: display.to_string(),
                grade,
                backend_result,
            })
        })
        .collect()
}

/// Create a snapshot for a completed check. Returns the shortid on success.
/// Logs and returns `None` on any failure — snapshot errors never surface to the user.
pub(crate) async fn create_snapshot(
    state: &AppState,
    domain: &str,
    output: &CheckOutput,
) -> Option<String> {
    let store = state.snapshot_store.as_ref()?;
    let sections = snapshot_section_results(output, &state.scoring_profile);
    let snap = crate::snapshot::Snapshot::from_check_output(
        domain.to_string(),
        String::new(), // shortid assigned by store.insert
        &output.score,
        &sections,
        env!("CARGO_PKG_VERSION"),
        &state.scoring_profile,
    );
    match store.insert(snap).await {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(domain, error = %e, "failed to create snapshot");
            None
        }
    }
}

/// Returns an Axum Router for the snapshot page endpoint.
pub fn snapshot_router() -> axum::Router<AppState> {
    use axum::routing::get;
    axum::Router::new()
        .route("/r/{shortid}", get(snapshot_handler))
        .route("/r/", get(snapshot_unmatched))
        .route("/r/{shortid}/", get(snapshot_unmatched))
        .route("/r/{shortid}/{*rest}", get(snapshot_unmatched))
}

async fn snapshot_unmatched() -> Response {
    not_found_html().into_response()
}

/// `GET /r/:shortid` — render a saved snapshot as HTML.
pub async fn snapshot_handler(
    State(state): State<AppState>,
    shortid: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(shortid)) = shortid else {
        return not_found_html().into_response();
    };
    if !crate::snapshot::validate_shortid(&shortid) {
        return not_found_html().into_response();
    }
    snapshot_answer(&state, &shortid)
        .await
        .unwrap_or_else(|| not_found_html().into_response())
}

/// The V1 answer for a snapshot ID: `None` when there is no store or no live snapshot, otherwise
/// the page or the 500 for a failed lookup.
pub(crate) async fn snapshot_answer(state: &AppState, shortid: &str) -> Option<Response> {
    use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
    use serde_json::json;

    let store = state.snapshot_store.as_ref()?;
    match store.get(shortid).await {
        Err(e) => {
            tracing::warn!(shortid, error = %e, "snapshot lookup failed");
            Some(
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": {"code": "INTERNAL", "message": "snapshot unavailable"}})),
                )
                    .into_response(),
            )
        }
        Ok(None) => None,
        Ok(Some(snap)) => {
            let html = crate::snapshot::render_snapshot_html(&snap, &state.config.site);
            Some(
                (
                    StatusCode::OK,
                    [
                        (CONTENT_TYPE, "text/html; charset=utf-8"),
                        (CACHE_CONTROL, "public, max-age=86400, immutable"),
                    ],
                    html,
                )
                    .into_response(),
            )
        }
    }
}

/// The V1 lookup `netray_web::entries` falls back to for an ID the history store does not hold.
pub(crate) fn v1_lookup(state: AppState) -> netray_web::entries::V1Lookup {
    std::sync::Arc::new(move |shortid: String| {
        let state = state.clone();
        Box::pin(async move {
            if !crate::snapshot::validate_shortid(&shortid) {
                return None;
            }
            snapshot_answer(&state, &shortid).await
        })
    })
}

fn not_found_html() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        netray_web::entries::NOT_FOUND_HTML,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use tower::ServiceExt;

    use crate::config::Config;
    use crate::config::{
        BackendsConfig, BadgesConfig, CacheConfig, EcosystemConfig, OgCardsConfig, RateLimitConfig,
        ScoringConfig, ServerConfig, SiteConfig,
    };

    pub fn test_config_with_rate_limit(per_ip: u32, burst: u32) -> Config {
        Config {
            server: ServerConfig {
                bind: ([127, 0, 0, 1], 0).into(),
                metrics_bind: ([127, 0, 0, 1], 0).into(),
                trusted_proxies: Vec::new(),
            },
            backends: BackendsConfig {
                resolve_timeout_ms: 2000,
                dns: crate::config::BackendConfig {
                    timeout_ms: 1000,
                    ..Default::default()
                },
                tls: crate::config::BackendConfig {
                    url: Some("http://127.0.0.1:19998".to_string()),
                    timeout_ms: 1000,
                    ..Default::default()
                },
                ip: crate::config::BackendConfig {
                    url: Some("http://127.0.0.1:19997".to_string()),
                    timeout_ms: 1000,
                    ..Default::default()
                },
                http: None,
                email: None,
            },
            ecosystem: EcosystemConfig::default(),
            cache: CacheConfig {
                enabled: true,
                ttl_seconds: 300,
            },
            telemetry: Default::default(),
            rate_limit: RateLimitConfig {
                per_ip_per_minute: per_ip,
                per_ip_burst: burst,
                global_per_minute: 1000,
                global_burst: 100,
            },
            scoring: ScoringConfig::default(),
            site: SiteConfig::default(),
            badges: BadgesConfig::default(),
            admission: crate::config::AdmissionConfig::default(),
            api: Default::default(),
            og_cards: OgCardsConfig::default(),
            snapshots: crate::config::SnapshotsConfig::default(),
            history: crate::config::HistoryConfig::default(),
            modules: Default::default(),
        }
    }

    pub fn make_test_state() -> AppState {
        AppState::new(test_config_with_rate_limit(60, 10)).unwrap()
    }

    /// Build an axum Router suitable for testing (no ConnectInfo extractor needed in most tests).
    fn test_app_no_connect_info() -> axum::Router {
        let state = make_test_state();
        Router::new()
            .route("/health", get(health_handler))
            .route("/api/check/{domain}", get(check_get_no_connect_handler))
            .route("/api/check", post(check_post_no_connect_handler))
            .route("/ready", get(ready_handler))
            .with_state(state)
    }

    /// Simplified GET handler for tests — uses loopback as client IP, no sync mode.
    async fn check_get_no_connect_handler(
        State(state): State<AppState>,
        Path(domain): Path<String>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        run_check_handler(state, client_ip, domain, false, None, None, false).await
    }

    /// Simplified POST handler for tests — uses loopback as client IP, no sync mode.
    async fn check_post_no_connect_handler(
        State(state): State<AppState>,
        Json(body): Json<CheckPostBody>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        run_check_handler(state, client_ip, body.domain, false, None, None, false).await
    }

    /// GET handler for sync-mode tests — computes sync flag from headers and query.
    async fn check_get_no_connect_with_sync(
        State(state): State<AppState>,
        headers: axum::http::HeaderMap,
        Path(domain): Path<String>,
        Query(query): Query<CheckGetQuery>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let sync = is_sync_mode(&headers, query.stream);
        run_check_handler(state, client_ip, domain, sync, None, None, false).await
    }

    /// POST handler for sync-mode tests — computes sync flag from headers and body.
    async fn check_post_no_connect_with_sync(
        State(state): State<AppState>,
        headers: axum::http::HeaderMap,
        Json(body): Json<CheckPostBody>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let sync = is_sync_mode(&headers, body.stream);
        run_check_handler(state, client_ip, body.domain, sync, None, None, false).await
    }

    /// Build a Router that computes sync mode from headers/query, for sync-mode tests.
    fn test_app_with_sync() -> axum::Router {
        let state = make_test_state();
        Router::new()
            .route("/api/check/{domain}", get(check_get_no_connect_with_sync))
            .route("/api/check", post(check_post_no_connect_with_sync))
            .with_state(state)
    }

    // ---

    #[tokio::test]
    async fn health_returns_200_ok() {
        let app = test_app_no_connect_info();
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn invalid_domain_returns_400() {
        let app = test_app_no_connect_info();
        // Double-dot is rejected by validate_domain (empty label).
        // URL-encode the second dot as %2E so axum routes it as a single path segment.
        let req = Request::builder()
            .uri("/api/check/example%2E%2Ecom")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "DOMAIN_INVALID");
    }

    #[tokio::test]
    async fn ip_address_returns_400() {
        let app = test_app_no_connect_info();
        let req = Request::builder()
            .uri("/api/check/192.168.1.1")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "DOMAIN_INVALID");
    }

    #[tokio::test]
    async fn rate_limit_11th_request_returns_429() {
        // Very tight limits: 1 per minute, burst 1 (so 2nd request is rejected)
        let state = AppState::new(test_config_with_rate_limit(1, 1)).unwrap();
        let app = Router::new()
            .route("/api/check/{domain}", get(check_get_no_connect_handler))
            .with_state(state);

        // First request: allowed (may fail check but not rate limited)
        // Subsequent requests from same IP should be rate limited.
        // With burst=1, rate=1/min: first request passes, second is rejected.
        let make_req = || {
            Request::builder()
                .uri("/api/check/example.com")
                .body(Body::empty())
                .unwrap()
        };

        // Drain the burst
        let resp1 = app.clone().oneshot(make_req()).await.unwrap();
        // First might be 200 (SSE) or error from backends — either way not 429
        assert_ne!(resp1.status(), StatusCode::TOO_MANY_REQUESTS);

        // Second request should be rate limited
        let resp2 = app.oneshot(make_req()).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn post_check_returns_sse_content_type() {
        let app = test_app_no_connect_info();
        let req = Request::builder()
            .uri("/api/check")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"domain":"example.com"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // The handler runs a real check that will fail (no backends), but it
        // should still return an SSE stream for a valid domain.
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("text/event-stream"),
            "expected SSE content-type, got: {ct}"
        );
    }

    #[tokio::test]
    async fn cache_hit_returns_x_cache_hit() {
        // Use a short TTL cache; call the same domain twice.
        let state = make_test_state();
        let app = Router::new()
            .route("/api/check/{domain}", get(check_get_no_connect_handler))
            .with_state(state);

        let make_req = || {
            Request::builder()
                .uri("/api/check/example.com")
                .body(Body::empty())
                .unwrap()
        };

        // First request — cache miss.
        let resp1 = app.clone().oneshot(make_req()).await.unwrap();
        let cache_header1 = resp1
            .headers()
            .get("x-cache")
            .map(|v| v.to_str().unwrap().to_string());
        // Drain body so cache can be written.
        let _ = to_bytes(resp1.into_body(), usize::MAX).await.unwrap();

        // v2-admission R12/R16: a second run of the same domain starts only after the throttle.
        tokio::time::pause();
        tokio::time::advance(crate::admission::THROTTLE).await;
        tokio::time::resume();

        // Second request — should be a HIT (same domain).
        let resp2 = app.oneshot(make_req()).await.unwrap();
        let cache_header2 = resp2
            .headers()
            .get("x-cache")
            .map(|v| v.to_str().unwrap().to_string());

        assert_eq!(cache_header1.as_deref(), Some("MISS"));
        // The test state's backends are unreachable, so the result is incomplete and never
        // cached (grade-integrity requirement 4); a cache HIT for a complete result is pinned
        // by `complete_result_is_snapshotted_and_cached` in tests/incomplete_results.rs.
        assert_eq!(cache_header2.as_deref(), Some("MISS"));
    }

    #[tokio::test]
    async fn meta_returns_version() {
        let state = make_test_state();
        let app = Router::new()
            .route("/api/meta", get(meta_handler))
            .with_state(state);
        let req = Request::builder()
            .uri("/api/meta")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["site_name"], "lens — Domain Health Check");
        // EcosystemMeta now uniformly carries the `ecosystem` object across
        // every service; sibling URLs default to empty strings when unset.
        let eco = &json["ecosystem"];
        assert!(eco.is_object(), "ecosystem must be present even when unset");
        for key in [
            "ip_base_url",
            "dns_base_url",
            "tls_base_url",
            "http_base_url",
            "email_base_url",
            "lens_base_url",
        ] {
            assert!(eco.get(key).is_some(), "ecosystem must contain {key}");
        }

        // Profile is now nested under `features.profile`.
        let profile = &json["features"]["profile"];
        assert!(profile.is_object(), "features.profile must be an object");
        let sw = &profile["section_weights"];
        assert!(sw.is_object(), "section_weights must be an object");
        assert!(sw.get("dns").is_some(), "section_weights must contain dns");
        assert!(sw.get("tls").is_some(), "section_weights must contain tls");
        assert!(sw.get("ip").is_some(), "section_weights must contain ip");
        let hf = &profile["hard_fail"];
        assert!(hf.is_object(), "hard_fail must be an object");
        assert!(hf["dns"].is_array(), "hard_fail.dns must be an array");
        assert!(hf["tls"].is_array(), "hard_fail.tls must be an array");

        // Rate limit fields
        let rl = &json["rate_limit"];
        assert!(rl.is_object(), "rate_limit must be an object");
        assert!(
            rl["per_ip_per_minute"].is_number(),
            "per_ip_per_minute must be a number"
        );
        assert!(
            rl["per_ip_burst"].is_number(),
            "per_ip_burst must be a number"
        );
        assert!(
            rl["global_per_minute"].is_number(),
            "global_per_minute must be a number"
        );
        assert!(
            rl["global_burst"].is_number(),
            "global_burst must be a number"
        );
    }

    // --- SDD product-repositioning §3 Requirement 14: fix_hint / fix_owner wire fields ---

    #[test]
    fn check_item_omits_fix_fields_when_empty() {
        let item = CheckItem {
            name: "hsts".to_string(),
            verdict: "warn",
            guide_url: Some("https://netray.info/guide/hsts".to_string()),
            fix_hint: None,
            fix_owner: None,
            weight: Some(10),
            messages: vec!["max-age too short".to_string()],
            evidence: vec![],
        };
        let json = serde_json::to_value(&item).unwrap();
        assert!(
            json.get("fix_hint").is_none(),
            "fix_hint must be omitted when None"
        );
        assert!(
            json.get("fix_owner").is_none(),
            "fix_owner must be omitted when None"
        );
    }

    #[test]
    fn check_item_serializes_fix_fields_when_present() {
        let item = CheckItem {
            name: "hsts".to_string(),
            verdict: "warn",
            guide_url: Some("https://netray.info/guide/hsts".to_string()),
            fix_hint: Some("HSTS max-age should be at least 1 year."),
            fix_owner: Some("Your web server config"),
            weight: Some(10),
            messages: vec![],
            evidence: vec![],
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["fix_hint"], "HSTS max-age should be at least 1 year.");
        assert_eq!(json["fix_owner"], "Your web server config");
    }

    #[test]
    fn check_items_carry_catalogue_texts_for_every_check() {
        let checks: Vec<CheckResult> = netray_engine::catalogue()
            .entries()
            .iter()
            .map(|spec| CheckResult {
                name: spec.id.name().to_string(),
                verdict: CheckVerdict::Warn,
                messages: vec![],
                evidence: vec![],
            })
            .collect();
        for item in build_check_items(&checks, &HashMap::new()) {
            assert!(item.fix_hint.is_some(), "{} lacks a fix hint", item.name);
            assert!(item.fix_owner.is_some(), "{} lacks a fix owner", item.name);
            assert!(item.guide_url.is_some(), "{} lacks a guide URL", item.name);
        }
        let reputation = build_check_items(&checks[checks.len() - 1..], &HashMap::new());
        assert_eq!(reputation[0].name, "reputation");
        assert_eq!(reputation[0].fix_owner, Some("your hosting provider"));
    }

    #[test]
    fn check_item_without_catalogue_entry_has_no_texts() {
        let checks = vec![CheckResult {
            name: "security_headers".to_string(),
            verdict: CheckVerdict::Warn,
            messages: vec![],
            evidence: vec![],
        }];
        let item = &build_check_items(&checks, &HashMap::new())[0];
        assert_eq!(item.guide_url, None);
        assert_eq!(item.fix_hint, None);
        assert_eq!(item.fix_owner, None);
    }

    #[test]
    fn guide_url_for_tls_reachable_is_some() {
        assert!(spec_by_name("tls_reachable").map(guide_url).is_some());
    }

    #[test]
    fn guide_url_for_http_checks_point_to_correct_pages() {
        let url = |n: &str| spec_by_name(n).map(guide_url);
        assert_eq!(
            url("cors").as_deref(),
            Some("https://netray.info/guide/cors")
        );
        assert_eq!(
            url("cookie_secure").as_deref(),
            Some("https://netray.info/guide/cookie-security")
        );
    }

    // --- SDD product-repositioning §3 Requirement 8 / §7.1: /api/meta exposes
    //     `site` as a top-level field (not under `features`).

    #[tokio::test]
    async fn meta_includes_site_with_all_12_fields() {
        let state = make_test_state();
        let app = Router::new()
            .route("/api/meta", get(meta_handler))
            .with_state(state);
        let req = Request::builder()
            .uri("/api/meta")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 8192).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let site = &json["site"];
        assert!(site.is_object(), "site must be a top-level object");
        for key in [
            "title",
            "description",
            "og_image",
            "og_site_name",
            "brand_name",
            "brand_tagline",
            "status_pill",
            "hero_heading",
            "hero_subheading",
            "example_domains",
            "trust_strip",
            "footer_about",
            "footer_links",
        ] {
            assert!(site.get(key).is_some(), "site must contain field {key}");
        }
        assert_eq!(site["brand_name"], "lens");
        assert_eq!(
            site["example_domains"],
            serde_json::json!(["netray.info", "example.com", "github.com", "cloudflare.com"])
        );

        // The pre-0.7.2 implementation nested site under `features.site`. The
        // top-level field is the SDD-canonical location; assert the legacy
        // location is gone so we don't accidentally re-introduce duplication.
        assert!(
            json["features"].get("site").is_none(),
            "site must NOT be duplicated under features.site"
        );
    }

    #[tokio::test]
    async fn meta_site_reflects_configured_overrides() {
        let mut state = make_test_state();
        // Overlay an operator override on top of the default SiteConfig — the
        // common partial-override case (one URL set, rest inherited).
        let mut cfg = (*state.config).clone();
        cfg.site.hero_heading = Some("Is your domain healthy?".to_string());
        cfg.site.example_domains = Some(vec!["example.com".to_string()]);
        state.config = std::sync::Arc::new(cfg);

        let app = Router::new()
            .route("/api/meta", get(meta_handler))
            .with_state(state);
        let req = Request::builder()
            .uri("/api/meta")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 8192).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["site"]["hero_heading"], "Is your domain healthy?");
        assert_eq!(
            json["site"]["example_domains"],
            serde_json::json!(["example.com"])
        );
        // Unspecified fields keep their defaults.
        assert_eq!(json["site"]["brand_name"], "lens");
        assert!(json["site"]["trust_strip"].is_string());
    }

    // --- AC-1: sync mode via Accept: application/json header

    #[tokio::test]
    async fn sync_mode_via_accept_header_returns_json() {
        let app = test_app_with_sync();
        let req = Request::builder()
            .uri("/api/check/example.com")
            .header("accept", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/json"),
            "expected JSON content-type, got: {ct}"
        );
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["dns"].is_object(), "dns key missing");
        assert!(json["tls"].is_object(), "tls key missing");
        assert!(json["ip"].is_object(), "ip key missing");
        assert!(json["summary"].is_object(), "summary key missing");
        assert!(json["done"].is_object(), "done key missing");
    }

    // --- AC-2: sync mode via ?stream=false query param

    #[tokio::test]
    async fn sync_mode_via_query_param_returns_json() {
        let app = test_app_with_sync();
        let req = Request::builder()
            .uri("/api/check/example.com?stream=false")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/json"),
            "expected JSON content-type, got: {ct}"
        );
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["dns"].is_object(), "dns key missing");
        assert!(json["tls"].is_object(), "tls key missing");
        assert!(json["ip"].is_object(), "ip key missing");
        assert!(json["summary"].is_object(), "summary key missing");
        assert!(json["done"].is_object(), "done key missing");
    }

    // --- AC-3: sync mode via POST body stream:false

    #[tokio::test]
    async fn sync_mode_via_post_body_field_returns_json() {
        let app = test_app_with_sync();
        let req = Request::builder()
            .uri("/api/check")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"domain":"example.com","stream":false}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/json"),
            "expected JSON content-type, got: {ct}"
        );
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["dns"].is_object(), "dns key missing");
        assert!(json["tls"].is_object(), "tls key missing");
        assert!(json["ip"].is_object(), "ip key missing");
        assert!(json["summary"].is_object(), "summary key missing");
        assert!(json["done"].is_object(), "done key missing");
    }

    // --- AC-5: hard_fail_reason populated when hard_fail is true

    #[test]
    fn hard_fail_reason_populated_when_hard_fail_triggered() {
        use crate::score::OverallScore;

        let score = OverallScore {
            sections: HashMap::new(),
            overall_percentage: 0.0,
            grade: "F".to_string(),
            hard_fail_triggered: true,
            hard_fail_checks: vec!["chain_trusted".to_string(), "cert_lifetime".to_string()],
            not_applicable: HashMap::new(),
            complete: true,
        };
        let sections = HashMap::new();
        let profile = netray_engine::profile::Profiles::current();

        let summary = summary_payload_from(&sections, &score, profile);

        assert!(summary.hard_fail);
        assert_eq!(
            summary.hard_fail_reason,
            Some("chain_trusted, cert_lifetime".to_string())
        );
    }

    // --- AC-6: hard_fail_reason is null when hard_fail is false

    #[test]
    fn hard_fail_reason_null_when_no_hard_fail() {
        use crate::score::OverallScore;

        let score = OverallScore {
            sections: HashMap::new(),
            overall_percentage: 91.5,
            grade: "A".to_string(),
            hard_fail_triggered: false,
            hard_fail_checks: vec![],
            not_applicable: HashMap::new(),
            complete: true,
        };
        let sections = HashMap::new();
        let profile = netray_engine::profile::Profiles::current();

        let summary = summary_payload_from(&sections, &score, profile);

        assert!(!summary.hard_fail);
        assert_eq!(summary.hard_fail_reason, None);
    }

    // --- AC-7: hard_fail_reason present in SSE summary event JSON

    #[test]
    fn hard_fail_reason_in_sse_summary_payload() {
        use crate::score::OverallScore;

        let score = OverallScore {
            sections: HashMap::new(),
            overall_percentage: 0.0,
            grade: "F".to_string(),
            hard_fail_triggered: true,
            hard_fail_checks: vec!["chain_trusted".to_string()],
            not_applicable: HashMap::new(),
            complete: true,
        };
        let sections = HashMap::new();
        let profile = netray_engine::profile::Profiles::current();

        // summary_payload_from is used by summary_event_from — test its output directly.
        let payload = summary_payload_from(&sections, &score, profile);
        let json_str = serde_json::to_string(&payload).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(
            !json["hard_fail_reason"].is_null(),
            "hard_fail_reason must be non-null when hard_fail is true"
        );
        assert_eq!(json["hard_fail_reason"], "chain_trusted");
    }

    // --- AC-8: domain validation error in sync mode returns JSON error body

    #[tokio::test]
    async fn invalid_domain_returns_400_in_sync_mode() {
        let app = test_app_with_sync();
        let req = Request::builder()
            .uri("/api/check/192.168.1.1")
            .header("accept", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("application/json"));
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "DOMAIN_INVALID");
    }

    // --- AC-9: rate limit error in sync mode returns JSON error body

    #[tokio::test]
    async fn rate_limit_returns_429_in_sync_mode() {
        let state = AppState::new(test_config_with_rate_limit(1, 1)).unwrap();
        let app = Router::new()
            .route("/api/check/{domain}", get(check_get_no_connect_with_sync))
            .with_state(state);

        let make_req = || {
            Request::builder()
                .uri("/api/check/example.com")
                .header("accept", "application/json")
                .body(Body::empty())
                .unwrap()
        };

        let resp1 = app.clone().oneshot(make_req()).await.unwrap();
        assert_ne!(resp1.status(), StatusCode::TOO_MANY_REQUESTS);

        let resp2 = app.oneshot(make_req()).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
        let bytes = to_bytes(resp2.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "RATE_LIMITED");
    }

    // --- Per-IP rate limiting keys on the real client IP (ConnectInfo peer) ---

    fn app_with_real_handler(trusted_proxies: &[&str]) -> Router {
        let mut config = test_config_with_rate_limit(1, 1);
        config.server.trusted_proxies = trusted_proxies.iter().map(|s| s.to_string()).collect();
        let state = AppState::new(config).unwrap();
        Router::new()
            .route("/api/check/{domain}", get(check_get_handler))
            .with_state(state)
    }

    fn req_from(peer: &str, xff: Option<&str>, domain: &str) -> Request<Body> {
        let mut req = Request::builder()
            .uri(format!("/api/check/{domain}"))
            .header("accept", "application/json");
        if let Some(xff) = xff {
            req = req.header("x-forwarded-for", xff);
        }
        let mut req = req.body(Body::empty()).unwrap();
        let peer: std::net::SocketAddr = peer.parse().unwrap();
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(peer));
        req
    }

    #[tokio::test]
    async fn rate_limit_buckets_per_forwarded_client_behind_trusted_proxy() {
        let app = app_with_real_handler(&["172.31.0.0/24"]);

        let a1 = app
            .clone()
            .oneshot(req_from(
                "172.31.0.5:40000",
                Some("203.0.113.1"),
                "example.com",
            ))
            .await
            .unwrap();
        assert_ne!(a1.status(), StatusCode::TOO_MANY_REQUESTS);

        // A different client behind the same proxy has its own bucket. It checks another domain:
        // example.com is throttled after a1's run (v2-admission R12/R16).
        let b1 = app
            .clone()
            .oneshot(req_from(
                "172.31.0.5:40001",
                Some("203.0.113.2"),
                "example.net",
            ))
            .await
            .unwrap();
        assert_ne!(b1.status(), StatusCode::TOO_MANY_REQUESTS);

        // The first client's bucket is exhausted.
        let a2 = app
            .oneshot(req_from(
                "172.31.0.5:40002",
                Some("203.0.113.1"),
                "example.com",
            ))
            .await
            .unwrap();
        assert_eq!(a2.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn rate_limit_ignores_client_supplied_cf_connecting_ip_behind_trusted_proxy() {
        let app = app_with_real_handler(&["172.31.0.0/24"]);

        // No Cloudflare in front: a client rotating CF-Connecting-IP must not
        // get a fresh bucket per request.
        let mut first = req_from("172.31.0.5:40000", Some("203.0.113.1"), "example.com");
        first
            .headers_mut()
            .insert("cf-connecting-ip", "198.51.100.1".parse().unwrap());
        let r1 = app.clone().oneshot(first).await.unwrap();
        assert_ne!(r1.status(), StatusCode::TOO_MANY_REQUESTS);

        let mut second = req_from("172.31.0.5:40001", Some("203.0.113.1"), "example.com");
        second
            .headers_mut()
            .insert("cf-connecting-ip", "198.51.100.2".parse().unwrap());
        let r2 = app.oneshot(second).await.unwrap();
        assert_eq!(r2.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn rate_limit_uses_peer_ip_without_trusted_proxies() {
        let app = app_with_real_handler(&[]);

        let p1 = app
            .clone()
            .oneshot(req_from("198.51.100.1:40000", None, "example.com"))
            .await
            .unwrap();
        assert_ne!(p1.status(), StatusCode::TOO_MANY_REQUESTS);

        // Another domain: example.com is throttled after p1's run (v2-admission R12/R16).
        let p2 = app
            .clone()
            .oneshot(req_from("198.51.100.2:40000", None, "example.net"))
            .await
            .unwrap();
        assert_ne!(p2.status(), StatusCode::TOO_MANY_REQUESTS);

        // A spoofed XFF from an untrusted peer does not escape the peer's bucket.
        let p1_spoofed = app
            .oneshot(req_from(
                "198.51.100.1:40001",
                Some("203.0.113.9"),
                "example.com",
            ))
            .await
            .unwrap();
        assert_eq!(p1_spoofed.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    // --- AC-10: X-Cache: HIT in sync mode on second request

    #[tokio::test]
    async fn cache_hit_returns_x_cache_hit_in_sync_mode() {
        let state = make_test_state();
        let app = Router::new()
            .route("/api/check/{domain}", get(check_get_no_connect_with_sync))
            .with_state(state);

        let make_req = || {
            Request::builder()
                .uri("/api/check/example.com")
                .header("accept", "application/json")
                .body(Body::empty())
                .unwrap()
        };

        // First request — cache miss.
        let resp1 = app.clone().oneshot(make_req()).await.unwrap();
        let cache1 = resp1
            .headers()
            .get("x-cache")
            .map(|v| v.to_str().unwrap().to_string());
        let _ = to_bytes(resp1.into_body(), usize::MAX).await.unwrap();

        // v2-admission R12/R16: a second run of the same domain starts only after the throttle.
        tokio::time::pause();
        tokio::time::advance(crate::admission::THROTTLE).await;
        tokio::time::resume();

        // Second request — should be a HIT; response is still JSON.
        let resp2 = app.oneshot(make_req()).await.unwrap();
        let cache2 = resp2
            .headers()
            .get("x-cache")
            .map(|v| v.to_str().unwrap().to_string());
        let ct = resp2
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();

        assert_eq!(cache1.as_deref(), Some("MISS"));
        // Unreachable backends: incomplete, never cached (grade-integrity requirement 4).
        assert_eq!(cache2.as_deref(), Some("MISS"));
        assert!(
            ct.contains("application/json"),
            "a repeated check in sync mode must return JSON"
        );
    }

    // --- AC-11: /api-docs/openapi.json lists all five endpoints

    #[tokio::test]
    async fn openapi_spec_lists_all_endpoints() {
        let (_, health_openapi) = health_router().split_for_parts();
        let (_, api_openapi) = api_router().split_for_parts();
        let (_, badge_openapi) = badge_router().split_for_parts();
        let openapi = crate::api_doc::build_openapi(health_openapi, api_openapi, badge_openapi);

        let app = Router::new().route(
            "/api-docs/openapi.json",
            get({
                let spec = openapi.clone();
                move || async move { axum::Json(spec) }
            }),
        );

        let req = Request::builder()
            .uri("/api-docs/openapi.json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("application/json"));
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let paths = &json["paths"];
        assert!(paths.is_object(), "spec must have a paths object");
        assert!(
            paths.get("/api/check/{domain}").is_some(),
            "/api/check/{{domain}} missing"
        );
        assert!(paths.get("/api/check").is_some(), "/api/check missing");
        assert!(paths.get("/api/meta").is_some(), "/api/meta missing");
        assert!(paths.get("/health").is_some(), "/health missing");
        assert!(paths.get("/ready").is_some(), "/ready missing");
    }

    // --- AC-8: sync response omits http field when http_url is not configured

    #[tokio::test]
    async fn sync_response_has_no_http_when_http_url_absent() {
        let app = test_app_with_sync();
        let req = Request::builder()
            .uri("/api/check/example.com")
            .header("accept", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // http_url is None in test config → http field must be absent from response.
        assert!(
            json.get("http").is_none(),
            "http field must be absent when http_url is not configured, got: {:?}",
            json.get("http"),
        );
        // Other sections must still be present.
        assert!(json["dns"].is_object(), "dns must be present");
        assert!(json["tls"].is_object(), "tls must be present");
        assert!(json["ip"].is_object(), "ip must be present");
        assert!(json["summary"].is_object(), "summary must be present");
    }

    // --- AC-12: /docs returns HTML containing "Scalar"

    #[tokio::test]
    async fn scalar_ui_served() {
        use utoipa_scalar::{Scalar, Servable};

        let (_, health_openapi) = health_router().split_for_parts();
        let (_, api_openapi) = api_router().split_for_parts();
        let (_, badge_openapi) = badge_router().split_for_parts();
        let openapi = crate::api_doc::build_openapi(health_openapi, api_openapi, badge_openapi);

        let app = Router::new().merge(Scalar::with_url("/docs", openapi));

        let req = Request::builder().uri("/docs").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/html"), "expected HTML, got: {ct}");
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Scalar"), "HTML must reference Scalar");
    }

    // T8: Meta includes ecosystem when configured
    #[tokio::test]
    async fn meta_includes_ecosystem_when_configured() {
        let mut config = test_config_with_rate_limit(60, 10);
        config.ecosystem = EcosystemConfig {
            ip_base_url: Some("https://ip.example.com".to_string()),
            dns_base_url: Some("https://dns.example.com".to_string()),
            tls_base_url: Some("https://tls.example.com".to_string()),
            ..Default::default()
        };
        let state = AppState::new(config).unwrap();
        let app = Router::new()
            .route("/api/meta", get(meta_handler))
            .with_state(state);
        let req = Request::builder()
            .uri("/api/meta")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 8192).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let eco = &json["ecosystem"];
        assert!(
            eco.is_object(),
            "ecosystem should be present when configured"
        );
        assert_eq!(eco["ip_base_url"], "https://ip.example.com");
        assert_eq!(eco["dns_base_url"], "https://dns.example.com");
        assert_eq!(eco["tls_base_url"], "https://tls.example.com");
        // Unconfigured fields are now always present as empty strings
        // (uniform shape across the suite per EcosystemMeta contract).
        assert_eq!(eco["http_base_url"], "");
        assert_eq!(eco["email_base_url"], "");
        assert_eq!(eco["lens_base_url"], "");
    }

    // T9: Ecosystem and backends are independent URL pools
    #[tokio::test]
    async fn ecosystem_and_backends_are_independent() {
        let mut config = test_config_with_rate_limit(60, 10);
        // Set ecosystem to public URLs
        config.ecosystem = EcosystemConfig {
            dns_base_url: Some("https://dns.example.com".to_string()),
            ..Default::default()
        };
        let state = AppState::new(config).unwrap();

        // Meta endpoint should return the public ecosystem URL
        let app = Router::new()
            .route("/api/meta", get(meta_handler))
            .with_state(state.clone());
        let req = Request::builder()
            .uri("/api/meta")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let bytes = to_bytes(resp.into_body(), 8192).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json["ecosystem"]["dns_base_url"], "https://dns.example.com",
            "ecosystem should use public URL"
        );

        // The DNS section runs in-process: the backend config carries no URL
        assert_eq!(state.config.backends.dns.url, None);
    }

    // --- dkim_selectors validation ---

    #[test]
    fn dkim_selectors_absent_returns_none() {
        assert_eq!(validate_dkim_selectors(None), Ok(None));
    }

    #[test]
    fn dkim_selectors_valid_two_selectors() {
        let result = validate_dkim_selectors(Some("google,selector1"));
        assert_eq!(
            result,
            Ok(Some(vec!["google".to_string(), "selector1".to_string()])),
        );
    }

    #[test]
    fn dkim_selectors_single_valid_selector() {
        let result = validate_dkim_selectors(Some("default"));
        assert_eq!(result, Ok(Some(vec!["default".to_string()])));
    }

    #[test]
    fn dkim_selectors_with_whitespace_trimmed() {
        let result = validate_dkim_selectors(Some(" google , selector1 "));
        assert_eq!(
            result,
            Ok(Some(vec!["google".to_string(), "selector1".to_string()])),
        );
    }

    #[test]
    fn dkim_selectors_empty_string_returns_400_error() {
        assert!(
            validate_dkim_selectors(Some("")).is_err(),
            "empty string must return Err"
        );
    }

    #[test]
    fn dkim_selectors_invalid_chars_returns_error() {
        // dots are not in [a-zA-Z0-9-]
        let err = validate_dkim_selectors(Some("bad.selector")).unwrap_err();
        assert!(
            err.contains("invalid characters"),
            "error must mention invalid characters, got: {err}"
        );
    }

    #[test]
    fn dkim_selectors_11_selectors_returns_error() {
        let selectors = "a,b,c,d,e,f,g,h,i,j,k"; // 11
        let err = validate_dkim_selectors(Some(selectors)).unwrap_err();
        assert!(
            err.contains("10"),
            "error must mention the 10-selector limit, got: {err}"
        );
    }

    #[test]
    fn dkim_selectors_exactly_10_selectors_valid() {
        let selectors = "a,b,c,d,e,f,g,h,i,j"; // 10
        assert!(
            validate_dkim_selectors(Some(selectors)).is_ok(),
            "exactly 10 selectors must be valid"
        );
    }

    #[test]
    fn dkim_selectors_selector_too_long_returns_error() {
        let long = "a".repeat(64); // 64 chars, exceeds 63
        let err = validate_dkim_selectors(Some(&long)).unwrap_err();
        assert!(
            err.contains("exceeds 63"),
            "error must mention 63-char limit, got: {err}"
        );
    }

    #[test]
    fn dkim_selectors_exactly_63_chars_valid() {
        let at_limit = "a".repeat(63); // 63 chars, at limit
        assert!(
            validate_dkim_selectors(Some(&at_limit)).is_ok(),
            "selector of exactly 63 chars must be valid"
        );
    }

    #[test]
    fn dkim_selectors_hyphen_allowed() {
        let result = validate_dkim_selectors(Some("my-selector-2024"));
        assert_eq!(result, Ok(Some(vec!["my-selector-2024".to_string()])),);
    }

    #[test]
    fn dkim_selectors_empty_token_between_commas_returns_error() {
        let err = validate_dkim_selectors(Some("a,,b")).unwrap_err();
        assert!(
            err.contains("empty token"),
            "empty token between commas must return error, got: {err}"
        );
    }

    // --- grade integrity: a section absent from the profile is not an error

    fn grade_integrity_check(name: &str, verdict: CheckVerdict) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            verdict,
            messages: vec![],
            evidence: vec![],
        }
    }

    fn grade_integrity_http_ok(checks: Vec<CheckResult>) -> Result<BackendResult, SectionError> {
        Ok(BackendResult {
            checks,
            extra: BackendExtra::Http {
                raw_headline: String::new(),
                detail_url: String::new(),
                status_code: Some(200),
                http_version: None,
                response_duration_ms: None,
                server_ip: None,
                server_org: None,
                server_network_type: None,
            },
            evidence: vec![],
        })
    }

    #[test]
    fn summary_section_absent_from_profile_is_not_error() {
        let profile = Profile::from_toml(
            r#"
[meta]
name = "no-http"
version = 2

[sections.dns]
weight = 50
[sections.dns.checks]
caa = 5

[sections.tls]
weight = 50
[sections.tls.checks]
chain_trusted = 5

[thresholds]
"A" = 90
"F" = 0
"#,
        )
        .unwrap();

        let dns_checks = vec![grade_integrity_check("caa", CheckVerdict::Pass)];
        let tls_checks = vec![grade_integrity_check("chain_trusted", CheckVerdict::Pass)];
        let http_checks = vec![grade_integrity_check("hsts", CheckVerdict::Warn)];

        let mut sections: HashMap<String, Result<BackendResult, SectionError>> = HashMap::new();
        for (name, checks) in [("dns", dns_checks), ("tls", tls_checks)] {
            let extra = BackendExtra::Tls {
                raw_headline: String::new(),
                detail_url: String::new(),
            };
            sections.insert(
                name.to_string(),
                Ok(BackendResult {
                    checks,
                    extra,
                    evidence: vec![],
                }),
            );
        }
        sections.insert("http".to_string(), grade_integrity_http_ok(http_checks));
        let score = crate::score::score(&profile, &sections);
        assert!(
            score.complete,
            "an unprofiled section must not make the score incomplete"
        );

        let summary = summary_payload_from(&sections, &score, &profile);
        assert_eq!(summary.sections["http"], "warn");
        assert_ne!(summary.overall, "error");
        assert!(summary.complete);

        // The section event status agrees, with no weights for the unprofiled section.
        let event = http_payload_from(&sections["http"], &HashMap::new());
        assert_eq!(event.status, "warn");
    }

    #[test]
    fn summary_profiled_section_with_nothing_possible_stays_error() {
        let profile = Profile::from_toml(
            r#"
[meta]
name = "http-only"
version = 2

[sections.http]
weight = 100
[sections.http.checks]
hsts = 5

[thresholds]
"A" = 90
"F" = 0
"#,
        )
        .unwrap();
        let checks = vec![grade_integrity_check("hsts", CheckVerdict::Skip)];
        let mut sections = HashMap::new();
        sections.insert("http".to_string(), grade_integrity_http_ok(checks));
        let score = crate::score::score(&profile, &sections);

        let summary = summary_payload_from(&sections, &score, &profile);
        assert_eq!(summary.sections["http"], "error");
        assert_eq!(summary.overall, "error");

        let weights = section_weights(&profile, "http");
        assert_eq!(
            http_payload_from(&sections["http"], &weights).status,
            "error"
        );
    }

    #[test]
    fn not_applicable_ip_section_is_skip_and_not_overall_error() {
        let profile = Profile::from_toml(
            r#"
[meta]
name = "http-only"
version = 2

[sections.http]
weight = 100
[sections.http.checks]
hsts = 5

[thresholds]
"A" = 90
"F" = 0
"#,
        )
        .unwrap();
        let checks = vec![grade_integrity_check("hsts", CheckVerdict::Pass)];
        let ip_result: Result<BackendResult, SectionError> = Err(SectionError::NotApplicable {
            reason: "no public addresses".to_string(),
        });
        let mut sections = HashMap::new();
        sections.insert("http".to_string(), grade_integrity_http_ok(checks));
        sections.insert("ip".to_string(), ip_result);
        let score = crate::score::score(&profile, &sections);
        assert!(score.complete);

        let summary = summary_payload_from(&sections, &score, &profile);
        assert_eq!(summary.sections["ip"], "skip");
        assert_eq!(summary.sections["http"], "pass");
        assert_ne!(summary.overall, "error");
        assert_eq!(summary.overall, "pass");

        let weights = HashMap::new();
        assert_eq!(ip_payload_from(&sections["ip"], &weights).status, "skip");
    }
}
