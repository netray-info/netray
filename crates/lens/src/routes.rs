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

use crate::backends::dns::DnsBackendResult;
use crate::backends::ip::IpBackendResult;
use crate::backends::tls::TlsBackendResult;
use crate::cache::{CachedResult, cache_key, is_fresh};
use crate::check::{CheckOutput, SectionError, run_check};
use crate::input::validate_domain;
use crate::scoring::engine::{CheckVerdict, OverallScore};
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
}

/// Return the guide URL for a check name if the verdict is fail or warn.
/// Mapping defined in SDD §4.12.
fn guide_url_for(name: &str, verdict: &str) -> Option<&'static str> {
    if verdict != "fail" && verdict != "warn" {
        return None;
    }
    match name {
        "spf" | "dmarc" | "dkim" | "mta_sts" | "tlsrpt" => {
            Some("https://netray.info/guide/email-auth")
        }
        "dnssec" | "dnskey_algorithm" | "dnssec_rollover" => {
            Some("https://netray.info/guide/record-types")
        }
        "dane_valid" | "caa_compliant" => Some("https://netray.info/guide/dane-tlsa"),
        "chain_trusted" | "chain_complete" | "cert_lifetime" | "strong_signature"
        | "hsts" | "https_redirect" | "tls_version" => {
            Some("https://netray.info/guide/certificate-chain")
        }
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
    pub dns: &'static str,
    pub tls: &'static str,
    pub ip: &'static str,
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
    pub section_weights: ProfileSectionWeights,
    pub thresholds: BTreeMap<String, u32>,
    pub hard_fail: ProfileHardFail,
}

#[derive(Serialize)]
pub struct ProfileSectionWeights {
    pub dns: u32,
    pub tls: u32,
    pub ip: u32,
}

