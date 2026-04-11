use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Extension, Query, RawQuery, State};
use axum::http::HeaderValue;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use utoipa::OpenApi;

use crate::error::{AppError, ErrorResponse};
use crate::input::{self, Target};
use crate::security::rate_limit::select_representative_ips;
use crate::security::target_policy;
use crate::state::AppState;
use crate::tls;
use crate::validate;
use netray_common::enrichment::{CloudInfo, IpInfo};

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct InspectResponse {
    pub request_id: String,
    pub hostname: String,
    pub input_mode: &'static str,
    pub summary: validate::Summary,
    pub ports: Vec<PortResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<crate::quality::QualityResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_ips: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PortResult {
    pub port: u16,
    pub ips: Vec<tls::IpInspectionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consistency: Option<ConsistencyResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<validate::ValidationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tlsa: Option<TlsaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<crate::quality::PortQualityResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<tls::InspectionError>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DnsContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caa: Option<CaaInfo>,
    pub resolved_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaaInfo {
    pub records: Vec<String>,
    pub issuer_allowed: Option<bool>,
    pub issuewild_present: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TlsaInfo {
    pub records: Vec<String>,
    pub dnssec_signed: bool,
    pub dane_valid: Option<bool>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ConsistencyResult {
    pub certificates_match: bool,
    pub tls_versions_match: bool,
    pub cipher_suites_match: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<ConsistencyMismatch>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ConsistencyMismatch {
    pub field: String,
    /// Per-IP values, keyed by IP address string.
    pub values: HashMap<String, String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MetaResponse {
    pub site_name: String,
    pub version: &'static str,
    pub features: MetaFeatures,
    pub limits: MetaLimits,
    pub custom_ca_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<netray_common::ecosystem::EcosystemConfig>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MetaFeatures {
    pub dane: bool,
    pub caa: bool,
    pub ct: bool,
    pub multi_port: bool,
    pub multi_ip: bool,
    pub ip_enrichment: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MetaLimits {
    pub max_ports: usize,
    pub max_ips_per_hostname: usize,
    pub handshake_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    pub h: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InspectPostBody {
    pub hostname: String,
    #[serde(default = "default_ports")]
    pub ports: Vec<u16>,
}

fn default_ports() -> Vec<u16> {
    vec![443]
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(
        title = "tlsight",
        description = "TLS certificate inspection and diagnostics API\n\n\
            ## Cross-Origin Requests\n\
            Cross-origin requests from browsers are not supported. Use server-side calls or curl for API integration.\n\n\
            ## Human-Readable Docs\n\
            See also: [TLS API reference](https://netray.info/api/tls) — curl-focused documentation with examples."
    ),
    paths(
        health_handler,
        ready_handler,
        inspect_handler,
        inspect_post_handler,
        meta_handler,
    ),
    components(schemas(
        InspectResponse,
        PortResult,
        DnsContext,
        CaaInfo,
        TlsaInfo,
        ConsistencyResult,
        ConsistencyMismatch,
        MetaResponse,
        MetaFeatures,
        MetaLimits,
        netray_common::ecosystem::EcosystemConfig,
        HealthResponse,
        ReadyResponse,
        ErrorResponse,
        crate::error::ErrorInfo,
        tls::IpInspectionResult,
        tls::InspectionError,
        tls::CertInfo,
        tls::TlsParams,
        tls::ocsp::OcspInfo,
        tls::ocsp::OcspRevocationResult,
        validate::CheckStatus,
        validate::Summary,
        validate::SummaryChecks,
        validate::ValidationResult,
        validate::ct::CtInfo,
        validate::ct::SctEntry,
        IpInfo,
        CloudInfo,
        crate::quality::QualityResult,
        crate::quality::PortQualityResult,
        crate::quality::HealthCheck,
        crate::quality::RedirectInfo,
        crate::quality::types::HstsInfo,
        crate::quality::types::Category,
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
        .route(
            "/api/inspect",
            get(inspect_handler).post(inspect_post_handler),
        )
        .route("/api/meta", get(meta_handler))
        .route("/api-docs/openapi.json", get(openapi_handler))
        .route("/docs", get(docs_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse),
    )
)]
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "Service is ready; warnings array lists any degraded conditions", body = ReadyResponse),
    )
)]
async fn ready_handler(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.load();
    let mut warnings: Vec<String> = Vec::new();

    // Check enrichment service reachability (2 s timeout HEAD request).
    if let Some(ref client) = state.enrichment_client
        && !client.is_reachable().await
    {
        warnings.push("enrichment service unreachable".to_owned());
    }

    // Check custom CA directory exists and is readable.
    if let Some(ref ca_dir) = config.validation.custom_ca_dir {
        let dir = std::path::Path::new(ca_dir);
        if !dir.is_dir() || std::fs::read_dir(dir).is_err() {
            warnings.push(format!("custom_ca_dir not accessible: {ca_dir}"));
        }
    }

    // Check that the trust store loaded at least the Mozilla root CAs.
    if state.trust_store.load().is_empty() {
        warnings.push("trust store contains no CA certificates".to_owned());
    }

    Json(ReadyResponse {
        status: "ok",
        warnings,
    })
}

#[utoipa::path(
    get,
    path = "/api/meta",
    responses(
        (status = 200, description = "Service metadata", body = MetaResponse),
    )
)]
async fn meta_handler(State(state): State<AppState>) -> Json<MetaResponse> {
    let config = state.config.load();
    let ecosystem = if config.ecosystem.has_any() {
        Some(config.ecosystem.clone())
    } else {
        None
    };

    Json(MetaResponse {
        site_name: config.site_name.clone(),
        version: env!("CARGO_PKG_VERSION"),
        features: MetaFeatures {
            dane: config.validation.check_dane,
            caa: config.validation.check_caa,
            ct: config.validation.check_ct,
            multi_port: true,
            multi_ip: true,
            ip_enrichment: state.enrichment_client.is_some(),
        },
        limits: MetaLimits {
            max_ports: config.limits.max_ports,
            max_ips_per_hostname: config.limits.max_ips_per_hostname,
            handshake_timeout_secs: config.limits.handshake_timeout_secs,
            request_timeout_secs: config.limits.request_timeout_secs,
        },
        custom_ca_count: state.custom_ca_count,
        ecosystem,
    })
}

#[utoipa::path(
    get,
    path = "/api/inspect",
    params(
        ("h" = String, Query, description = "Hostname or IP with optional ports, e.g. example.com:443,8443"),
    ),
    responses(
        (status = 200, description = "Inspection result", body = InspectResponse),
        (status = 400, description = "Invalid input", body = ErrorResponse),
        (status = 403, description = "Blocked target", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse),
        (status = 504, description = "Request timeout", body = ErrorResponse),
    )
)]
async fn inspect_handler(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(request_id): Extension<crate::RequestId>,
    headers: axum::http::HeaderMap,
    query: Query<InspectQuery>,
) -> Result<Response, AppError> {
    let client_ip = state.ip_extractor.extract(&headers, addr);
    let config = state.config.load();
    let parsed = match input::parse_input(&query.h, config.limits.max_ports) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(input = %query.h, error = %e, "input validation failed");
            return Err(e);
        }
    };
    do_inspect(state, client_ip, &headers, parsed, request_id.0).await
}

