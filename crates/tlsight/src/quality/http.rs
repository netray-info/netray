use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::validate::CheckStatus;

use super::types::HstsInfo;

/// Accept-any verifier for HSTS check (same pattern as tls/connect.rs).
#[derive(Debug)]
struct AcceptAnyCert;

impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub struct HstsCheckResult {
    pub status: CheckStatus,
    pub info: Option<HstsInfo>,
    pub detail: String,
}

pub struct RedirectCheckResult {
    pub status: CheckStatus,
    pub redirect_url: Option<String>,
    pub detail: String,
}

/// Check HSTS header by making a HEAD request over TLS.
pub async fn check_hsts(
    ip: IpAddr,
    hostname: &str,
    port: u16,
    timeout: Duration,
) -> HstsCheckResult {
    let result = tokio::time::timeout(timeout, async {
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyCert))
            .with_no_client_auth();

        let server_name = match ServerName::try_from(hostname.to_owned()) {
            Ok(sn) => sn,
            Err(_) => {
                return HstsCheckResult {
                    status: CheckStatus::Skip,
                    info: None,
                    detail: "invalid hostname for HSTS check".to_string(),
                };
            }
        };

        let connector = TlsConnector::from(Arc::new(config));
        let tcp = match TcpStream::connect((ip, port)).await {
            Ok(tcp) => tcp,
            Err(e) => {
                return HstsCheckResult {
                    status: CheckStatus::Skip,
                    info: None,
                    detail: format!("connection failed: {e}"),
                };
            }
        };
        let _ = tcp.set_nodelay(true);

        let mut tls = match connector.connect(server_name, tcp).await {
            Ok(tls) => tls,
            Err(e) => {
                return HstsCheckResult {
                    status: CheckStatus::Skip,
                    info: None,
                    detail: format!("TLS handshake failed: {e}"),
                };
            }
        };

        let request = format!("HEAD / HTTP/1.1\r\nHost: {hostname}\r\nConnection: close\r\n\r\n");
        if let Err(e) = tls.write_all(request.as_bytes()).await {
            return HstsCheckResult {
                status: CheckStatus::Skip,
                info: None,
                detail: format!("write failed: {e}"),
            };
        }

        let mut buf = vec![0u8; 8192];
        let n = match tls.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                return HstsCheckResult {
                    status: CheckStatus::Skip,
                    info: None,
                    detail: format!("read failed: {e}"),
                };
            }
        };

        let response = String::from_utf8_lossy(&buf[..n]);
        let (_, headers) = parse_http_response(&response);

        match headers.get("strict-transport-security") {
            Some(value) => {
                let info = parse_hsts_header(value);
                let status = if info.max_age >= 15_768_000 {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Warn
                };
                let age_human = humanize_seconds(info.max_age);
                let detail = format!(
                    "max-age={} ({}){}{}",
                    info.max_age,
                    age_human,
                    if info.include_sub_domains {
                        ", includeSubDomains"
                    } else {
                        ""
                    },
                    if info.preload { ", preload" } else { "" },
                );
                HstsCheckResult {
                    status,
                    info: Some(info),
                    detail,
                }
            }
            None => HstsCheckResult {
                status: CheckStatus::Fail,
                info: None,
                detail: "Strict-Transport-Security header absent".to_string(),
            },
        }
    })
    .await;

    match result {
        Ok(r) => r,
        Err(_) => HstsCheckResult {
            status: CheckStatus::Skip,
            info: None,
            detail: "HSTS check timed out".to_string(),
        },
    }
}

/// Check HTTP-to-HTTPS redirect by making a plaintext HEAD request to port 80.
pub async fn check_https_redirect(
    ip: IpAddr,
    hostname: &str,
    timeout: Duration,
) -> RedirectCheckResult {
    let result = tokio::time::timeout(timeout, async {
        let mut tcp = match TcpStream::connect((ip, 80)).await {
            Ok(tcp) => tcp,
            Err(_) => {
                return RedirectCheckResult {
                    status: CheckStatus::Skip,
                    redirect_url: None,
                    detail: "port 80 not reachable".to_string(),
                };
            }
        };
        let _ = tcp.set_nodelay(true);

        let request = format!("HEAD / HTTP/1.1\r\nHost: {hostname}\r\nConnection: close\r\n\r\n");
        if let Err(e) = tcp.write_all(request.as_bytes()).await {
            return RedirectCheckResult {
                status: CheckStatus::Skip,
                redirect_url: None,
                detail: format!("write failed: {e}"),
            };
        }

        let mut buf = vec![0u8; 8192];
        let n = match tcp.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                return RedirectCheckResult {
                    status: CheckStatus::Skip,
                    redirect_url: None,
                    detail: format!("read failed: {e}"),
                };
            }
        };

        let response = String::from_utf8_lossy(&buf[..n]);
        let (status_code, headers) = parse_http_response(&response);

        if !(300..400).contains(&status_code) {
            return RedirectCheckResult {
                status: CheckStatus::Fail,
                redirect_url: None,
                detail: format!("HTTP {status_code} \u{2014} no redirect to HTTPS"),
            };
        }

        match headers.get("location") {
            Some(location) => {
                let location_lower = location.to_lowercase();
                if !location_lower.starts_with("https://") {
                    return RedirectCheckResult {
                        status: CheckStatus::Fail,
                        redirect_url: Some(location.clone()),
                        detail: format!("redirects to non-HTTPS: {location}"),
                    };
                }
                // Check if redirect target matches hostname
                let target_host = location
                    .strip_prefix("https://")
                    .or_else(|| location.strip_prefix("HTTPS://"))
                    .unwrap_or(location)
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .split(':')
                    .next()
                    .unwrap_or("");
                let status = if target_host.eq_ignore_ascii_case(hostname) {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Warn
                };
                let detail = if status == CheckStatus::Pass {
                    format!("HTTP {status_code} \u{2192} {location}")
                } else {
                    format!("HTTP {status_code} \u{2192} {location} (different host)")
                };
                RedirectCheckResult {
                    status,
                    redirect_url: Some(location.clone()),
                    detail,
                }
            }
            None => RedirectCheckResult {
                status: CheckStatus::Fail,
                redirect_url: None,
                detail: format!("HTTP {status_code} redirect without Location header"),
            },
        }
    })
    .await;

    match result {
        Ok(r) => r,
        Err(_) => RedirectCheckResult {
            status: CheckStatus::Skip,
            redirect_url: None,
            detail: "redirect check timed out".to_string(),
        },
    }
}

