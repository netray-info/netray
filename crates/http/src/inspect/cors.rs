use reqwest::header::HeaderMap;

use crate::quality::types::CheckStatus;

use super::assembler::CorsReport;

/// Analyze the CORS probe response headers.
pub fn analyze_cors(headers: &HeaderMap) -> CorsReport {
    let acao = headers
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    let acac = headers
        .get("access-control-allow-credentials")
        .and_then(|v| v.to_str().ok());

    let allows_any_origin = acao == Some("*");
    let reflects_origin = acao.is_some_and(|v| v != "*" && !v.is_empty());
    let allows_credentials = acac.is_some_and(|v| v.eq_ignore_ascii_case("true"));

    // Reflected origin + credentials is the dangerous combination
    let (status, message) = if reflects_origin && allows_credentials {
        (
            CheckStatus::Fail,
            "Reflects origin with credentials enabled — credential theft risk".to_string(),
        )
    } else if allows_any_origin && allows_credentials {
        // Browsers block this, but it signals misconfiguration
        (
            CheckStatus::Warn,
            "Wildcard origin with credentials (browsers will block)".to_string(),
        )
    } else if allows_any_origin {
        (
            CheckStatus::Warn,
            "Allows any origin (Access-Control-Allow-Origin: *)".to_string(),
        )
    } else if acao.is_none() {
        (
            CheckStatus::Pass,
            "No CORS headers (same-origin only)".to_string(),
        )
    } else {
        (CheckStatus::Pass, "CORS configured".to_string())
    };

    CorsReport {
        allows_any_origin,
        reflects_origin,
        allows_credentials,
        status,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    #[test]
    fn no_cors_headers_is_pass() {
        let report = analyze_cors(&HeaderMap::new());
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(!report.allows_any_origin);
        assert!(!report.reflects_origin);
    }

    #[test]
    fn reflected_origin_with_credentials_is_fail() {
        let h = headers_with(&[
            ("access-control-allow-origin", "https://evil.example.com"),
            ("access-control-allow-credentials", "true"),
        ]);
        let report = analyze_cors(&h);
        assert_eq!(report.status, CheckStatus::Fail);
        assert!(report.reflects_origin);
        assert!(report.allows_credentials);
    }

    #[test]
    fn wildcard_origin_is_warn() {
        let h = headers_with(&[("access-control-allow-origin", "*")]);
        let report = analyze_cors(&h);
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(report.allows_any_origin);
    }

    #[test]
    fn specific_origin_without_credentials_is_pass() {
        let h = headers_with(&[("access-control-allow-origin", "https://example.com")]);
        let report = analyze_cors(&h);
        assert_eq!(report.status, CheckStatus::Pass);
    }
}
