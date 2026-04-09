pub mod assembler;
pub mod caching;
pub mod cookies;
pub mod cors;
pub mod csp;
pub mod fingerprint;
pub mod headers;
pub mod redirects;
pub mod request;
pub mod security;

use std::net::SocketAddr;
use std::time::Duration;

use chrono::Utc;
use url::Url;

use crate::config::InspectConfig;
use crate::quality::types::CheckStatus;
use assembler::*;

/// Result of the three concurrent inspection tasks.
pub struct InspectResult {
    pub https: TaskResult,
    pub http_upgrade: Option<TaskResult>,
    pub cors: TaskResult,
}

/// Raw result from a single HTTP request chain.
pub struct TaskResult {
    pub final_url: String,
    pub status: u16,
    pub http_version: String,
    pub headers: reqwest::header::HeaderMap,
    pub redirects: Vec<RedirectHop>,
    pub redirect_limit_reached: bool,
    #[allow(dead_code)] // Available for future error reporting in assembler
    pub error: Option<String>,
}

/// Execute the full inspection: HTTPS + HTTP-upgrade probe + CORS probe.
pub async fn inspect(
    url: &Url,
    resolved_addr: SocketAddr,
    config: &InspectConfig,
) -> Result<InspectResult, crate::error::AppError> {
    let total_timeout = Duration::from_secs(config.total_timeout_secs);

    let https_url = url.clone();
    let cors_url = url.clone();
    let max_redirects = config.max_redirects;
    let request_timeout = Duration::from_secs(config.request_timeout_secs);
    let user_agent = format!("{}/{}", config.user_agent, env!("CARGO_PKG_VERSION"));

    let https_task = request::execute_request(
        https_url,
        resolved_addr,
        max_redirects,
        request_timeout,
        &user_agent,
        None,
    );

    let http_upgrade_task = async {
        // Only probe port 80 when input scheme is https://
        if url.scheme() != "https" {
            return None;
        }
        let mut http_url = url.clone();
        let _ = http_url.set_scheme("http");
        let _ = http_url.set_port(Some(80));
        let upgrade_addr = SocketAddr::new(resolved_addr.ip(), 80);
        Some(
            request::execute_request(
                http_url,
                upgrade_addr,
                max_redirects,
                request_timeout,
                &user_agent,
                None,
            )
            .await,
        )
    };

    let cors_task = request::execute_request(
        cors_url,
        resolved_addr,
        max_redirects,
        request_timeout,
        &user_agent,
        Some("https://evil.example.com"),
    );

    let result = tokio::time::timeout(total_timeout, async {
        let (https_result, upgrade_result, cors_result) =
            tokio::join!(https_task, http_upgrade_task, cors_task);
        (https_result, upgrade_result, cors_result)
    })
    .await;

    match result {
        Ok((https, upgrade, cors)) => Ok(InspectResult {
            https,
            http_upgrade: upgrade,
            cors,
        }),
        Err(_) => Err(crate::error::AppError::Timeout(config.total_timeout_secs)),
    }
}

/// Assemble the full InspectResponse from the raw task results.
pub fn assemble_response(
    original_url: &Url,
    resolved_addr: SocketAddr,
    result: InspectResult,
    enrichment_org: Option<String>,
    enrichment_base_url: &str,
    duration_ms: u64,
) -> InspectResponse {
    let https = &result.https;

    // Parse all header categories from the HTTPS response
    let raw_headers = headers::dump_headers(&https.headers);
    let security_report = security::analyze_security_headers(&https.headers);
    let cors_report = cors::analyze_cors(&result.cors.headers);
    let cookie_entries = cookies::parse_cookies(&https.headers);
    let fingerprint_report = fingerprint::analyze_fingerprint(&https.headers);
    let caching_report = caching::analyze_caching(&https.headers);
    let cdn_report = caching::detect_cdn(&https.headers);
    let deprecated = caching::detect_deprecated(&https.headers);
    let reporting = analyze_reporting(&https.headers);
    let compression = https
        .headers
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let alt_svc = https
        .headers
        .get("alt-svc")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // HTTP upgrade analysis
    let http_upgrade = result.http_upgrade.map(|upgrade| {
        let redirects_to_https = upgrade.redirects.iter().any(|hop| {
            hop.location
                .as_deref()
                .is_some_and(|loc| loc.starts_with("https://"))
        }) || upgrade.final_url.starts_with("https://");

        let same_host = Url::parse(&upgrade.final_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            == original_url.host_str().map(|h| h.to_string());

        let message = if redirects_to_https && same_host {
            "HTTP port 80 redirects to HTTPS on the same host".to_string()
        } else if redirects_to_https {
            "HTTP port 80 redirects to HTTPS on a different host".to_string()
        } else {
            format!("HTTP port 80 returned status {}", upgrade.status)
        };

        HttpUpgrade {
            redirects_to_https,
            status_code: Some(upgrade.status),
            same_host,
            message,
            redirects: upgrade.redirects,
        }
    });

    let redirect_limit_reached = if https.redirect_limit_reached {
        Some(result.https.redirects.len())
    } else {
        None
    };

    let ip_str = resolved_addr.ip().to_string();
    let detail_url = format!(
        "{}/{}",
        enrichment_base_url.trim_end_matches('/'),
        ip_str
    );

    let mut resp = InspectResponse {
        url: original_url.to_string(),
        final_url: https.final_url.clone(),
        timestamp: Utc::now().to_rfc3339(),
        duration_ms,
        http_version: https.http_version.clone(),
        alt_svc,
        status: https.status,
        redirects: https.redirects.clone(),
        http_upgrade,
        headers: raw_headers,
        security: security_report,
        cors: cors_report,
        cookies: cookie_entries,
        compression,
        caching: caching_report,
        cdn: cdn_report,
        fingerprint: fingerprint_report,
        deprecated_headers: deprecated,
        reporting,
        quality: crate::quality::QualityReport {
            verdict: CheckStatus::Pass,
            checks: vec![],
        },
        enrichment: EnrichmentInfo {
            ip: ip_str,
            org: enrichment_org,
            detail_url,
        },
        redirect_limit_reached,
    };

    // Run quality checks and update the report
    let checks = crate::quality::checks::run_checks(&resp);
    resp.quality = crate::quality::QualityReport::from_checks(checks);

    resp
}

fn analyze_reporting(headers: &reqwest::header::HeaderMap) -> ReportingReport {
    let report_to = headers.contains_key("report-to");
    let nel = headers.contains_key("nel");
    let csp_reporting = headers
        .get("content-security-policy")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("report-uri") || v.contains("report-to"));

    ReportingReport {
        report_to,
        nel,
        csp_reporting,
    }
}
