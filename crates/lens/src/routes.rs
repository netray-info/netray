use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::SystemTime;

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream;
use serde::{Deserialize, Serialize};

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

#[derive(Serialize)]
pub struct CheckItem {
    pub name: String,
    pub verdict: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        "dane_valid" | "caa_compliant" => Some("https://netray.info/guide/dane-tlsa"),
        "mta_sts" => Some("https://netray.info/guide/mta-sts"),
        // IP
        "reputation" => Some("https://netray.info/guide/ip-enrichment"),
        _ => None,
    }
}

#[derive(Serialize)]
pub struct IpAddressInfo {
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<String>,
    pub network_type: String,
}

#[derive(Serialize)]
pub struct DnsEvent {
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub detail_url: String,
}

#[derive(Serialize)]
pub struct TlsEvent {
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub detail_url: String,
}

#[derive(Serialize)]
pub struct IpEvent {
    pub status: &'static str,
    pub headline: String,
    pub checks: Vec<CheckItem>,
    pub addresses: Vec<IpAddressInfo>,
    pub detail_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guide_url: Option<&'static str>,
}

#[derive(Serialize)]
pub struct SummaryEvent {
    pub sections: HashMap<String, &'static str>,
    pub section_grades: HashMap<String, String>,
    pub overall: &'static str,
    pub grade: String,
    pub score: f64,
    pub hard_fail: bool,
    pub hard_fail_checks: Vec<String>,
}

#[derive(Serialize)]
pub struct DoneEvent {
    pub domain: String,
    pub duration_ms: u64,
    pub cached: bool,
}

// ---------------------------------------------------------------------------
// POST body
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CheckPostBody {
    pub domain: String,
}

// ---------------------------------------------------------------------------
// Meta response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ProfileData {
    pub name: String,
    pub version: u32,
    pub checks: HashMap<String, u32>,
    pub section_weights: HashMap<String, u32>,
    pub thresholds: BTreeMap<String, u32>,
    pub hard_fail: HashMap<String, Vec<String>>,
}

#[derive(Serialize)]
pub struct MetaResponse {
    pub site_name: String,
    pub version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<MetaEcosystem>,
    pub profile: ProfileData,
}

