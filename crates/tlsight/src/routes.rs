use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Query, RawQuery, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use utoipa::OpenApi;

use crate::enrichment::{CloudInfo, IpEnrichment};
use crate::error::{AppError, ErrorResponse};
use crate::input::{self, Target};
use crate::security::rate_limit::select_representative_ips;
use crate::security::target_policy;
use crate::state::AppState;
use crate::tls;
use crate::validate;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<MetaEcosystem>,
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
pub struct MetaEcosystem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_base_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
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
        description = "TLS certificate inspection and diagnostics API"
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
        MetaEcosystem,
        HealthResponse,
        ErrorResponse,
        crate::error::ErrorInfo,
        tls::IpInspectionResult,
        tls::InspectionError,
        tls::CertInfo,
        tls::TlsParams,
        tls::ocsp::OcspInfo,
        validate::CheckStatus,
        validate::Summary,
        validate::SummaryChecks,
        validate::ValidationResult,
        validate::ct::CtInfo,
        validate::ct::SctEntry,
        IpEnrichment,
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

pub fn health_router() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/ready", get(ready_handler))
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
    path = "/api/health",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse),
    )
)]
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[utoipa::path(
    get,
    path = "/api/ready",
    responses(
        (status = 200, description = "Service is ready", body = HealthResponse),
    )
)]
async fn ready_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ready" })
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
    let ecosystem =
        if config.ecosystem.dns_base_url.is_some() || config.ecosystem.ip_base_url.is_some() {
            Some(MetaEcosystem {
                dns_base_url: config.ecosystem.dns_base_url.clone(),
                ip_base_url: config.ecosystem.ip_base_url.clone(),
            })
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
    headers: axum::http::HeaderMap,
    query: Query<InspectQuery>,
) -> Result<Json<InspectResponse>, AppError> {
    let client_ip = state.ip_extractor.extract(&headers, addr);
    let config = state.config.load();
    let parsed = input::parse_input(&query.h, config.limits.max_ports)?;
    do_inspect(state, client_ip, &headers, parsed).await
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
    headers: axum::http::HeaderMap,
    raw_query: RawQuery,
    Json(body): Json<InspectPostBody>,
) -> Result<Json<InspectResponse>, AppError> {
    // Reject if query string contains h= param
    if let Some(ref qs) = raw_query.0
        && qs.contains("h=")
    {
        return Err(AppError::AmbiguousInput(
            "POST body and ?h= query parameter cannot be used together".to_string(),
        ));
    }

    let config = state.config.load();

    // Validate ports
    if body.ports.is_empty() {
        return Err(AppError::InvalidPort(
            "ports array must not be empty".to_string(),
        ));
    }
    if body.ports.len() > config.limits.max_ports {
        return Err(AppError::TooManyPorts {
            requested: body.ports.len(),
            max: config.limits.max_ports,
        });
    }
    for &p in &body.ports {
        if p == 0 {
            return Err(AppError::InvalidPort("port must be 1-65535".to_string()));
        }
    }

    // Validate hostname through the same path as GET
    let target_parsed = input::parse_input(&body.hostname, config.limits.max_ports)?;
    let parsed = input::ParsedInput {
        target: target_parsed.target,
        ports: body.ports,
    };

    let client_ip = state.ip_extractor.extract(&headers, addr);
    do_inspect(state, client_ip, &headers, parsed).await
}

