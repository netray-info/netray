use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use axum::http::HeaderMap;

/// Extracts the real client IP from proxy headers.
///
/// When deployed behind a reverse proxy (Cloudflare, nginx, Caddy), the direct
/// peer IP is the proxy, not the actual client. This extractor checks proxy headers
/// in priority order (CF-Connecting-IP, X-Real-IP, X-Forwarded-For) but only when
/// the peer IP is in the configured trusted proxy list.
///
/// **Safe default**: When `trusted_proxies` is empty, all proxy headers are ignored
/// and the peer address is returned directly. This prevents IP spoofing when no
/// proxy is configured.
#[derive(Debug)]
pub struct IpExtractor {
    trusted_proxies: Vec<IpAddr>,
}

impl IpExtractor {
    /// Create a new extractor from a list of trusted proxy IP strings.
    ///
    /// Returns an error if any entry looks like a CIDR range (contains `/`).
    /// Only individual IP addresses are supported; CIDR ranges are not.
    /// Other invalid IP strings are skipped with a warning.
    pub fn new(trusted_proxy_strs: &[String]) -> Result<Self, String> {
        for s in trusted_proxy_strs {
            if s.contains('/') {
                return Err(format!(
                    "trusted_proxies entry {s:?} looks like a CIDR range — only individual IP addresses are supported"
                ));
            }
        }
        let trusted_proxies = trusted_proxy_strs
            .iter()
            .filter_map(|s| {
                IpAddr::from_str(s).map_err(|_| {
                    tracing::warn!(entry = %s, "trusted_proxies entry is not a valid IP address — skipped");
                }).ok()
            })
            .collect();
        Ok(Self { trusted_proxies })
    }

    /// Extract the real client IP from headers and peer address.
    ///
    /// Priority:
    /// 1. If no trusted proxies configured, return peer IP (safe default).
    /// 2. If peer IP is not trusted, return peer IP (untrusted source).
    /// 3. Try `CF-Connecting-IP` header (Cloudflare).
    /// 4. Try `X-Real-IP` header (nginx).
    /// 5. Try rightmost non-trusted IP in `X-Forwarded-For`.
    /// 6. Fall back to peer IP.
    pub fn extract(&self, headers: &HeaderMap, peer_addr: SocketAddr) -> IpAddr {
        if self.trusted_proxies.is_empty() {
            return peer_addr.ip();
        }

        if !self.trusted_proxies.contains(&peer_addr.ip()) {
            return peer_addr.ip();
        }

        self.extract_cf_connecting_ip(headers)
            .or_else(|| self.extract_x_real_ip(headers))
            .or_else(|| self.extract_x_forwarded_for(headers))
            .unwrap_or_else(|| peer_addr.ip())
    }