#[utoipa::path(
    post,
    path = "/api/inspect",
    request_body = InspectPostBody,
    responses(
        (status = 200, description = "Inspection result", body = InspectResponse),
        (status = 400, description = "Invalid input", body = ErrorResponse),
        (status = 403, description = "Blocked target", body = ErrorResponse),
        (status = 422, description = "Too many ports", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 502, description = "Upstream failure", body = ErrorResponse),
        (status = 504, description = "Request timeout", body = ErrorResponse),
    )
)]
async fn inspect_post_handler(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(request_id): Extension<crate::RequestId>,
    headers: axum::http::HeaderMap,
    raw_query: RawQuery,
    Json(body): Json<InspectPostBody>,
) -> Result<Response, AppError> {
    // Reject if query string contains h= param
    if let Some(ref qs) = raw_query.0
        && qs.contains("h=")
    {
        tracing::debug!(hostname = %body.hostname, "ambiguous input: POST with query string");
        return Err(AppError::AmbiguousInput(
            "POST body and ?h= query parameter cannot be used together".to_string(),
        ));
    }

    let config = state.config.load();

    // Validate ports
    if body.ports.is_empty() {
        tracing::debug!(hostname = %body.hostname, "input validation failed: empty ports");
        return Err(AppError::InvalidPort(
            "ports array must not be empty".to_string(),
        ));
    }
    if body.ports.len() > config.limits.max_ports {
        tracing::debug!(hostname = %body.hostname, port_count = body.ports.len(), "input validation failed: too many ports");
        return Err(AppError::TooManyPorts {
            requested: body.ports.len(),
            max: config.limits.max_ports,
        });
    }
    for &p in &body.ports {
        if p == 0 {
            tracing::debug!(hostname = %body.hostname, "input validation failed: port 0");
            return Err(AppError::InvalidPort("port must be 1-65535".to_string()));
        }
    }

    // Validate hostname through the same path as GET
    let target_parsed = match input::parse_input(&body.hostname, config.limits.max_ports) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(input = %body.hostname, error = %e, "input validation failed");
            return Err(e);
        }
    };
    let parsed = input::ParsedInput {
        target: target_parsed.target,
        ports: body.ports,
    };

    let client_ip = state.ip_extractor.extract(&headers, addr);
    do_inspect(state, client_ip, &headers, parsed, request_id.0).await
}