/// Format seconds as a human-readable duration (e.g. "1 year" or "30 days").
fn humanize_seconds(secs: u64) -> String {
    const DAY: u64 = 86400;
    const YEAR: u64 = 365 * DAY;
    const MONTH: u64 = 30 * DAY;
    if secs >= YEAR {
        let years = secs / YEAR;
        if years == 1 { "1 year".to_string() } else { format!("{years} years") }
    } else if secs >= MONTH {
        let months = secs / MONTH;
        if months == 1 { "1 month".to_string() } else { format!("{months} months") }
    } else {
        let days = secs / DAY;
        if days <= 1 { "1 day".to_string() } else { format!("{days} days") }
    }
}

/// Minimal HTTP response parser. Returns (status_code, headers).
/// Headers are lowercased keys.
fn parse_http_response(response: &str) -> (u16, HashMap<String, String>) {
    let mut headers = HashMap::new();
    let mut lines = response.lines();

    let status_code = lines
        .next()
        .and_then(|line| {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            parts.get(1)?.parse::<u16>().ok()
        })
        .unwrap_or(0);

    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    (status_code, headers)
}

/// Parse HSTS header value into structured info.
pub fn parse_hsts_header(value: &str) -> HstsInfo {
    let mut max_age = 0u64;
    let mut include_sub_domains = false;
    let mut preload = false;

    for directive in value.split(';') {
        let directive = directive.trim().to_lowercase();
        if let Some(val) = directive.strip_prefix("max-age=") {
            max_age = val.trim().parse().unwrap_or(0);
        } else if directive == "includesubdomains" {
            include_sub_domains = true;
        } else if directive == "preload" {
            preload = true;
        }
    }

    HstsInfo {
        present: true,
        max_age,
        include_sub_domains,
        preload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- HSTS header parsing ---

    #[test]
    fn parse_hsts_full() {
        let info = parse_hsts_header("max-age=31536000; includeSubDomains; preload");
        assert_eq!(info.max_age, 31536000);
        assert!(info.include_sub_domains);
        assert!(info.preload);
        assert!(info.present);
    }

    #[test]
    fn parse_hsts_max_age_only() {
        let info = parse_hsts_header("max-age=86400");
        assert_eq!(info.max_age, 86400);
        assert!(!info.include_sub_domains);
        assert!(!info.preload);
    }

    #[test]
    fn parse_hsts_zero_max_age() {
        let info = parse_hsts_header("max-age=0");
        assert_eq!(info.max_age, 0);
    }

    #[test]
    fn parse_hsts_malformed_max_age() {
        let info = parse_hsts_header("max-age=abc");
        assert_eq!(info.max_age, 0);
    }

    #[test]
    fn parse_hsts_empty() {
        let info = parse_hsts_header("");
        assert_eq!(info.max_age, 0);
        assert!(!info.include_sub_domains);
        assert!(!info.preload);
    }

    #[test]
    fn parse_hsts_case_insensitive() {
        let info = parse_hsts_header("Max-Age=31536000; INCLUDESUBDOMAINS; Preload");
        assert_eq!(info.max_age, 31536000);
        assert!(info.include_sub_domains);
        assert!(info.preload);
    }

    // --- HTTP response parsing ---

    #[test]
    fn parse_response_200() {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n";
        let (status, headers) = parse_http_response(resp);
        assert_eq!(status, 200);
        assert_eq!(headers.get("content-type").unwrap(), "text/html");
    }

    #[test]
    fn parse_response_301() {
        let resp = "HTTP/1.1 301 Moved Permanently\r\nLocation: https://example.com/\r\n\r\n";
        let (status, headers) = parse_http_response(resp);
        assert_eq!(status, 301);
        assert_eq!(headers.get("location").unwrap(), "https://example.com/");
    }

    #[test]
    fn parse_response_empty() {
        let (status, headers) = parse_http_response("");
        assert_eq!(status, 0);
        assert!(headers.is_empty());
    }

    #[test]
    fn parse_response_hsts_header() {
        let resp = "HTTP/1.1 200 OK\r\nStrict-Transport-Security: max-age=31536000\r\n\r\n";
        let (_, headers) = parse_http_response(resp);
        assert_eq!(
            headers.get("strict-transport-security").unwrap(),
            "max-age=31536000"
        );
    }
}