#[derive(Serialize)]
pub struct ProfileHardFail {
    pub dns: Vec<String>,
    pub tls: Vec<String>,
    pub ip: Vec<String>,
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

pub fn health_router() -> Router {
    Router::new().route("/api/health", get(health_handler))
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/api/meta", get(meta_handler))
        .route("/api/check/{domain}", get(check_get_handler))
        .route("/api/check", post(check_post_handler))
        .route("/api/ready", get(ready_handler))
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
    all_checks.extend(profile.dns.iter().map(|(k, v)| (k.clone(), *v)));
    all_checks.extend(profile.tls.iter().map(|(k, v)| (k.clone(), *v)));
    all_checks.extend(profile.ip.iter().map(|(k, v)| (k.clone(), *v)));
    let profile_data = ProfileData {
        name: profile.meta.name.clone(),
        version: profile.meta.version,
        checks: all_checks,
        section_weights: ProfileSectionWeights {
            dns: profile.section_weights.dns,
            tls: profile.section_weights.tls,
            ip: profile.section_weights.ip,
        },
        thresholds: profile.thresholds.clone(),
        hard_fail: ProfileHardFail {
            dns: profile.hard_fail.dns.clone(),
            tls: profile.hard_fail.tls.clone(),
            ip: profile.hard_fail.ip.clone(),
        },
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

    // Check each backend with a short ping (HEAD / GET to /).
    for (name, url) in [
        ("dns", config.backends.dns_url.as_str()),
        ("tls", config.backends.tls_url.as_str()),
        ("ip", config.backends.ip_url.as_str()),
    ] {
        if url.is_empty() {
            down.push(name.to_string());
            continue;
        }
        let health_url = format!("{}/api/health", url.trim_end_matches('/'));
        let ok = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            client.get(&health_url).send(),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .map(|r| r.status().is_success())
        .unwrap_or(false);

        if !ok {
            down.push(name.to_string());
        }
    }

    if down.is_empty() {
        (StatusCode::OK, Json(ReadyResponse { status: "ok", down: vec![] })).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadyResponse { status: "degraded", down }),
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
    let client_ip = extract_client_ip_from_peer(&headers, &state.config.server.trusted_proxies, peer.ip());
    run_check_handler(state, client_ip, domain).await
}

pub async fn check_post_handler(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(body): Json<CheckPostBody>,
) -> Response {
    let client_ip = extract_client_ip_from_peer(&headers, &state.config.server.trusted_proxies, peer.ip());
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
    if let Some(cache) = &state.cache {
        if let Some(cached) = cache.get(&key).await {
            if is_fresh(&cached, state.config.cache.ttl_seconds) {
                return sse_response_from_cached(domain, &cached, true, &state.scoring_profile);
            }
        }
    }

    // 4. Run check.
    let output = run_check(&state, &domain).await;
    let duration_ms = output.duration_ms;
    let domain_out = output.domain.clone();

    // 5. Store in cache.
    if let Some(cache) = &state.cache {
        let entry = Arc::new(CachedResult {
            dns: output.dns.clone(),
            tls: output.tls.clone(),
            ip: output.ip.clone(),
            score: clone_score(&output.score),
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
        CheckVerdict::Skip => "pass",
    }
}

fn section_status_from_checks<T>(
    result: &Result<T, SectionError>,
    checks_fn: impl Fn(&T) -> &[crate::scoring::engine::CheckResult],
) -> &'static str {
    match result {
        Err(_) => "error",
        Ok(r) => {
            let checks = checks_fn(r);
            let mut has_warn = false;
            for c in checks {
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

fn dns_event_from(result: &Result<DnsBackendResult, SectionError>, weights: &HashMap<String, u32>) -> Event {
    let status = section_status_from_checks(result, |r| &r.checks);
    let (headline, checks, detail_url) = match result {
        Ok(r) => {
            let items = r
                .checks
                .iter()
                .map(|c| {
                    let verdict = verdict_str(&c.verdict);
                    CheckItem {
                        guide_url: guide_url_for(&c.name, verdict),
                        name: c.name.clone(),
                        verdict,
                        weight: weights.get(&c.name).copied(),
                    }
                })
                .collect();
            (r.raw_headline.clone(), items, r.detail_url.clone())
        }
        Err(e) => {
            let msg = match e {
                SectionError::Timeout => "timeout",
                SectionError::BackendError(_) => "backend error",
            };
            (msg.to_string(), vec![], String::new())
        }
    };
    let payload = DnsEvent { status, headline, checks, detail_url };
    Event::default()
        .event("dns")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn tls_event_from(result: &Result<TlsBackendResult, SectionError>, weights: &HashMap<String, u32>) -> Event {
    let status = section_status_from_checks(result, |r| &r.checks);
    let (headline, checks, detail_url) = match result {
        Ok(r) => {
            let items = r
                .checks
                .iter()
                .map(|c| {
                    let verdict = verdict_str(&c.verdict);
                    CheckItem {
                        guide_url: guide_url_for(&c.name, verdict),
                        name: c.name.clone(),
                        verdict,
                        weight: weights.get(&c.name).copied(),
                    }
                })
                .collect();
            (r.raw_headline.clone(), items, r.detail_url.clone())
        }
        Err(e) => {
            let msg = match e {
                SectionError::Timeout => "timeout",
                SectionError::BackendError(_) => "backend error",
            };
            (msg.to_string(), vec![], String::new())
        }
    };
    let payload = TlsEvent { status, headline, checks, detail_url };
    Event::default()
        .event("tls")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn ip_event_from(result: &Result<IpBackendResult, SectionError>, weights: &HashMap<String, u32>) -> Event {
    let status = section_status_from_checks(result, |r| &r.checks);
    let (headline, checks, addresses, detail_url) = match result {
        Ok(r) => {
            let items = r
                .checks
                .iter()
                .map(|c| {
                    let verdict = verdict_str(&c.verdict);
                    CheckItem {
                        guide_url: guide_url_for(&c.name, verdict),
                        name: c.name.clone(),
                        verdict,
                        weight: weights.get(&c.name).copied(),
                    }
                })
                .collect();
            let addrs = r
                .addresses
                .iter()
                .map(|a| IpAddressInfo {
                    ip: a.ip.to_string(),
                    org: a.org.clone(),
                    geo: a.geo.clone(),
                    network_type: a.network_type.clone(),
                })
                .collect();
            (r.raw_headline.clone(), items, addrs, r.detail_url.clone())
        }
        Err(e) => {
            let msg = match e {
                SectionError::Timeout => "timeout",
                SectionError::BackendError(_) => "backend error",
            };
            (msg.to_string(), vec![], vec![], String::new())
        }
    };
    let guide_url = if status == "fail" || status == "warn" {
        Some("https://netray.info/guide/ip-enrichment")
    } else {
        None
    };
    let payload = IpEvent { status, headline, checks, addresses, detail_url, guide_url };
    Event::default()
        .event("ip")
        .data(serde_json::to_string(&payload).unwrap_or_default())
}

fn summary_event_from(
    dns_result: &Result<DnsBackendResult, SectionError>,
    tls_result: &Result<TlsBackendResult, SectionError>,
    ip_result: &Result<IpBackendResult, SectionError>,
    score: &OverallScore,
) -> Event {
    let dns_status = section_status_from_checks(dns_result, |r| &r.checks);
    let tls_status = section_status_from_checks(tls_result, |r| &r.checks);
    let ip_status = section_status_from_checks(ip_result, |r| &r.checks);

    // Overall status: if any section is error → "error", else roll up worst
    let overall = if dns_status == "error" || tls_status == "error" || ip_status == "error" {
        "error"
    } else if dns_status == "fail" || tls_status == "fail" || ip_status == "fail" {
        "fail"
    } else if dns_status == "warn" || tls_status == "warn" || ip_status == "warn" {
        "warn"
    } else {
        "pass"
    };

    let payload = SummaryEvent {
        dns: dns_status,
        tls: tls_status,
        ip: ip_status,
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

fn build_sse_events(domain: String, output: CheckOutput, cached: bool, profile: &crate::scoring::profile::ScoringProfile) -> Vec<Event> {
    let duration_ms = output.duration_ms;
    vec![
        dns_event_from(&output.dns, &profile.dns),
        tls_event_from(&output.tls, &profile.tls),
        ip_event_from(&output.ip, &profile.ip),
        summary_event_from(&output.dns, &output.tls, &output.ip, &output.score),
        done_event(&domain, duration_ms, cached),
    ]
}

fn build_sse_events_from_cached(domain: &str, cached: &CachedResult, profile: &crate::scoring::profile::ScoringProfile) -> Vec<Event> {
    let dummy_output = CheckOutput {
        domain: domain.to_string(),
        dns: cached.dns.clone(),
        tls: cached.tls.clone(),
        ip: cached.ip.clone(),
        score: clone_score(&cached.score),
        duration_ms: cached.duration_ms,
    };
    build_sse_events(domain.to_string(), dummy_output, true, profile)
}

fn make_sse_stream(events: Vec<Event>, cache_header: &'static str) -> Response {
    let s = stream::iter(events.into_iter().map(Ok::<_, Infallible>));
    let sse = Sse::new(s).keep_alive(KeepAlive::default());
    let mut response = sse.into_response();
    response.headers_mut().insert(
        "x-cache",
        HeaderValue::from_static(cache_header),
    );
    response
}

fn sse_response_from_cached(domain: String, cached: &CachedResult, is_cached: bool, profile: &crate::scoring::profile::ScoringProfile) -> Response {
    let events = build_sse_events_from_cached(&domain, cached, profile);
    make_sse_stream(events, if is_cached { "HIT" } else { "MISS" })
}

/// Clone an OverallScore (which doesn't derive Clone due to non-Clone fields — all fields
/// are primitives or String/Vec, so this is straightforward).
fn clone_score(s: &OverallScore) -> OverallScore {
    OverallScore {
        dns: s.dns.clone(),
        tls: s.tls.clone(),
        ip: s.ip.clone(),
        overall_percentage: s.overall_percentage,
        grade: s.grade.clone(),
        hard_fail_triggered: s.hard_fail_triggered,
        hard_fail_checks: s.hard_fail_checks.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::config::{
        BackendsConfig, CacheConfig, RateLimitConfig, ScoringConfig, ServerConfig,
    };
    use crate::config::Config;

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
            .route("/api/health", get(health_handler))
            .route("/api/check/{domain}", get(check_get_no_connect_handler))
            .route("/api/check", post(check_post_no_connect_handler))
            .route("/api/ready", get(ready_handler))
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
            .uri("/api/health")
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
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/event-stream"), "expected SSE content-type, got: {ct}");
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
    }
}
