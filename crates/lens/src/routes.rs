use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::SystemTime;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::stream;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;

use crate::backends::{BackendExtra, BackendResult};
use crate::cache::{CachedResult, cache_key, is_fresh};
use crate::check::{CheckInput, CheckOutput, SectionError, run_check_with_input};
use crate::input::validate_domain;
use crate::scoring::engine::{CheckResult, CheckVerdict, OverallScore};
use crate::security::{check_rate_limit, extract_client_ip};
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
    #[schema(value_type = Option<String>)]
    pub guide_url: Option<&'static str>,
    /// Plain-English remediation sentence ("Your SPF record doesn't include
    /// your MX hosts — external mail from you may be rejected."). Populated
    /// per-check from `fix_for()`; absent until SDD product-repositioning
    /// Phase 4 fills the copy in.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub fix_hint: Option<&'static str>,
    /// Who can fix it ("your DNS provider", "your web server config").
    /// Populated alongside `fix_hint`. Absent until Phase 4.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub fix_owner: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
}

/// Return the guide URL for a check name.
fn guide_url_for(name: &str) -> Option<&'static str> {
    match name {
        // DNS — DNSSEC
        "dnssec" | "dnskey_algorithm" | "dnssec_rollover" => {
            Some("https://netray.info/guide/dnssec")
        }
        // DNS — record types & infrastructure
        "cname_apex" | "https_svcb" | "ns" | "ttl" => {
            Some("https://netray.info/guide/record-types")
        }
        "caa" => Some("https://netray.info/guide/caa-records"),
        "ns_lame" | "ns_delegation" => Some("https://netray.info/guide/lame-delegation"),
        "infrastructure" => Some("https://netray.info/guide/ip-enrichment"),
        // TLS — certificate chain
        "chain_trusted" | "chain_complete" | "strong_signature" | "key_strength"
        | "not_expired" | "hostname_match" => Some("https://netray.info/guide/certificate-chain"),
        // TLS — certificate management (expiry, lifetime, SAN, AIA)
        "expiry_window" | "cert_lifetime" | "san_quality" | "aia_reachability" => {
            Some("https://netray.info/guide/certificate-management")
        }
        // TLS — protocol & cipher suites
        "tls_version" | "forward_secrecy" | "aead_cipher" => {
            Some("https://netray.info/guide/tls-protocol")
        }
        // TLS — multi-IP consistency
        "consistency" | "alpn_consistency" => Some("https://netray.info/guide/multi-ip-tls"),
        // TLS — Encrypted Client Hello
        "ech_advertised" => Some("https://netray.info/guide/encrypted-client-hello"),
        "ct_logged" => Some("https://netray.info/guide/certificate-transparency"),
        "ocsp_stapled" => Some("https://netray.info/guide/certificate-chain"),
        "hsts" | "https_redirect" => Some("https://netray.info/guide/hsts"),
        "security_headers" | "cors" | "cookie_secure" | "hygiene" => {
            Some("https://netray.info/guide/http-security")
        }
        "dane_valid" | "caa_compliant" => Some("https://netray.info/guide/dane-tlsa"),
        // Email buckets
        "email_authentication"
        | "email_infrastructure"
        | "email_transport"
        | "email_brand_policy" => Some("https://netray.info/guide/email-auth"),
        // IP
        "reputation" => Some("https://netray.info/guide/ip-enrichment"),
        _ => None,
    }
}

/// Return the `(fix_hint, fix_owner)` pair for a check name.
///
/// Plain-English remediation copy. Empty for every check today; SDD
/// product-repositioning Phase 4 fills entries one-by-one as copy is written.
/// The mechanism (this lookup + the wire fields on `CheckItem`) ships now so
/// the frontend can render remediation blocks the moment any entry is added.
/// Phase 4 PRs convert this to a `match name { ... }` as entries are written.
fn fix_for(_name: &str) -> (Option<&'static str>, Option<&'static str>) {
    (None, None)
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
}

#[derive(Serialize, ToSchema)]
pub struct TlsEvent {
    #[schema(value_type = String)]
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub detail_url: String,
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
}

#[derive(Serialize, ToSchema)]
pub struct SummaryEvent {
    /// Section status map, keyed by section name.
    pub sections: HashMap<String, String>,
    pub section_grades: HashMap<String, String>,
    pub overall: String,
    pub grade: String,
    pub score: f64,
    pub hard_fail: bool,
    pub hard_fail_checks: Vec<String>,
    /// Comma-joined list of hard-fail check names; null when `hard_fail` is false.
    pub hard_fail_reason: Option<String>,
    /// Section name → reason. Always present (may be empty). Populated only for NotApplicable.
    pub not_applicable: HashMap<String, String>,
}