#[derive(Serialize)]
pub struct MetaEcosystem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_base_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Health / Ready response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Serialize)]
pub struct ReadyResponse {
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

pub fn health_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .with_state(state)
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/meta", get(meta_handler))
        .route("/api/check/{domain}", get(check_get_handler))
        .route("/api/check", post(check_post_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn health_handler() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}

pub async fn meta_handler(State(state): State<AppState>) -> impl IntoResponse {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    let backends = &state.config.backends;
    let ecosystem = Some(MetaEcosystem {
        dns_base_url: Some(backends.dns_url.clone()),
        tls_base_url: Some(backends.tls_url.clone()),
        ip_base_url: Some(backends.ip_url.clone()),
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
    Json(MetaResponse {
        site_name: "lens — Domain Health Check".to_string(),
        version: VERSION,
        ecosystem,
        profile: profile_data,
    })
}

pub async fn ready_handler(State(state): State<AppState>) -> impl IntoResponse {
    let client = &state.http_client;
    let config = &state.config;
    let mut down: Vec<String> = Vec::new();

    // Probe all backends concurrently (3 s timeout each).
    let backends = [
        ("dns", config.backends.dns_url.as_str()),
        ("tls", config.backends.tls_url.as_str()),
        ("ip", config.backends.ip_url.as_str()),
    ];
    let checks = backends.map(|(name, url)| {
        let client = client.clone();
        async move {
            if url.is_empty() {
                return Some(name.to_string());
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
            if !ok { Some(name.to_string()) } else { None }
        }
    });
    let [r0, r1, r2] = checks;
    let (r0, r1, r2) = tokio::join!(r0, r1, r2);
    for name in [r0, r1, r2].into_iter().flatten() {
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

pub async fn check_get_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path(domain): Path<String>,
) -> Response {
    let client_ip =
        extract_client_ip_from_peer(&headers, &state.config.server.trusted_proxies, peer.ip());
    run_check_handler(state, client_ip, domain).await
}

pub async fn check_post_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(body): Json<CheckPostBody>,
) -> Response {
    let client_ip =
        extract_client_ip_from_peer(&headers, &state.config.server.trusted_proxies, peer.ip());
    run_check_handler(state, client_ip, body.domain).await
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

async fn run_check_handler(state: AppState, client_ip: IpAddr, domain_raw: String) -> Response {
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
        return sse_response_from_cached(domain, &cached, true, &state.scoring_profile);
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

    // 6. Stream SSE from output.
    let events = build_sse_events(domain_out, output, false, &state.scoring_profile);
    make_sse_stream(events, "MISS")
}

// ---------------------------------------------------------------------------
// SSE helpers
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

fn build_check_items(
    checks: &[CheckResult],
    weights: &HashMap<String, u32>,
) -> Vec<CheckItem> {
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

fn dns_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
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
    let payload = DnsEvent {
        status,
        headline,
        checks,
        detail_url,
    };
    Event::default()
        .event("dns")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn tls_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
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
    let payload = TlsEvent {
        status,
        headline,
        checks,
        detail_url,
    };
    Event::default()
        .event("tls")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn ip_event_from(
    result: &Result<BackendResult, SectionError>,
    weights: &HashMap<String, u32>,
) -> Event {
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
    let payload = IpEvent {
        status,
        headline,
        checks,
        addresses,
        detail_url,
        guide_url,
    };
    Event::default()
        .event("ip")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn summary_event_from(
    sections: &HashMap<String, Result<BackendResult, SectionError>>,
    score: &OverallScore,
    thresholds: &std::collections::BTreeMap<String, u32>,
) -> Event {
    use crate::scoring::engine::lookup_grade;

    // Build section status and grade maps.
    let mut section_statuses: HashMap<String, &'static str> = HashMap::new();
    let mut section_grades: HashMap<String, String> = HashMap::new();

    for (name, result) in sections {
        section_statuses.insert(name.clone(), section_status_from_checks(result));
        if let Some(s) = score.sections.get(name) {
            section_grades.insert(name.clone(), lookup_grade(thresholds, s.percentage));
        }
    }

    // Overall status: if any section is error → "error", else roll up worst.
    let overall = if section_statuses.values().any(|s| *s == "error") {
        "error"
    } else if section_statuses.values().any(|s| *s == "fail") {
        "fail"
    } else if section_statuses.values().any(|s| *s == "warn") {
        "warn"
    } else {
        "pass"
    };

    let payload = SummaryEvent {
        sections: section_statuses,
        section_grades,
        overall,
        grade: score.grade.clone(),
        score: (score.overall_percentage * 10.0).round() / 10.0,
        hard_fail: score.hard_fail_triggered,
        hard_fail_checks: score.hard_fail_checks.clone(),
    };
    Event::default()
        .event("summary")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn done_event(domain: &str, duration_ms: u64, cached: bool) -> Event {
    let payload = DoneEvent {
        domain: domain.to_string(),
        duration_ms,
        cached,
    };
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
    let ip_checks = profile
        .sections
        .get("ip")
        .map(|s| &s.checks)
        .unwrap_or(&empty_checks);
    vec![
        dns_event_from(output.sections.get("dns").unwrap_or(&empty_err), dns_checks),
        tls_event_from(output.sections.get("tls").unwrap_or(&empty_err), tls_checks),
        ip_event_from(output.sections.get("ip").unwrap_or(&empty_err), ip_checks),
        summary_event_from(&output.sections, &output.score, &profile.thresholds),
        done_event(&domain, duration_ms, cached),
    ]
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

/// Clone an OverallScore (which doesn't derive Clone due to non-Clone fields — all fields
/// are primitives or String/Vec, so this is straightforward).
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
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

    /// Simplified GET handler for tests — uses loopback as client IP.
    async fn check_get_no_connect_handler(
        State(state): State<AppState>,
        Path(domain): Path<String>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        run_check_handler(state, client_ip, domain).await
    }

    /// Simplified POST handler for tests — uses loopback as client IP.
    async fn check_post_no_connect_handler(
        State(state): State<AppState>,
        Json(body): Json<CheckPostBody>,
    ) -> Response {
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        run_check_handler(state, client_ip, body.domain).await
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
    }
}
