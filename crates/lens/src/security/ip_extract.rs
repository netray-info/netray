use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::http::HeaderMap;
use netray_common::ip_extract::IpExtractor;

/// Extract the real client IP from request headers using the trusted proxy list.
///
/// Delegates to `netray_common::IpExtractor`. Because axum middleware does not
/// always surface a peer `SocketAddr` directly, this function synthesises a
/// loopback peer address. The extractor will treat the loopback peer as trusted
/// only if `127.0.0.1` appears in the proxy list — callers that need the
/// actual peer IP must pass it via their own extractor instance.
///
/// If `trusted_proxies` is empty, all proxy headers are ignored and the fallback
/// `127.0.0.1` is returned.
pub fn extract_client_ip(headers: &HeaderMap, trusted_proxies: &[String]) -> IpAddr {
    if trusted_proxies.is_empty() {
        return IpAddr::V4(Ipv4Addr::LOCALHOST);
    }

    let extractor = IpExtractor::new(trusted_proxies);

    // Synthesise a loopback peer so the extractor inspects headers when
    // 127.0.0.1/32 (or 127.0.0.0/8) is in the trusted proxy list.
    let synthetic_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    extractor.extract(headers, synthetic_peer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn empty_trusted_proxies_returns_loopback() {
        let headers = HeaderMap::new();
        let ip = extract_client_ip(&headers, &[]);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn extracts_x_forwarded_for_when_peer_is_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());

        // Trust 127.0.0.1/32 so the synthetic loopback peer is trusted.
        let trusted = vec!["127.0.0.1/32".to_string()];
        let ip = extract_client_ip(&headers, &trusted);
        assert_eq!(ip, "203.0.113.5".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn returns_loopback_when_no_forwarded_header() {
        let headers = HeaderMap::new();
        let trusted = vec!["127.0.0.1/32".to_string()];
        let ip = extract_client_ip(&headers, &trusted);
        // No XFF header → extractor falls back to the peer address (127.0.0.1).
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
}