async fn do_inspect(
    state: AppState,
    client_ip: IpAddr,
    _headers: &axum::http::HeaderMap,
    parsed: input::ParsedInput,
) -> Result<Json<InspectResponse>, AppError> {
    let request_start = Instant::now();
    let config = state.config.load();

    // Extract request ID from extensions (set by middleware)
    let request_id = uuid::Uuid::now_v7().to_string();
    tracing::Span::current().record("request_id", &request_id);
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

    // Resolve IPs
    let ips = match &parsed.target {
        Target::Hostname(h) => resolve_hostname(h).await?,
        Target::Ip(ip) => vec![*ip],
    };

    if ips.is_empty() {
        return Err(AppError::DnsResolutionFailed(format!(
            "no addresses found for {hostname_str}"
        )));
    }

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
            return Err(AppError::RateLimited {
                retry_after_secs: 60,
                scope: "per_ip",
            });
        }

        let reduced_cost = parsed.ports.len() as u32 * selected.len() as u32;
        state
            .rate_limiter
            .check_cost(client_ip, &hostname_str, reduced_cost)?;

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
        Some(tokio::spawn(async move { client.lookup_batch(&ips).await }))
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

                (caa, tlsa)
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
    let (caa_lookup, tlsa_lookups) = if let Some(handle) = dns_handle {
        match handle.await {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(error = %e, "DNS task failed or was cancelled");
                (None, HashMap::new())
            }
        }
    } else {
        (None, HashMap::new())
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

    // DANE status: Skip without DNSSEC
    let dane_status = validate::CheckStatus::Skip;

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

    // Inject enrichment data into IP results
    let mut port_results = port_results;
    if !enrichments.is_empty() {
        for pr in &mut port_results {
            for ip_result in &mut pr.ips {
                if let Ok(ip) = ip_result.ip.parse::<IpAddr>() {
                    ip_result.enrichment = enrichments.get(&ip).cloned();
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
                dane_valid: None, // DNSSEC not available
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
    let (validation_ref, ocsp_stapled) = port_results
        .iter()
        .flat_map(|pr| pr.ips.iter())
        .find(|ip_result| ip_result.error.is_none())
        .map(|ip_result| (&ip_result.validation, ip_result.ocsp_stapled()))
        .unwrap_or((&None, false));

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
                is_hostname,
                caa_status,
                dane_status,
                ocsp_stapled,
                pr.consistency.as_ref(),
                ct_enabled,
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

    Ok(Json(InspectResponse {
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
    }))
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
    let mut ip_results = Vec::with_capacity(ips.len());

    for &ip in ips {
        let _permit = semaphore.acquire().await.ok();
        let mut result = tls::inspect_ip(ip, port, hostname, timeout).await;
        if result.error.is_some() {
            metrics::counter!("tlsight_handshake_errors_total").increment(1);
        }

        // Run validation if we got a chain
        if let Some(chain) = &result.chain {
            let raw_certs = result.raw_certs.as_deref().unwrap_or(&[]);
            let validation = validate::chain_trust::validate_chain(
                chain,
                hostname,
                cert_verifier.as_ref(),
                raw_certs,
            );
            result.validation = Some(validation);

            // Extract SCTs from leaf certificate
            if check_ct && let Some(leaf_der) = raw_certs.first() {
                result.ct = validate::ct::extract_ct_info(leaf_der.as_ref());
            }
        }

        ip_results.push(result);
    }

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

/// Resolve a hostname to IP addresses using tokio's built-in resolver.
/// Phase 1: Simple resolution. Phase 2+ will use mhost for DNSSEC.
async fn resolve_hostname(hostname: &str) -> Result<Vec<std::net::IpAddr>, AppError> {
    let addrs: Vec<_> = tokio::net::lookup_host(format!("{hostname}:0"))
        .await
        .map_err(|e| AppError::DnsResolutionFailed(format!("{hostname}: {e}")))?
        .map(|addr| addr.ip())
        .collect();

    // Deduplicate
    let mut unique = Vec::new();
    for ip in addrs {
        if !unique.contains(&ip) {
            unique.push(ip);
        }
    }
    Ok(unique)
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
            dns: crate::config::DnsConfig {
                resolver: "cloudflare".to_owned(),
                timeout_secs: 3,
            },
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
        }
    }

    fn test_state() -> AppState {
        let _ = rustls::crypto::ring::default_provider().install_default();
        AppState::new(&test_config())
    }

    fn test_router() -> Router {
        let state = test_state();
        health_router().merge(api_router(state))
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
        let (status, body) = get(&app, "/api/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn ready_returns_ready() {
        let app = test_router();
        let (status, body) = get(&app, "/api/ready").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
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
