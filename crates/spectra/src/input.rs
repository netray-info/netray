use std::net::SocketAddr;

use url::Url;

use crate::error::AppError;

/// Parse and normalize the user-provided URL string.
///
/// - Bare hostnames and schemeless URLs get `https://` prepended.
/// - `http://` scheme is rejected (port-80 probing is automatic).
pub fn parse_url(raw: &str) -> Result<Url, AppError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(AppError::InvalidUrl("url field is required".to_string()));
    }

    // If no scheme, prepend https://
    let input = if !raw.contains("://") {
        format!("https://{raw}")
    } else {
        raw.to_string()
    };

    let url =
        Url::parse(&input).map_err(|e| AppError::InvalidUrl(format!("Could not parse URL: {e}")))?;

    match url.scheme() {
        "http" => Err(AppError::InvalidUrl(
            "Submit the URL with https:// scheme; port-80 probing is automatic.".to_string(),
        )),
        "https" => Ok(url),
        other => Err(AppError::InvalidUrl(format!(
            "Unsupported scheme: {other}"
        ))),
    }
}

/// Resolve the hostname and validate that none of the resolved IPs are in reserved ranges.
///
/// Returns the first non-blocked `SocketAddr`.
pub async fn validate_target(url: &Url) -> Result<SocketAddr, AppError> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::InvalidUrl("URL has no host".to_string()))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addr_str = format!("{host}:{port}");

    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(|e| AppError::InvalidTarget(format!("Could not resolve hostname: {e}")))?
        .collect();

    if addrs.is_empty() {
        return Err(AppError::InvalidTarget(
            "Could not resolve hostname".to_string(),
        ));
    }

    for addr in &addrs {
        if !netray_common::target_policy::is_allowed_target(addr.ip()) {
            return Err(AppError::BlockedTarget(
                "Target resolves to a reserved address".to_string(),
            ));
        }
    }

    Ok(addrs[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_hostname() {
        let url = parse_url("example.com").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("example.com"));
    }

    #[test]
    fn parse_schemeless_url() {
        let url = parse_url("example.com/path").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.path(), "/path");
    }

    #[test]
    fn parse_https_url() {
        let url = parse_url("https://example.com").unwrap();
        assert_eq!(url.scheme(), "https");
    }

    #[test]
    fn reject_http_scheme() {
        let err = parse_url("http://example.com").unwrap_err();
        assert!(matches!(err, AppError::InvalidUrl(_)));
        assert!(err.to_string().contains("https://"));
    }

    #[test]
    fn reject_empty_input() {
        let err = parse_url("").unwrap_err();
        assert!(matches!(err, AppError::InvalidUrl(_)));
    }

    #[test]
    fn reject_unsupported_scheme() {
        let err = parse_url("ftp://example.com").unwrap_err();
        assert!(matches!(err, AppError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn validate_blocks_loopback() {
        let url = Url::parse("https://127.0.0.1").unwrap();
        let err = validate_target(&url).await.unwrap_err();
        assert!(matches!(err, AppError::BlockedTarget(_)));
    }

    #[tokio::test]
    async fn validate_blocks_private() {
        let url = Url::parse("https://192.168.1.1").unwrap();
        let err = validate_target(&url).await.unwrap_err();
        assert!(matches!(err, AppError::BlockedTarget(_)));
    }
}
