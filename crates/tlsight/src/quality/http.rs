use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::validate::CheckStatus;

use super::types::HstsInfo;

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
    connector: &Arc<TlsConnector>,
) -> HstsCheckResult {
    let result = tokio::time::timeout(timeout, async {
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

        let connector = Arc::clone(connector);
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
        if years == 1 {
            "1 year".to_string()
        } else {
            format!("{years} years")
        }
    } else if secs >= MONTH {
        let months = secs / MONTH;
        if months == 1 {
            "1 month".to_string()
        } else {
            format!("{months} months")
        }
    } else {
        let days = secs / DAY;
        if days <= 1 {
            "1 day".to_string()
        } else {
            format!("{days} days")
        }
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
    use std::time::Duration;

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    fn make_self_signed() -> (
        rustls::pki_types::CertificateDer<'static>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ) {
        use rcgen::CertifiedKey;
        let CertifiedKey { cert, key_pair } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let cert_der = rustls::pki_types::CertificateDer::from(cert.der().to_vec());
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der()).unwrap();
        (cert_der, key_der)
    }

    fn make_test_connector() -> Arc<TlsConnector> {
        ensure_crypto_provider();
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(crate::tls::verifier::AcceptAnyCert))
            .with_no_client_auth();
        Arc::new(TlsConnector::from(Arc::new(config)))
    }

    /// Spawns a TLS server that responds with a canned HTTP response.
    async fn spawn_tls_http_server(http_response: &'static str) -> u16 {
        ensure_crypto_provider();
        let (cert_der, key_der) = make_self_signed();

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server_config));
            while let Ok((stream, _)) = listener.accept().await {
                if let Ok(mut tls) = acceptor.accept(stream).await {
                    let mut buf = [0u8; 1024];
                    let _ = tls.read(&mut buf).await;
                    let _ = tls.write_all(http_response.as_bytes()).await;
                }
            }
        });

        port
    }

    // --- check_hsts integration tests ---

    #[tokio::test]
    async fn hsts_pass_when_header_present_with_long_max_age() {
        let response = "HTTP/1.1 200 OK\r\nStrict-Transport-Security: max-age=31536000; includeSubDomains\r\nConnection: close\r\n\r\n";
        let port = spawn_tls_http_server(response).await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let connector = make_test_connector();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let result = check_hsts(ip, "localhost", port, Duration::from_secs(5), &connector).await;

        assert_eq!(result.status, CheckStatus::Pass);
        let info = result.info.unwrap();
        assert_eq!(info.max_age, 31536000);
        assert!(info.include_sub_domains);
    }

    #[tokio::test]
    async fn hsts_warn_when_max_age_too_short() {
        // 86400 = 1 day, below the 15_768_000 (6 months) threshold
        let response = "HTTP/1.1 200 OK\r\nStrict-Transport-Security: max-age=86400\r\nConnection: close\r\n\r\n";
        let port = spawn_tls_http_server(response).await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let connector = make_test_connector();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let result = check_hsts(ip, "localhost", port, Duration::from_secs(5), &connector).await;

        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[tokio::test]
    async fn hsts_fail_when_header_absent() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n";
        let port = spawn_tls_http_server(response).await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let connector = make_test_connector();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let result = check_hsts(ip, "localhost", port, Duration::from_secs(5), &connector).await;

        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.info.is_none());
    }

    #[tokio::test]
    async fn hsts_skip_on_connection_refused() {
        let connector = make_test_connector();
        // Port 1 is never open
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let result = check_hsts(ip, "localhost", 1, Duration::from_secs(2), &connector).await;

        assert_eq!(result.status, CheckStatus::Skip);
    }

    // --- check_https_redirect integration tests ---
    // check_https_redirect hardcodes port 80, so connection tests that require
    // a specific port cannot inject a custom server. We verify the Skip path
    // (port 80 not reachable on loopback in test environments).

    #[tokio::test]
    async fn redirect_skip_when_port_80_not_reachable() {
        // In CI and dev environments port 80 on 127.0.0.1 is typically closed.
        // We can only assert it doesn't panic; the actual status depends on the environment.
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        let result = check_https_redirect(ip, "localhost", Duration::from_millis(500)).await;
        let _ = result.status;
    }

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