#[derive(Serialize, ToSchema)]
pub struct DoneEvent {
    pub domain: String,
    pub duration_ms: u64,
    pub cached: bool,
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
    for (name, section) in &profile.sections {
        all_checks.extend(section.checks.iter().map(|(k, v)| (k.clone(), *v)));
        section_weights.insert(name.clone(), section.weight);
        hard_fail.insert(name.clone(), section.hard_fail.clone());
    }
    let profile_data = ProfileData {
        name: profile.meta.name.clone(),
        version: profile.meta.version,
        checks: all_checks,
        section_weights,
        thresholds: profile.thresholds.clone(),
        hard_fail,
    };

    let mut features = Map::new();
    features.insert(
        "profile".into(),
        serde_json::to_value(&profile_data).unwrap_or(serde_json::Value::Null),
    );
    features.insert(
        "site".into(),
        serde_json::to_value(&config.site).unwrap_or(serde_json::Value::Null),
    );

    let rl = &config.rate_limit;
    Json(EcosystemMeta {
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
    })
}

#[utoipa::path(
    get,
    path = "/ready",
    tag = "health",
    responses(
        (status = 200, description = "All backends reachable", body = ReadyResponse),
        (status = 503, description = "One or more backends unreachable", body = ReadyResponse)
    )
)]
pub async fn ready_handler(State(state): State<AppState>) -> impl IntoResponse {
    let client = &state.http_client;
    let config = &state.config;
    let mut down: Vec<String> = Vec::new();

    // Build list of required backends to probe (3 s timeout each).
    // Optional http backend is only probed when its url is configured.
    let mut probes: Vec<(String, String)> = vec![
        (
            "dns".to_string(),
            config.backends.dns.url.clone().unwrap_or_default(),
        ),
        (
            "tls".to_string(),
            config.backends.tls.url.clone().unwrap_or_default(),
        ),
        (
            "ip".to_string(),
            config.backends.ip.url.clone().unwrap_or_default(),
        ),
    ];
    if let Some(ref http_cfg) = config.backends.http
        && let Some(ref url) = http_cfg.url
    {
        probes.push(("http".to_string(), url.clone()));
    }
    if let Some(ref email_cfg) = config.backends.email
        && let Some(ref url) = email_cfg.url
    {
        probes.push(("email".to_string(), url.clone()));
    }

    let futures: Vec<_> = probes
        .into_iter()
        .map(|(name, url)| {
            let client = client.clone();
            async move {
                if url.is_empty() {
                    return Some(name);
                }
                let health_url = format!("{}/health", url.trim_end_matches('/'));
                let ok = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    client.get(&health_url).send(),
                )
                .await
                .ok()
                .and_then(|r| r.ok())
                .map(|r| r.status().is_success())
                .unwrap_or(false);
                if !ok { Some(name) } else { None }
            }
        })
        .collect();

    for name in futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
    {
        down.push(name);
    }

    if down.is_empty() {
        (
            StatusCode::OK,
            Json(ReadyResponse {
                status: "ok",
                down: vec![],
            }),
        )
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadyResponse {
                status: "degraded",
                down,
            }),
        )
            .into_response()
    }
}

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
    )
)]
pub async fn check_get_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(domain): Path<String>,
    Query(query): Query<CheckGetQuery>,
) -> Response {
    let client_ip =
        extract_client_ip_from_peer(&headers, &state.config.server.trusted_proxies, peer.ip());
    let sync = is_sync_mode(&headers, query.stream);
    let dkim_selectors = match validate_dkim_selectors(query.dkim_selectors.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            return crate::error::AppError::InvalidInput(msg).into_response();
        }
    };
    run_check_handler(state, client_ip, domain, sync, dkim_selectors).await
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
    )
)]
pub async fn check_post_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(body): Json<CheckPostBody>,
) -> Response {
    let client_ip =
        extract_client_ip_from_peer(&headers, &state.config.server.trusted_proxies, peer.ip());
    let sync = is_sync_mode(&headers, body.stream);
    let dkim_selectors = match validate_dkim_selectors(body.dkim_selectors.as_deref()) {
        Ok(s) => s,
        Err(msg) => {
            return crate::error::AppError::InvalidInput(msg).into_response();
        }
    };
    run_check_handler(state, client_ip, body.domain, sync, dkim_selectors).await
}