async fn do_inspect(
    state: AppState,
    client_ip: IpAddr,
    _headers: &axum::http::HeaderMap,
    parsed: input::ParsedInput,
    request_id: String,
) -> Result<Response, AppError> {
    let request_start = Instant::now();
    let config = state.config.load();

    let span = tracing::Span::current();
    span.record("request_id", &request_id);
    span.record("client_ip", tracing::field::display(&client_ip));
    metrics::counter!("tlsight_inspections_total").increment(1);

    let hostname_str = match &parsed.target {
        Target::Hostname(h) => h.clone(),
        Target::Ip(ip) => ip.to_string(),
    };
    let input_mode = match &parsed.target {
        Target::Hostname(_) => "hostname",
        Target::Ip(_) => "ip",
    };

    let mut warnings = Vec::new();
    let mut skipped_ips = Vec::new();

    tracing::info!(
        hostname = %hostname_str,
        client_ip = %client_ip,
        request_id = %request_id,
        "inspect start"
    );

    // Resolve IPs via mhost (same resolver used for CAA/TLSA — unified DNS path).
    // Resolved IPs are re-checked against target_policy below (DNS rebinding protection).
    let ips = match &parsed.target {
        Target::Hostname(h) => {
            let resolver = state.dns_resolver.as_ref().ok_or_else(|| {
                AppError::DnsResolutionFailed("DNS resolver not available".to_string())
            })?;
            resolve_hostname(h, resolver).await?
        }
        Target::Ip(ip) => vec![*ip],
    };

    // Filter blocked IPs
    let allow_blocked = config.limits.allow_blocked_targets;
    let allowed_ips: Vec<IpAddr> = ips
        .into_iter()
        .filter(
            |ip| match target_policy::check_allowed_with_policy(ip, allow_blocked) {
                Ok(()) => true,
                Err(reason) => {
                    warnings.push(format!("{ip}: blocked ({reason})"));
                    false
                }
            },
        )
        .collect();

    if allowed_ips.is_empty() {
        tracing::warn!(
            client_ip = %client_ip,
            target = %hostname_str,
            "blocked target: all resolved IPs in blocked ranges"
        );
        return Err(AppError::BlockedTarget(format!(
            "all resolved IPs for {hostname_str} are in blocked ranges"
        )));
    }

    // Cap-and-warn: compute cost and reduce IPs if over budget
    let full_cost = parsed.ports.len() as u32 * allowed_ips.len() as u32;
    let inspected_ips = if state
        .rate_limiter
        .check_cost(client_ip, &hostname_str, full_cost)
        .is_ok()
    {
        allowed_ips
    } else {
        // Try with reduced IPs
        let budget = state.rate_limiter.remaining_budget(client_ip);
        let ip_budget = (budget / parsed.ports.len() as u32).max(1) as usize;
        let (selected, skipped) = select_representative_ips(&allowed_ips, ip_budget);

        if selected.is_empty() {
            // Even 1 IP doesn't fit — hard reject
            tracing::warn!(
                client_ip = %client_ip,
                scope = "per_ip",
                "rate limited"
            );
            return Err(AppError::RateLimited {
                retry_after_secs: 60,
                scope: "per_ip",
            });
        }

        let reduced_cost = parsed.ports.len() as u32 * selected.len() as u32;
        if let Err(e) = state
            .rate_limiter
            .check_cost(client_ip, &hostname_str, reduced_cost)
        {
            if let AppError::RateLimited { scope, .. } = &e {
                tracing::warn!(
                    client_ip = %client_ip,
                    scope = *scope,
                    "rate limited"
                );
            }
            return Err(e);
        }

        if !skipped.is_empty() {
            warnings.push(format!(
                "rate limit: inspecting {} of {} IPs to stay within budget",
                selected.len(),
                selected.len() + skipped.len()
            ));
            skipped_ips = skipped.iter().map(|ip| ip.to_string()).collect();
        }

        selected
    };

    // IP input warning
    if input_mode == "ip" {
        warnings.push(
            "IP address input: no SNI sent, results may differ from hostname-based access"
                .to_string(),
        );
    }

    // Spawn enrichment (concurrent with TLS + DNS)
    let enrichment_handle = if let Some(ref client) = state.enrichment_client {
        let client = Arc::clone(client);
        let ips = inspected_ips.clone();
        let rid = request_id.clone();
        Some(tokio::spawn(async move {
            client.lookup_batch(&ips, Some(&rid)).await
        }))
    } else {
        None
    };

    let handshake_timeout = Duration::from_secs(config.limits.handshake_timeout_secs);
    let request_timeout = Duration::from_secs(config.limits.request_timeout_secs);

    let is_hostname = input_mode == "hostname";
    let do_dns = is_hostname && state.dns_resolver.is_some();

    // Spawn DNS lookups on a blocking thread (mhost uses rand::thread_rng which
    // is !Send in rand 0.9, so the DNS future can't be sent across threads via tokio::spawn;
    // TODO: remove spawn_blocking once mhost upgrades to a Send-compatible rand version)
    let dns_handle = if do_dns {
        let resolver = state.dns_resolver.clone().unwrap();
        let hostname = parsed.target.hostname().unwrap().to_string();
        let ports = parsed.ports.clone();
        let check_caa = config.validation.check_caa;
        let check_dane = config.validation.check_dane;
        let rt = tokio::runtime::Handle::current();

        Some(tokio::task::spawn_blocking(move || {
            rt.block_on(async move {
                let caa = if check_caa {
                    Some(resolver.lookup_caa(&hostname).await)
                } else {
                    None
                };

                let mut tlsa = HashMap::new();
                if check_dane {
                    for &port in &ports {
                        tlsa.insert(port, resolver.lookup_tlsa(&hostname, port).await);
                    }
                }

                let ech = resolver.lookup_ech_advertised(&hostname).await;

                (caa, tlsa, ech)
            })
        }))
    } else {
        None
    };

    // Run TLS handshakes with request timeout — ports run concurrently
    let port_results = tokio::time::timeout(request_timeout, async {
        let mut join_set = tokio::task::JoinSet::new();
        for &port in &parsed.ports {
            let ips = inspected_ips.clone();
            let hostname = parsed.target.hostname().map(|h| h.to_string());
            let verifier = state.cert_verifier.clone();
            let check_ct = config.validation.check_ct;
            let semaphore = Arc::clone(&state.handshake_semaphore);
            join_set.spawn(async move {
                inspect_port(
                    &ips,
                    port,
                    hostname.as_deref(),
                    handshake_timeout,
                    verifier,
                    check_ct,
                    semaphore,
                )
                .await
            });
        }
        let mut results = Vec::with_capacity(parsed.ports.len());
        while let Some(result) = join_set.join_next().await {
            results.push(result.unwrap());
        }
        results.sort_by_key(|r| r.port);
        results
    })
    .await
    .map_err(|_| AppError::RequestTimeout)?;

    // Collect DNS results
    let (caa_lookup, tlsa_lookups, ech_advertised) = if let Some(handle) = dns_handle {
        match handle.await {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(error = %e, "DNS task failed or was cancelled");
                (None, HashMap::new(), false)
            }
        }
    } else {
        (None, HashMap::new(), false)
    };

    // Collect enrichment results
    let enrichments = if let Some(handle) = enrichment_handle {
        match handle.await {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(error = %e, "enrichment task failed or was cancelled");
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    // Extract leaf issuer for CAA checking
    let leaf_issuer: Option<String> = port_results
        .iter()
        .flat_map(|pr| pr.ips.iter())
        .find(|r| r.chain.is_some())
        .and_then(|r| r.chain.as_ref()?.first())
        .map(|leaf| leaf.issuer.clone());

    // Compute CAA check status
    let caa_status = match (&caa_lookup, &leaf_issuer) {
        (Some(caa), Some(issuer)) => validate::caa_compliance::check_caa_compliance(caa, issuer),
        (Some(caa), None) if caa.is_empty() => validate::CheckStatus::Pass,
        (Some(_), None) => validate::CheckStatus::Skip, // Have records but no cert to check against
        (None, _) => validate::CheckStatus::Skip,
    };

    // Compute per-port DANE status by matching TLSA records against the presented cert chain.
    // DNSSEC signature verification is not yet available, so dnssec_signed is always false for now.
    // We perform structural DANE matching (RFC 6698) regardless; callers can check dnssec_signed.
    let mut port_dane_valid: HashMap<u16, Option<bool>> = HashMap::new();
    for pr in &port_results {
        if let Some(tlsa) = tlsa_lookups.get(&pr.port)
            && !tlsa.is_empty()
        {
            // Take raw certs from the first successful IP result for this port.
            let raw_certs_opt = pr
                .ips
                .iter()
                .find(|r| r.error.is_none())
                .and_then(|r| r.raw_certs.as_deref());

            let dane_ok = if let Some(raw_certs) = raw_certs_opt {
                tlsa.records
                    .iter()
                    .any(|record| validate::dane::dane_match(record, raw_certs))
            } else {
                false
            };
            port_dane_valid.insert(pr.port, Some(dane_ok));
        }
    }

    // Global DANE status: Pass if all ports with TLSA records matched; Skip if none had records.
    let dane_status = {
        let results: Vec<bool> = port_dane_valid.values().filter_map(|v| *v).collect();
        if results.is_empty() {
            validate::CheckStatus::Skip
        } else if results.iter().all(|&ok| ok) {
            validate::CheckStatus::Pass
        } else {
            validate::CheckStatus::Fail
        }
    };

    // Build DNS context for response
    let dns_context = if do_dns {
        let caa_info = caa_lookup.as_ref().map(|caa| {
            let issuer_allowed = leaf_issuer.as_ref().map(|issuer| {
                validate::caa_compliance::check_caa_compliance(caa, issuer)
                    == validate::CheckStatus::Pass
            });
            CaaInfo {
                records: caa
                    .records
                    .iter()
                    .map(|r| format!("{} \"{}\"", r.tag, r.value))
                    .collect(),
                issuer_allowed,
                issuewild_present: caa.issuewild_present(),
            }
        });

        Some(DnsContext {
            caa: caa_info,
            resolved_ips: inspected_ips.iter().map(|ip| ip.to_string()).collect(),
        })
    } else {
        None
    };

    // Inject enrichment data and ECH into IP results
    let mut port_results = port_results;
    let ech_for_port = if is_hostname && do_dns {
        Some(ech_advertised)
    } else {
        None
    };
    for pr in &mut port_results {
        for ip_result in &mut pr.ips {
            if let Ok(ip) = ip_result.ip.parse::<IpAddr>()
                && !enrichments.is_empty()
            {
                ip_result.enrichment = enrichments.get(&ip).cloned();
            }
            // Set ECH and STARTTLS on TLS params
            if let Some(ref mut tls) = ip_result.tls {
                tls.ech_advertised = ech_for_port;
                // STARTTLS: set based on port
                if tls.starttls.is_none() {
                    tls.starttls = starttls_protocol_for_port(pr.port);
                }
            }
        }
    }

    // Build per-port TLSA info
    for pr in &mut port_results {
        if let Some(tlsa) = tlsa_lookups.get(&pr.port)
            && !tlsa.is_empty()
        {
            pr.tlsa = Some(TlsaInfo {
                records: tlsa.records.iter().map(|r| r.display.clone()).collect(),
                dnssec_signed: tlsa.dnssec_signed,
                dane_valid: port_dane_valid.get(&pr.port).copied().flatten(),
            });
        }
    }

    // Quality assessment (always when enabled in config)
    let do_quality = config.quality.enabled;

    let hostname_quality = if do_quality {
        let is_hn = is_hostname;
        let skip_http = config.quality.skip_http_checks || !is_hn;
        if skip_http {
            Some(crate::quality::assess_hostname(None, None))
        } else {
            let hsts_port = crate::quality::hsts_port(&parsed.ports);
            let first_ip = inspected_ips[0];
            let hostname_clone = hostname_str.clone();
            let http_timeout = Duration::from_secs(config.quality.http_check_timeout_secs);

            let hsts_connector = Arc::clone(&state.hsts_tls_connector);
            let (hsts_result, redirect_result) = tokio::join!(
                crate::quality::http::check_hsts(
                    first_ip,
                    &hostname_clone,
                    hsts_port,
                    http_timeout,
                    &hsts_connector,
                ),
                crate::quality::http::check_https_redirect(first_ip, &hostname_clone, http_timeout,),
            );
            Some(crate::quality::assess_hostname(
                Some(hsts_result),
                Some(redirect_result),
            ))
        }
    } else {
        None
    };

    // Compute summary from first successful port result
    let (validation_ref, ocsp_stapled, has_ocsp_url) = port_results
        .iter()
        .flat_map(|pr| pr.ips.iter())
        .find(|ip_result| ip_result.error.is_none())
        .map(|ip_result| {
            (
                &ip_result.validation,
                ip_result.ocsp_stapled(),
                ip_result.leaf_has_ocsp_url(),
            )
        })
        .unwrap_or((&None, false, false));

    // Compute CT status from first successful IP result
    let ct_status = if config.validation.check_ct {
        let ct_info = port_results
            .iter()
            .flat_map(|pr| pr.ips.iter())
            .find(|r| r.error.is_none())
            .and_then(|r| r.ct.as_ref());
        validate::ct::check_ct_status(ct_info)
    } else {
        validate::CheckStatus::Skip
    };

    // Check if any port has consistency mismatches
    let has_consistency_mismatch = port_results.iter().any(|pr| {
        pr.consistency
            .as_ref()
            .is_some_and(|c| !c.mismatches.is_empty())
    });

    let summary = validate::summarize(
        validation_ref.as_ref(),
        parsed.target.hostname(),
        ocsp_stapled,
        has_ocsp_url,
        has_consistency_mismatch,
        caa_status,
        dane_status,
        ct_status,
    );

    // Per-port quality assessment
    if do_quality {
        let ct_enabled = config.validation.check_ct;
        for pr in &mut port_results {
            let port_quality = crate::quality::assess_port(
                &pr.ips,
                pr.port,
                is_hostname,
                caa_status,
                dane_status,
                ocsp_stapled,
                pr.consistency.as_ref(),
                ct_enabled,
                if is_hostname {
                    parsed.target.hostname().unwrap_or("")
                } else {
                    ""
                },
            );
            pr.quality = Some(port_quality);
        }
    }

    let duration_ms = request_start.elapsed().as_millis() as u64;

    metrics::histogram!("tlsight_inspect_duration_ms").record(duration_ms as f64);
    metrics::counter!("tlsight_ips_inspected_total").increment(inspected_ips.len() as u64);
    if !skipped_ips.is_empty() {
        metrics::counter!("tlsight_skipped_ips_total").increment(skipped_ips.len() as u64);
    }

    tracing::info!(
        hostname = %hostname_str,
        ip_count = inspected_ips.len(),
        duration_ms = duration_ms,
        request_id = %request_id,
        "inspect complete"
    );

    // Extract soonest-expiring leaf cert for response headers
    let cert_expiry_info: Option<(String, i64)> = port_results
        .iter()
        .flat_map(|pr| pr.ips.iter())
        .filter(|r| r.error.is_none())
        .filter_map(|r| r.chain.as_ref())
        .flat_map(|chain| chain.iter())
        .filter(|c| c.position == "leaf" || c.position == "leaf_self_signed")
        .min_by_key(|c| c.days_remaining)
        .map(|c| (c.not_after.clone(), c.days_remaining));

    let mut response = Json(InspectResponse {
        request_id,
        hostname: hostname_str,
        input_mode,
        summary,
        ports: port_results,
        dns: dns_context,
        quality: hostname_quality,
        warnings,
        skipped_ips,
        duration_ms,
    })
    .into_response();

    if let Some((expiry, days)) = cert_expiry_info {
        if let Ok(v) = HeaderValue::from_str(&expiry) {
            response.headers_mut().insert("x-cert-expiry", v);
        }
        if let Ok(v) = HeaderValue::from_str(&days.to_string()) {
            response.headers_mut().insert("x-cert-days-remaining", v);
        }
    }

    Ok(response)
}

/// Map well-known STARTTLS ports to a protocol name.
fn starttls_protocol_for_port(port: u16) -> Option<String> {
    match port {
        25 | 587 => Some("smtp".to_string()),
        143 => Some("imap".to_string()),
        21 => Some("ftp".to_string()),
        _ => None,
    }
}

/// Inspect a single port across all IPs.
async fn inspect_port(
    ips: &[std::net::IpAddr],
    port: u16,
    hostname: Option<&str>,
    timeout: Duration,
    cert_verifier: Arc<dyn rustls::client::danger::ServerCertVerifier>,
    check_ct: bool,
    semaphore: Arc<tokio::sync::Semaphore>,
) -> PortResult {
    let mut join_set = tokio::task::JoinSet::new();

    for (idx, &ip) in ips.iter().enumerate() {
        let semaphore = Arc::clone(&semaphore);
        let cert_verifier = Arc::clone(&cert_verifier);
        let hostname = hostname.map(|h| h.to_string());
        join_set.spawn(async move {
            let _permit = semaphore.acquire().await.ok();
            let mut result = tls::inspect_ip(ip, port, hostname.as_deref(), timeout).await;
            if result.error.is_some() {
                metrics::counter!("tlsight_handshake_errors_total").increment(1);
            }

            // Run validation if we got a chain
            if let Some(chain) = &result.chain {
                let raw_certs = result.raw_certs.as_deref().unwrap_or(&[]);
                let validation = validate::chain_trust::validate_chain(
                    chain,
                    hostname.as_deref(),
                    cert_verifier.as_ref(),
                    raw_certs,
                );
                result.validation = Some(validation);

                // Extract SCTs from leaf certificate
                if check_ct && let Some(leaf_der) = raw_certs.first() {
                    result.ct = validate::ct::extract_ct_info(leaf_der.as_ref());
                }

                // Live OCSP revocation check via AIA OCSP URL
                let live_ocsp = if let (Some(ocsp_url), Some(leaf_der), Some(issuer_der)) = (
                    chain.first().and_then(|c| c.ocsp_url.clone()),
                    raw_certs.first().map(|c| c.as_ref().to_vec()),
                    raw_certs.get(1).map(|c| c.as_ref().to_vec()),
                ) {
                    Some(tls::ocsp::check_live_ocsp(&ocsp_url, &leaf_der, &issuer_der).await)
                } else {
                    None
                };
                if let (Some(ocsp_result), Some(tls_params)) = (live_ocsp, result.tls.as_mut()) {
                    tls_params.ocsp_live = Some(ocsp_result);
                }
            }

            (idx, result)
        });
    }

    let mut indexed: Vec<(usize, tls::IpInspectionResult)> =
        join_set.join_all().await.into_iter().collect();
    indexed.sort_by_key(|(idx, _)| *idx);
    let ip_results: Vec<tls::IpInspectionResult> = indexed.into_iter().map(|(_, r)| r).collect();

    let consistency = compute_consistency(&ip_results);

    PortResult {
        port,
        ips: ip_results,
        consistency,
        validation: None,
        tlsa: None,
        quality: None,
        error: None,
    }
}

/// Compute consistency across IP results for a single port.
///
/// Only considers successful IPs (those with `tls` present).
/// Returns `None` if fewer than 2 IPs succeeded.
fn compute_consistency(ip_results: &[tls::IpInspectionResult]) -> Option<ConsistencyResult> {
    let successful: Vec<_> = ip_results.iter().filter(|r| r.tls.is_some()).collect();

    if successful.len() < 2 {
        return None;
    }

    let mut mismatches = Vec::new();

    // Compare leaf cert fingerprints
    let fingerprints: HashMap<String, String> = successful
        .iter()
        .filter_map(|r| {
            let fp = r.chain.as_ref()?.first()?.fingerprint_sha256.clone();
            Some((r.ip.clone(), fp))
        })
        .collect();
    let certs_match = fingerprints
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
        <= 1;
    if !certs_match {
        mismatches.push(ConsistencyMismatch {
            field: "leaf_certificate".to_string(),
            values: fingerprints,
        });
    }

    // Compare TLS versions
    let versions: HashMap<String, String> = successful
        .iter()
        .filter_map(|r| {
            let v = r.tls.as_ref()?.version.clone();
            Some((r.ip.clone(), v))
        })
        .collect();
    let versions_match = versions
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
        <= 1;
    if !versions_match {
        mismatches.push(ConsistencyMismatch {
            field: "tls_version".to_string(),
            values: versions,
        });
    }

    // Compare cipher suites
    let ciphers: HashMap<String, String> = successful
        .iter()
        .filter_map(|r| {
            let c = r.tls.as_ref()?.cipher_suite.clone();
            Some((r.ip.clone(), c))
        })
        .collect();
    let ciphers_match = ciphers
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
        <= 1;
    if !ciphers_match {
        mismatches.push(ConsistencyMismatch {
            field: "cipher_suite".to_string(),
            values: ciphers,
        });
    }

    Some(ConsistencyResult {
        certificates_match: certs_match,
        tls_versions_match: versions_match,
        cipher_suites_match: ciphers_match,
        mismatches,
    })
}

/// Resolve a hostname to IP addresses using the mhost resolver from AppState.
///
/// Falls back to an error if the resolver is unavailable or returns no addresses.
/// IP filtering (DNS rebinding protection) is applied by the caller via
/// `target_policy::check_allowed_with_policy` immediately after this returns.
async fn resolve_hostname(
    hostname: &str,
    resolver: &Arc<crate::dns::DnsResolver>,
) -> Result<Vec<std::net::IpAddr>, AppError> {
    let resolver = Arc::clone(resolver);
    let hostname_owned = hostname.to_string();

    // Spawn on a blocking thread because mhost uses rand::thread_rng which is !Send in rand 0.9.
    // This matches the existing DNS task pattern used for CAA/TLSA in do_inspect.
    let rt = tokio::runtime::Handle::current();
    let ips =
        tokio::task::spawn_blocking(move || rt.block_on(resolver.lookup_ips(&hostname_owned)))
            .await
            .map_err(|e| {
                AppError::DnsResolutionFailed(format!("{hostname}: task join error: {e}"))
            })?;

    if ips.is_empty() {
        return Err(AppError::DnsResolutionFailed(format!(
            "no addresses found for {hostname}"
        )));
    }
    Ok(ips)
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
// IpInspectionResult helper
// ---------------------------------------------------------------------------

impl tls::IpInspectionResult {
    fn ocsp_stapled(&self) -> bool {
        self.tls.as_ref().is_some_and(|t| t.ocsp.stapled)
    }

    fn leaf_has_ocsp_url(&self) -> bool {
        self.chain
            .as_ref()
            .and_then(|c| c.first())
            .is_some_and(|leaf| leaf.ocsp_url.is_some())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_config() -> crate::config::Config {
        crate::config::Config {
            site_name: "tlsight".to_string(),
            server: crate::config::ServerConfig {
                bind: ([127, 0, 0, 1], 8080).into(),
                metrics_bind: ([127, 0, 0, 1], 9090).into(),
                trusted_proxies: Vec::new(),
            },
            limits: crate::config::LimitsConfig {
                per_ip_per_minute: 30,
                per_ip_burst: 10,
                per_target_per_minute: 60,
                per_target_burst: 20,
                max_concurrent_connections: 256,
                max_concurrent_handshakes: 10,
                handshake_timeout_secs: 5,
                request_timeout_secs: 15,
                max_ports: 7,
                max_ips_per_hostname: 10,
                max_domain_length: 253,
                allow_blocked_targets: false,
            },
            dns: crate::config::DnsConfig { timeout_secs: 3 },
            validation: crate::config::ValidationConfig {
                expiry_warning_days: 30,
                expiry_critical_days: 14,
                check_dane: true,
                check_caa: true,
                check_ct: false,
                custom_ca_dir: None,
            },
            ecosystem: crate::config::EcosystemConfig::default(),
            quality: crate::config::QualityConfig::default(),
            telemetry: crate::config::TelemetryConfig::default(),
            backends: crate::config::BackendsConfig::default(),
        }
    }

    fn test_state() -> AppState {
        let _ = rustls::crypto::ring::default_provider().install_default();
        AppState::new(&test_config())
    }

    fn test_router() -> Router {
        let state = test_state();
        health_router(state.clone()).merge(api_router(state))
    }

    async fn get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
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
        let app = test_router();
        let (status, body) = get(&app, "/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn ready_returns_ok() {
        // No enrichment URL configured, no custom_ca_dir — should return "ok" with no warnings.
        let mut config = test_config();
        config.validation.check_caa = false;
        config.validation.check_dane = false;
        let state = AppState::new(&config);
        let app = health_router(state.clone()).merge(api_router(state));
        let (status, body) = get(&app, "/ready").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn meta_returns_version() {
        let app = test_router();
        let (status, body) = get(&app, "/api/meta").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["version"].is_string());
        assert!(body["features"]["multi_port"].as_bool().unwrap());
        assert_eq!(body["limits"]["max_ports"], 7);
    }

    #[tokio::test]
    async fn inspect_missing_param_returns_error() {
        let app = test_router();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/inspect")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Missing required `h` param — 400 (or 500 without ConnectInfo in test)
        assert!(response.status().is_client_error() || response.status().is_server_error());
    }

    #[tokio::test]
    async fn inspect_empty_param_returns_error() {
        let app = test_router();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/inspect?h=")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Empty input — error (400 or 500 without ConnectInfo in test)
        assert!(response.status().is_client_error() || response.status().is_server_error());
    }

    #[tokio::test]
    async fn openapi_returns_json() {
        let app = test_router();
        let (status, body) = get(&app, "/api-docs/openapi.json").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["openapi"], "3.1.0");
        assert_eq!(body["info"]["title"], "tlsight");
    }
}