    /// Parse the `CF-Connecting-IP` header (Cloudflare sets this to the client IP).
    fn extract_cf_connecting_ip(&self, headers: &HeaderMap) -> Option<IpAddr> {
        headers
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| IpAddr::from_str(s.trim()).ok())
    }

    /// Parse the `X-Real-IP` header (typically set by nginx `proxy_set_header`).
    fn extract_x_real_ip(&self, headers: &HeaderMap) -> Option<IpAddr> {
        headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| IpAddr::from_str(s.trim()).ok())
    }

    /// Walk `X-Forwarded-For` right-to-left, returning the rightmost IP that is
    /// not in the trusted proxy set.
    ///
    /// The rightmost entry is the one added by the most recent proxy. Walking
    /// right-to-left and skipping trusted proxies prevents spoofing: an attacker
    /// can prepend arbitrary IPs to X-Forwarded-For, but they cannot control the
    /// entries added by trusted proxies.
    fn extract_x_forwarded_for(&self, headers: &HeaderMap) -> Option<IpAddr> {
        let value = headers.get("x-forwarded-for")?.to_str().ok()?;
        value
            .rsplit(',')
            .filter_map(|s| IpAddr::from_str(s.trim()).ok())
            .find(|ip| !self.trusted_proxies.contains(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn peer(addr: &str) -> SocketAddr {
        addr.parse().unwrap()
    }

    fn extractor(proxies: &[&str]) -> IpExtractor {
        IpExtractor::new(&proxies.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    // ---- No trusted proxies (safe default) ----

    #[test]
    fn no_proxies_returns_peer_ip() {
        let ext = extractor(&[]);
        let headers = HeaderMap::new();
        assert_eq!(
            ext.extract(&headers, peer("1.2.3.4:12345")),
            "1.2.3.4".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn no_proxies_ignores_all_headers() {
        let ext = extractor(&[]);
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", HeaderValue::from_static("5.6.7.8"));
        headers.insert("x-real-ip", HeaderValue::from_static("9.10.11.12"));
        headers.insert("x-forwarded-for", HeaderValue::from_static("13.14.15.16"));

        assert_eq!(
            ext.extract(&headers, peer("1.2.3.4:12345")),
            "1.2.3.4".parse::<IpAddr>().unwrap()
        );
    }

    // ---- Untrusted peer ----

    #[test]
    fn untrusted_peer_returns_peer_ip() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", HeaderValue::from_static("5.6.7.8"));

        // Peer 1.2.3.4 is not in trusted list
        assert_eq!(
            ext.extract(&headers, peer("1.2.3.4:12345")),
            "1.2.3.4".parse::<IpAddr>().unwrap()
        );
    }

    // ---- CF-Connecting-IP ----

    #[test]
    fn trusted_peer_uses_cf_connecting_ip() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", HeaderValue::from_static("203.0.114.50"));

        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "203.0.114.50".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn cf_connecting_ip_with_whitespace() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert(
            "cf-connecting-ip",
            HeaderValue::from_static(" 203.0.114.50 "),
        );

        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "203.0.114.50".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn cf_connecting_ip_invalid_falls_through() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", HeaderValue::from_static("not-an-ip"));
        headers.insert("x-real-ip", HeaderValue::from_static("5.6.7.8"));

        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "5.6.7.8".parse::<IpAddr>().unwrap()
        );
    }

    // ---- X-Real-IP ----

    #[test]
    fn trusted_peer_uses_x_real_ip() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("5.6.7.8"));

        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "5.6.7.8".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn cf_connecting_ip_takes_priority_over_x_real_ip() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", HeaderValue::from_static("1.1.1.1"));
        headers.insert("x-real-ip", HeaderValue::from_static("2.2.2.2"));

        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "1.1.1.1".parse::<IpAddr>().unwrap()
        );
    }

    // ---- X-Forwarded-For ----

    #[test]
    fn x_forwarded_for_single_ip() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.114.50"));

        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "203.0.114.50".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn x_forwarded_for_rightmost_untrusted() {
        let ext = extractor(&["10.0.0.1", "10.0.0.2"]);
        let mut headers = HeaderMap::new();
        // Spoofed, Real client, Proxy 2
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("99.99.99.99, 5.6.7.8, 10.0.0.2"),
        );

        // Walking right-to-left: 10.0.0.2 (trusted, skip), 5.6.7.8 (not trusted, return)
        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "5.6.7.8".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn x_forwarded_for_all_trusted_returns_peer() {
        let ext = extractor(&["10.0.0.1", "10.0.0.2", "10.0.0.3"]);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("10.0.0.3, 10.0.0.2"),
        );

        // All entries are trusted, fallback to peer
        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "10.0.0.1".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn x_forwarded_for_with_whitespace() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("  5.6.7.8 , 10.0.0.1 "),
        );

        // 10.0.0.1 is trusted (skipped), 5.6.7.8 is returned
        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "5.6.7.8".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn x_forwarded_for_with_invalid_entries() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("5.6.7.8, garbage, not-ip"),
        );

        // Invalid entries are skipped; 5.6.7.8 is the only valid untrusted IP
        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "5.6.7.8".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn x_forwarded_for_priority_after_cf_and_real_ip() {
        let ext = extractor(&["10.0.0.1"]);
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("5.6.7.8"));

        // No CF-Connecting-IP or X-Real-IP, falls through to X-Forwarded-For
        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "5.6.7.8".parse::<IpAddr>().unwrap()
        );
    }

    // ---- Fallback ----

    #[test]
    fn no_headers_returns_peer() {
        let ext = extractor(&["10.0.0.1"]);
        let headers = HeaderMap::new();

        assert_eq!(
            ext.extract(&headers, peer("10.0.0.1:443")),
            "10.0.0.1".parse::<IpAddr>().unwrap()
        );
    }

    // ---- IPv6 ----

    #[test]
    fn ipv6_peer_and_header() {
        let ext = extractor(&["::1"]);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-real-ip",
            HeaderValue::from_static("2001:4860:4860::8888"),
        );

        assert_eq!(
            ext.extract(&headers, peer("[::1]:443")),
            "2001:4860:4860::8888".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn ipv6_in_x_forwarded_for() {
        let ext = extractor(&["::1"]);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("2606:4700::1, ::1"),
        );

        assert_eq!(
            ext.extract(&headers, peer("[::1]:443")),
            "2606:4700::1".parse::<IpAddr>().unwrap()
        );
    }

    // ---- Constructor edge cases ----

    #[test]
    fn invalid_proxy_strings_are_skipped() {
        let ext = IpExtractor::new(&[
            "10.0.0.1".to_string(),
            "not-an-ip".to_string(),
            "".to_string(),
            "10.0.0.2".to_string(),
        ])
        .unwrap();
        assert_eq!(ext.trusted_proxies.len(), 2);
    }

    #[test]
    fn cidr_entry_returns_error() {
        let err =
            IpExtractor::new(&["10.0.0.1".to_string(), "10.0.0.0/8".to_string()]).unwrap_err();
        assert!(err.contains("CIDR range"), "unexpected error: {err}");
        assert!(err.contains("10.0.0.0/8"), "unexpected error: {err}");
    }

    #[test]
    fn ipv6_cidr_entry_returns_error() {
        let err = IpExtractor::new(&["2001:db8::/32".to_string()]).unwrap_err();
        assert!(err.contains("CIDR range"), "unexpected error: {err}");
    }
}
