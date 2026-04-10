pub mod assembler;
pub mod caching;
pub mod cookies;
pub mod cors;
pub mod csp;
pub mod fingerprint;
pub mod headers;
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
#[tracing::instrument(skip(client, config), fields(url = %url, request_id = tracing::field::Empty))]
pub async fn inspect(
    url: &Url,
    resolved_addr: SocketAddr,
    config: &InspectConfig,
    client: &reqwest::Client,
) -> Result<InspectResult, crate::error::AppError> {
    let total_timeout = Duration::from_secs(config.total_timeout_secs);

    let https_url = url.clone();
    let cors_url = url.clone();
    let max_redirects = config.max_redirects;
    let request_timeout = Duration::from_secs(config.request_timeout_secs);
    let user_agent = format!("{}/{}", config.user_agent, env!("CARGO_PKG_VERSION"));

    let https_task = request::execute_request(
        client,
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
                client,
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
        client,
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
        Ok((https, upgrade, cors)) => {
            if let Some(ref e) = https.error {
                tracing::warn!(error = %e, "https probe failed");
                metrics::counter!("spectra_probe_failures_total", "probe" => "https").increment(1);
            }
            if let Some(ref e) = upgrade.as_ref().and_then(|r| r.error.as_ref()) {
                tracing::warn!(error = %e, "http_upgrade probe failed");
                metrics::counter!("spectra_probe_failures_total", "probe" => "http_upgrade").increment(1);
            }
            if let Some(ref e) = cors.error {
                tracing::warn!(error = %e, "cors probe failed");
                metrics::counter!("spectra_probe_failures_total", "probe" => "cors").increment(1);
            }
            Ok(InspectResult {
                https,
                http_upgrade: upgrade,
                cors,
            })
        }
        Err(_) => Err(crate::error::AppError::Timeout(config.total_timeout_secs)),
    }
}

/// Enrichment fields extracted from the ifconfig-rs lookup, passed to [`assemble_response`].
#[derive(Default)]
pub struct EnrichmentData {
    pub org: Option<String>,
    pub ip_type: Option<String>,
    pub threat: Option<String>,
    pub role: Option<String>,
}

/// Assemble the full InspectResponse from the raw task results.
pub fn assemble_response(
    original_url: &Url,
    resolved_addr: SocketAddr,
    result: InspectResult,
    enrichment: EnrichmentData,
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
    let detail_url = format!("{}/{}", enrichment_base_url.trim_end_matches('/'), ip_str);

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
            org: enrichment.org,
            detail_url,
            ip_type: enrichment.ip_type,
            threat: enrichment.threat,
            role: enrichment.role,
        },
        redirect_limit_reached,
    };

    // Run quality checks and update the report
    let checks = crate::quality::checks::run_checks(&resp);
    resp.quality = crate::quality::QualityReport::from_checks(checks);

    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_config() -> InspectConfig {
        InspectConfig {
            request_timeout_secs: 5,
            total_timeout_secs: 10,
            max_redirects: 10,
            user_agent: "test-agent".to_string(),
        }
    }

    /// Spawn a minimal HTTP server that sends `response_bytes` to every connection
    /// and returns the bound address.
    async fn spawn_http_server(response_bytes: &'static [u8]) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let mut buf = [0u8; 1024];
                    let _ = tokio::time::timeout(
                        Duration::from_millis(200),
                        stream.read(&mut buf),
                    )
                    .await;
                    let _ = stream.write_all(response_bytes).await;
                }
            }
        });
        addr
    }

    #[tokio::test]
    async fn all_probes_succeed_returns_no_errors() {
        let response: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let addr = spawn_http_server(response).await;
        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port());

        let url = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
        let client = reqwest::Client::new();
        let mut cfg = test_config();
        cfg.total_timeout_secs = 5;

        let result = inspect(&url, resolved, &cfg, &client).await.unwrap();
        assert!(
            result.https.error.is_none(),
            "https probe should not error: {:?}",
            result.https.error
        );
        assert!(
            result.cors.error.is_none(),
            "cors probe should not error: {:?}",
            result.cors.error
        );
    }

    #[tokio::test]
    async fn one_probe_fails_partial_result_still_returned() {
        // Server that closes connection immediately (no data)
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                if let Ok((_stream, _)) = listener.accept().await {
                    // close immediately — no response
                }
            }
        });

        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port());
        let url = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
        let client = reqwest::Client::new();
        let mut cfg = test_config();
        cfg.request_timeout_secs = 2;
        cfg.total_timeout_secs = 10;

        // inspect() should return Ok (not Err) even when probes fail — partial result
        let result = inspect(&url, resolved, &cfg, &client).await.unwrap();
        // At least one probe should have an error (connection closed)
        let any_error = result.https.error.is_some() || result.cors.error.is_some();
        assert!(any_error, "expected at least one probe to fail on closed connection");
    }

    #[tokio::test]
    async fn total_timeout_returns_timeout_error() {
        // Server that never responds
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                if let Ok((mut stream, _)) = listener.accept().await {
                    // Accept but never write — hang forever
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    drop(stream);
                }
            }
        });

        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port());
        let url = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
        let client = reqwest::Client::new();
        let mut cfg = test_config();
        cfg.request_timeout_secs = 60;
        cfg.total_timeout_secs = 1; // total timeout fires first

        let result = inspect(&url, resolved, &cfg, &client).await;
        assert!(
            matches!(result, Err(crate::error::AppError::Timeout(1))),
            "expected Timeout(1), got {:?}",
            result.map(|_| "Ok")
        );
    }

    #[tokio::test]
    async fn http_upgrade_probe_attempted_for_https_scheme() {
        // The upgrade probe always targets port 80 (fixed by inspect logic), so we can't
        // intercept it in a unit test without root. We verify that the probe is *attempted*
        // (http_upgrade is Some) for https:// URLs and skipped (None) for http:// URLs.
        let response: &'static [u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
        let addr = spawn_http_server(response).await;
        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port());

        let client = reqwest::Client::new();
        let mut cfg = test_config();
        cfg.request_timeout_secs = 1;
        cfg.total_timeout_secs = 5;

        // https:// → upgrade probe is attempted (Some), even if it errors (port 80 not open)
        let https_url = Url::parse(&format!("https://127.0.0.1:{}/", addr.port())).unwrap();
        let result = inspect(&https_url, resolved, &cfg, &client).await.unwrap();
        assert!(
            result.http_upgrade.is_some(),
            "expected http_upgrade to be Some for https:// URL"
        );

        // http:// → upgrade probe is skipped (None)
        let http_url = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
        let result = inspect(&http_url, resolved, &cfg, &client).await.unwrap();
        assert!(
            result.http_upgrade.is_none(),
            "expected http_upgrade to be None for http:// URL"
        );
    }
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