fn extract_client_ip_from_peer(
    headers: &axum::http::HeaderMap,
    trusted_proxies: &[String],
    peer_ip: IpAddr,
) -> IpAddr {
    if trusted_proxies.is_empty() {
        return peer_ip;
    }
    extract_client_ip(headers, trusted_proxies)
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

async fn run_check_handler(
    state: AppState,
    client_ip: IpAddr,
    domain_raw: String,
    sync: bool,
    dkim_selectors: Option<Vec<String>>,
) -> Response {
    // Record client_ip into the TraceLayer span.
    tracing::Span::current().record("client_ip", tracing::field::display(&client_ip));

    // 1. Validate domain.
    let domain = match validate_domain(&domain_raw) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };

    // 2. Rate limit.
    if let Err(e) = check_rate_limit(&state.per_ip_limiter, &state.global_limiter, client_ip) {
        return e.into_response();
    }

    // 3. Cache lookup (only when no per-request options like dkim_selectors).
    let key = cache_key(&domain);
    if dkim_selectors.is_none()
        && let Some(cache) = &state.cache
        && let Some(cached) = cache.get(&key).await
        && is_fresh(&cached, state.config.cache.ttl_seconds)
    {
        return if sync {
            sync_response_from_cached(domain, &cached, true, &state.scoring_profile)
        } else {
            sse_response_from_cached(domain, &cached, true, &state.scoring_profile)
        };
    }

    // 4. Run check.
    let output = run_check_with_input(
        &state,
        CheckInput {
            domain: domain.clone(),
            dkim_selectors,
        },
    )
    .await;
    let duration_ms = output.duration_ms;
    let domain_out = output.domain.clone();

    // 5. Store in cache.
    if let Some(cache) = &state.cache {
        let entry = Arc::new(CachedResult {
            sections: output.sections.clone(),
            score: output.score.clone(),
            duration_ms,
            cached_at: SystemTime::now(),
        });
        cache.insert(key, entry).await;
    }

    // 6. Return SSE stream or sync JSON.
    if sync {
        build_sync_response(domain_out, output, false, &state.scoring_profile)
    } else {
        let events = build_sse_events(domain_out, output, false, &state.scoring_profile);
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
    }
}

fn section_status_from_checks(result: &Result<BackendResult, SectionError>) -> &'static str {
    match result {
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
            let (fix_hint, fix_owner) = fix_for(&c.name);
            CheckItem {
                guide_url: guide_url_for(&c.name),
                fix_hint,
                fix_owner,
                name: c.name.clone(),
                verdict,
                weight: weights.get(&c.name).copied(),
                messages: c.messages.clone(),
            }
        })
        .collect()
}

fn error_headline(e: &SectionError) -> String {
    match e {
        SectionError::Timeout => "timeout".to_string(),
        SectionError::NoDnsResults => "no DNS results".to_string(),
        SectionError::BackendError(_) => "backend error".to_string(),
        SectionError::NotApplicable { .. } => "not applicable".to_string(),
    }
}

fn dns_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> DnsEvent {
    let status = section_status_from_checks(result);
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
    DnsEvent {
        status,
        headline,
        checks,
        detail_url,
    }
}

fn tls_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> TlsEvent {
    let status = section_status_from_checks(result);
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
    }
}

fn http_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> HttpEvent {
    let status = section_status_from_checks(result);
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
    }
}

fn ip_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> IpEvent {
    let status = section_status_from_checks(result);
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
    }
}

fn email_payload_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> EmailEvent {
    let status = section_status_from_checks(result);
    match result {
        Err(SectionError::NotApplicable { reason }) => EmailEvent {
            status,
            headline: format!("unavailable ({})", reason),
            checks: vec![],
            detail_url: String::new(),
            grade: None,
        },
        Err(e) => EmailEvent {
            status,
            headline: error_headline(e),
            checks: vec![],
            detail_url: String::new(),
            grade: None,
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
            }
        }
    }
}

fn summary_payload_from(
    sections: &HashMap<String, Result<BackendResult, SectionError>>,
    score: &OverallScore,
    thresholds: &std::collections::BTreeMap<String, u32>,
) -> SummaryEvent {
    use crate::scoring::engine::lookup_grade;

    let mut section_statuses: HashMap<String, String> = HashMap::new();
    let mut section_grades: HashMap<String, String> = HashMap::new();

    for (name, result) in sections {
        section_statuses.insert(name.clone(), section_status_from_checks(result).to_string());
        if let Some(s) = score.sections.get(name) {
            section_grades.insert(name.clone(), lookup_grade(thresholds, s.percentage));
        }
    }

    let overall = if section_statuses.values().any(|s| s == "error") {
        "error".to_string()
    } else if section_statuses.values().any(|s| s == "fail") {
        "fail".to_string()
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
    }
}

