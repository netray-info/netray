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
use crate::check::{CheckOutput, SectionError, run_check};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
}

/// Return the guide URL for a check name.
fn guide_url_for(name: &str) -> Option<&'static str> {
    match name {
        // DNS — email authentication
        "spf" | "dmarc" | "dkim" | "tlsrpt" | "bimi" => {
            Some("https://netray.info/guide/email-auth")
        }
        // DNS — DNSSEC
        "dnssec" | "dnskey_algorithm" | "dnssec_rollover" => {
            Some("https://netray.info/guide/dnssec")
        }
        // DNS — record types & infrastructure
        "cname_apex" | "https_svcb" | "mx" | "ns" | "ttl" => {
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
        "mta_sts" => Some("https://netray.info/guide/mta-sts"),
        // IP
        "reputation" => Some("https://netray.info/guide/ip-enrichment"),
        _ => None,
    }
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
}

/// Query parameters for `GET /api/check/{domain}`.
#[derive(Deserialize, IntoParams)]
pub struct CheckGetQuery {
    /// When `false`, triggers synchronous (non-SSE) response mode.
    pub stream: Option<bool>,
}

// ---------------------------------------------------------------------------
// Meta response types
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct RateLimitInfo {
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub global_per_minute: u32,
    pub global_burst: u32,
}

#[derive(Serialize, ToSchema)]
pub struct ProfileData {
    pub name: String,
    pub version: u32,
    pub checks: HashMap<String, u32>,
    pub section_weights: HashMap<String, u32>,
    pub thresholds: BTreeMap<String, u32>,
    pub hard_fail: HashMap<String, Vec<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct MetaResponse {
    pub site_name: String,
    #[schema(value_type = String)]
    pub version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<MetaEcosystem>,
    pub profile: ProfileData,
    pub rate_limit: RateLimitInfo,
}

#[derive(Serialize, ToSchema)]
pub struct MetaEcosystem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_base_url: Option<String>,
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
// SPA embedded assets
// ---------------------------------------------------------------------------

#[derive(rust_embed::RustEmbed)]
#[folder = "frontend/dist"]
pub struct Assets;

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
        (status = 200, description = "Service metadata and scoring profile", body = MetaResponse)
    )
)]
pub async fn meta_handler(State(state): State<AppState>) -> impl IntoResponse {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    let backends = &state.config.backends;
    let ecosystem = Some(MetaEcosystem {
        dns_base_url: Some(backends.dns_url.clone()),
        tls_base_url: Some(backends.tls_url.clone()),
        ip_base_url: Some(backends.ip_url.clone()),
        http_base_url: backends.http_url.clone(),
    });
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
    let rl = &state.config.rate_limit;
    Json(MetaResponse {
        site_name: "lens — Domain Health Check".to_string(),
        version: VERSION,
        ecosystem,
        profile: profile_data,
        rate_limit: RateLimitInfo {
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
    // Optional http backend is only probed when http_url is configured.
    let mut probes: Vec<(String, String)> = vec![
        ("dns".to_string(), config.backends.dns_url.clone()),
        ("tls".to_string(), config.backends.tls_url.clone()),
        ("ip".to_string(), config.backends.ip_url.clone()),
    ];
    if let Some(ref http_url) = config.backends.http_url {
        probes.push(("http".to_string(), http_url.clone()));
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
    run_check_handler(state, client_ip, domain, sync).await
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
    run_check_handler(state, client_ip, body.domain, sync).await
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

    // 3. Cache lookup.
    let key = cache_key(&domain);
    if let Some(cache) = &state.cache
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
    let output = run_check(&state, &domain).await;
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
            CheckItem {
                guide_url: guide_url_for(&c.name),
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
                _ => (String::new(), String::new(), None, None, None, None, None, None),
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
    let mut events = vec![
        dns_event_from(output.sections.get("dns").unwrap_or(&empty_err), dns_checks),
        tls_event_from(output.sections.get("tls").unwrap_or(&empty_err), tls_checks),
    ];
    if let Some(http_result) = output.sections.get("http") {
        events.push(http_event_from(http_result, http_checks));
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
    let duration_ms = output.duration_ms;
    let response = SyncCheckResponse {
        dns: dns_payload_from(output.sections.get("dns").unwrap_or(&empty_err), dns_checks),
        tls: tls_payload_from(output.sections.get("tls").unwrap_or(&empty_err), tls_checks),
        http: output
            .sections
            .get("http")
            .map(|r| http_payload_from(r, http_checks)),
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
        BackendsConfig, CacheConfig, RateLimitConfig, ScoringConfig, ServerConfig,
    };

    pub fn test_config_with_rate_limit(per_ip: u32, burst: u32) -> Config {
        Config {
            server: ServerConfig {
                bind: ([127, 0, 0, 1], 0).into(),
                metrics_bind: ([127, 0, 0, 1], 0).into(),
                trusted_proxies: Vec::new(),
            },
            backends: BackendsConfig {
                dns_url: "http://127.0.0.1:19999".to_string(),
                tls_url: "http://127.0.0.1:19998".to_string(),
                ip_url: "http://127.0.0.1:19997".to_string(),
                http_url: None,
                backend_timeout_secs: 1,
            },
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
        run_check_handler(state, client_ip, domain, false).await
    }

    /// Simplified POST handler for tests — uses loopback as client IP, no sync mode.
    async fn check_post_no_connect_handler(
        State(state): State<AppState>,
        Json(body): Json<CheckPostBody>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        run_check_handler(state, client_ip, body.domain, false).await
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
        run_check_handler(state, client_ip, domain, sync).await
    }

    /// POST handler for sync-mode tests — computes sync flag from headers and body.
    async fn check_post_no_connect_with_sync(
        State(state): State<AppState>,
        headers: axum::http::HeaderMap,
        Json(body): Json<CheckPostBody>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let sync = is_sync_mode(&headers, body.stream);
        run_check_handler(state, client_ip, body.domain, sync).await
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
        assert!(json["ecosystem"].is_object());

        // Profile section_weights and hard_fail are string-keyed maps
        let profile = &json["profile"];
        assert!(profile.is_object(), "profile must be an object");
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
}