fn done_payload(domain: &str, duration_ms: u64, cached: bool) -> DoneEvent {
    DoneEvent {
        domain: domain.to_string(),
        duration_ms,
        cached,
    }
}

// ---------------------------------------------------------------------------
// SSE helpers
// ---------------------------------------------------------------------------

fn dns_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
    let payload = dns_payload_from(result, weights);
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
    thresholds: &std::collections::BTreeMap<String, u32>,
) -> Event {
    let payload = summary_payload_from(sections, score, thresholds);
    Event::default()
        .event("summary")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn done_event(domain: &str, duration_ms: u64, cached: bool) -> Event {
    let payload = done_payload(domain, duration_ms, cached);
    Event::default()
        .event("done")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn build_sse_events(
    domain: String,
    output: CheckOutput,
    cached: bool,
    profile: &crate::scoring::profile::ScoringProfile,
) -> Vec<Event> {
    let duration_ms = output.duration_ms;
    let empty_checks = HashMap::new();
    let empty_err: Result<BackendResult, SectionError> =
        Err(SectionError::BackendError("absent".to_string()));
    let dns_checks = profile
        .sections
        .get("dns")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let tls_checks = profile
        .sections
        .get("tls")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let http_checks = profile
        .sections
        .get("http")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let ip_checks = profile
        .sections
        .get("ip")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let email_checks = profile
        .sections
        .get("email")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let mut events = vec![
        dns_event_from(output.sections.get("dns").unwrap_or(&empty_err), dns_checks),
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
    events.push(summary_event_from(
        &output.sections,
        &output.score,
        &profile.thresholds,
    ));
    events.push(done_event(&domain, duration_ms, cached));
    events
}

fn build_sse_events_from_cached(
    domain: &str,
    cached: &CachedResult,
    profile: &crate::scoring::profile::ScoringProfile,
) -> Vec<Event> {
    let dummy_output = CheckOutput {
        domain: domain.to_string(),
        sections: cached.sections.clone(),
        score: cached.score.clone(),
        duration_ms: cached.duration_ms,
    };
    build_sse_events(domain.to_string(), dummy_output, true, profile)
}

fn make_sse_stream(events: Vec<Event>, cache_header: &'static str) -> Response {
    let s = stream::iter(events.into_iter().map(Ok::<_, Infallible>));
    let sse = Sse::new(s).keep_alive(KeepAlive::default());
    let mut response = sse.into_response();
    response
        .headers_mut()
        .insert("x-cache", HeaderValue::from_static(cache_header));
    response
}

fn sse_response_from_cached(
    domain: String,
    cached: &CachedResult,
    is_cached: bool,
    profile: &crate::scoring::profile::ScoringProfile,
) -> Response {
    let events = build_sse_events_from_cached(&domain, cached, profile);
    make_sse_stream(events, if is_cached { "HIT" } else { "MISS" })
}

// ---------------------------------------------------------------------------
// Sync response helpers
// ---------------------------------------------------------------------------

fn build_sync_response(
    domain: String,
    output: CheckOutput,
    cached: bool,
    profile: &crate::scoring::profile::ScoringProfile,
) -> Response {
    let empty_checks = HashMap::new();
    let empty_err: Result<BackendResult, SectionError> =
        Err(SectionError::BackendError("absent".to_string()));
    let dns_checks = profile
        .sections
        .get("dns")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let tls_checks = profile
        .sections
        .get("tls")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let http_checks = profile
        .sections
        .get("http")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let ip_checks = profile
        .sections
        .get("ip")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let email_checks = profile
        .sections
        .get("email")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    let duration_ms = output.duration_ms;
    let response = SyncCheckResponse {
        dns: dns_payload_from(output.sections.get("dns").unwrap_or(&empty_err), dns_checks),
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
        summary: summary_payload_from(&output.sections, &output.score, &profile.thresholds),
        done: done_payload(&domain, duration_ms, cached),
    };
    let cache_header = if cached { "HIT" } else { "MISS" };
    let mut resp = Json(response).into_response();
    resp.headers_mut()
        .insert("x-cache", HeaderValue::from_static(cache_header));
    resp
}

fn sync_response_from_cached(
    domain: String,
    cached: &CachedResult,
    is_cached: bool,
    profile: &crate::scoring::profile::ScoringProfile,
) -> Response {
    let dummy_output = CheckOutput {
        domain: domain.clone(),
        sections: cached.sections.clone(),
        score: cached.score.clone(),
        duration_ms: cached.duration_ms,
    };
    build_sync_response(domain, dummy_output, is_cached, profile)
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
        BackendsConfig, CacheConfig, EcosystemConfig, RateLimitConfig, ScoringConfig, ServerConfig,
        SiteConfig,
    };

    pub fn test_config_with_rate_limit(per_ip: u32, burst: u32) -> Config {
        Config {
            server: ServerConfig {
                bind: ([127, 0, 0, 1], 0).into(),
                metrics_bind: ([127, 0, 0, 1], 0).into(),
                trusted_proxies: Vec::new(),
            },
            backends: BackendsConfig {
                dns: netray_common::backend::BackendConfig {
                    url: Some("http://127.0.0.1:19999".to_string()),
                    timeout_ms: 1000,
                    ..Default::default()
                },
                dns_servers: Vec::new(),
                tls: netray_common::backend::BackendConfig {
                    url: Some("http://127.0.0.1:19998".to_string()),
                    timeout_ms: 1000,
                    ..Default::default()
                },
                ip: netray_common::backend::BackendConfig {
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
        run_check_handler(state, client_ip, domain, false, None).await
    }

    /// Simplified POST handler for tests — uses loopback as client IP, no sync mode.
    async fn check_post_no_connect_handler(
        State(state): State<AppState>,
        Json(body): Json<CheckPostBody>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        run_check_handler(state, client_ip, body.domain, false, None).await
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
        run_check_handler(state, client_ip, domain, sync, None).await
    }

    /// POST handler for sync-mode tests — computes sync flag from headers and body.
    async fn check_post_no_connect_with_sync(
        State(state): State<AppState>,
        headers: axum::http::HeaderMap,
        Json(body): Json<CheckPostBody>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let sync = is_sync_mode(&headers, body.stream);
        run_check_handler(state, client_ip, body.domain, sync, None).await
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

        // Second request — should be a HIT (same domain).
        let resp2 = app.oneshot(make_req()).await.unwrap();
        let cache_header2 = resp2
            .headers()
            .get("x-cache")
            .map(|v| v.to_str().unwrap().to_string());

        assert_eq!(cache_header1.as_deref(), Some("MISS"));
        // Note: due to timing, the second request may be MISS if cache insert
        // hasn't completed — but with moka's async insert + await above, HIT is expected.
        assert_eq!(cache_header2.as_deref(), Some("HIT"));
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
            guide_url: Some("https://netray.info/guide/hsts"),
            fix_hint: None,
            fix_owner: None,
            weight: Some(10),
            messages: vec!["max-age too short".to_string()],
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
            guide_url: Some("https://netray.info/guide/hsts"),
            fix_hint: Some("HSTS max-age should be at least 1 year."),
            fix_owner: Some("Your web server config"),
            weight: Some(10),
            messages: vec![],
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["fix_hint"], "HSTS max-age should be at least 1 year.");
        assert_eq!(json["fix_owner"], "Your web server config");
    }

    #[test]
    fn fix_for_returns_empty_for_all_known_check_names() {
        // Phase 1 contract: mechanism plumbed, content empty.
        // Phase 4 will fill entries; this test then becomes obsolete and is removed.
        for name in [
            "hsts",
            "dnssec",
            "tls_version",
            "spf_align",
            "unknown_check",
        ] {
            assert_eq!(fix_for(name), (None, None), "fix_for({name}) must be empty");
        }
    }

    // --- SDD product-repositioning §3 Requirement 8: /api/meta exposes `site` ---

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

        let site = &json["features"]["site"];
        assert!(site.is_object(), "features.site must be an object");
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
            assert!(
                site.get(key).is_some(),
                "features.site must contain field {key}"
            );
        }
        assert_eq!(site["brand_name"], "lens");
        assert_eq!(
            site["example_domains"],
            serde_json::json!(["example.com", "github.com", "cloudflare.com"])
        );
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
        use crate::scoring::engine::OverallScore;

        let score = OverallScore {
            sections: HashMap::new(),
            overall_percentage: 0.0,
            grade: "F".to_string(),
            hard_fail_triggered: true,
            hard_fail_checks: vec!["chain_trusted".to_string(), "cert_lifetime".to_string()],
            not_applicable: HashMap::new(),
        };
        let sections = HashMap::new();
        let thresholds = BTreeMap::new();

        let summary = summary_payload_from(&sections, &score, &thresholds);

        assert!(summary.hard_fail);
        assert_eq!(
            summary.hard_fail_reason,
            Some("chain_trusted, cert_lifetime".to_string())
        );
    }

    // --- AC-6: hard_fail_reason is null when hard_fail is false

    #[test]
    fn hard_fail_reason_null_when_no_hard_fail() {
        use crate::scoring::engine::OverallScore;

        let score = OverallScore {
            sections: HashMap::new(),
            overall_percentage: 91.5,
            grade: "A".to_string(),
            hard_fail_triggered: false,
            hard_fail_checks: vec![],
            not_applicable: HashMap::new(),
        };
        let sections = HashMap::new();
        let thresholds = BTreeMap::new();

        let summary = summary_payload_from(&sections, &score, &thresholds);

        assert!(!summary.hard_fail);
        assert_eq!(summary.hard_fail_reason, None);
    }

    // --- AC-7: hard_fail_reason present in SSE summary event JSON

    #[test]
    fn hard_fail_reason_in_sse_summary_payload() {
        use crate::scoring::engine::OverallScore;

        let score = OverallScore {
            sections: HashMap::new(),
            overall_percentage: 0.0,
            grade: "F".to_string(),
            hard_fail_triggered: true,
            hard_fail_checks: vec!["chain_trusted".to_string()],
            not_applicable: HashMap::new(),
        };
        let sections = HashMap::new();
        let thresholds = BTreeMap::new();

        // summary_payload_from is used by summary_event_from — test its output directly.
        let payload = summary_payload_from(&sections, &score, &thresholds);
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
        assert_eq!(cache2.as_deref(), Some("HIT"));
        assert!(
            ct.contains("application/json"),
            "cache hit in sync mode must return JSON"
        );
    }

    // --- AC-11: /api-docs/openapi.json lists all five endpoints

    #[tokio::test]
    async fn openapi_spec_lists_all_endpoints() {
        let (_, health_openapi) = health_router().split_for_parts();
        let (_, api_openapi) = api_router().split_for_parts();
        let openapi = crate::api_doc::build_openapi(health_openapi, api_openapi);

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
        let openapi = crate::api_doc::build_openapi(health_openapi, api_openapi);

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
            ip_base_url: Some("https://ip.example.com".to_string()),
            ..Default::default()
        };
        // Set backend to internal URL
        config.backends.ip = netray_common::backend::BackendConfig {
            url: Some("http://127.0.0.1:19997".to_string()),
            timeout_ms: 1000,
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
            json["ecosystem"]["ip_base_url"], "https://ip.example.com",
            "ecosystem should use public URL"
        );

        // The backend config should use the internal URL
        assert_eq!(
            state.config.backends.ip.url.as_deref(),
            Some("http://127.0.0.1:19997"),
            "backend should use internal URL"
        );
    }

    // T10: Lens IP backend uses /network/json path
    #[tokio::test]
    async fn ip_backend_uses_network_json_path() {
        use crate::backends::ip::check_ip;
        use std::sync::Arc;

        let received_path = Arc::new(tokio::sync::Mutex::new(String::new()));
        let path_ref = received_path.clone();

        let app = axum::Router::new().route(
            "/network/json",
            axum::routing::get(
                move |axum::extract::Query(params): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| {
                    let path_ref = path_ref.clone();
                    async move {
                        let ip = params.get("ip").cloned().unwrap_or_default();
                        *path_ref.lock().await = format!("/network/json?ip={ip}");
                        axum::Json(serde_json::json!({
                            "network": { "type": "cloud", "org": "Example Corp" },
                            "location": { "city": "Berlin", "country": "Germany" }
                        }))
                    }
                },
            ),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        let client = reqwest::Client::new();
        let ip: std::net::IpAddr = "1.2.3.4".parse().unwrap();
        let result = check_ip(
            &client,
            &format!("http://{addr}"),
            &[ip],
            std::time::Duration::from_secs(5),
        )
        .await;

        assert!(result.is_ok(), "check_ip should succeed");
        let path = received_path.lock().await;
        assert_eq!(
            *path, "/network/json?ip=1.2.3.4",
            "should call /network/json?ip=<addr>"
        );
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
}
