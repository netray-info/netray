use reqwest::header::HeaderMap;

use crate::quality::types::CheckStatus;

use super::assembler::{FingerprintReport, InfoLeakage};

/// Headers that leak server/technology information.
const INFO_LEAK_HEADERS: &[&str] = &[
    "server",
    "x-powered-by",
    "via",
    "x-aspnet-version",
    "x-aspnetmvc-version",
    "x-generator",
    "x-debug-token",
    "x-debug-token-link",
    "x-runtime",
];

pub fn analyze_fingerprint(headers: &HeaderMap) -> FingerprintReport {
    let server = headers
        .get("server")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut exposed = Vec::new();
    for &name in INFO_LEAK_HEADERS {
        if name == "server" {
            continue; // Server is reported separately
        }
        if headers.contains_key(name) {
            exposed.push(name.to_string());
        }
    }

    // Include Server in exposed_headers if present (for the info_leakage check)
    if server.is_some() {
        exposed.insert(0, "Server".to_string());
    }

    let status = if exposed.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };

    FingerprintReport {
        server,
        info_leakage: InfoLeakage {
            status,
            exposed_headers: exposed,
        },
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
    fn no_leakage_headers_is_pass() {
        let h = headers_with(&[("content-type", "text/html")]);
        let report = analyze_fingerprint(&h);
        assert_eq!(report.info_leakage.status, CheckStatus::Pass);
        assert!(report.info_leakage.exposed_headers.is_empty());
        assert!(report.server.is_none());
    }

    #[test]
    fn server_header_detected() {
        let h = headers_with(&[("server", "nginx/1.25")]);
        let report = analyze_fingerprint(&h);
        assert_eq!(report.server.as_deref(), Some("nginx/1.25"));
        assert_eq!(report.info_leakage.status, CheckStatus::Warn);
        assert!(report.info_leakage.exposed_headers.contains(&"Server".to_string()));
    }

    #[test]
    fn x_powered_by_detected() {
        let h = headers_with(&[("x-powered-by", "PHP/8.1")]);
        let report = analyze_fingerprint(&h);
        assert_eq!(report.info_leakage.status, CheckStatus::Warn);
        assert!(report
            .info_leakage
            .exposed_headers
            .contains(&"x-powered-by".to_string()));
    }

    #[test]
    fn multiple_leak_headers() {
        let h = headers_with(&[
            ("server", "Apache"),
            ("x-powered-by", "Express"),
            ("x-runtime", "0.042"),
        ]);
        let report = analyze_fingerprint(&h);
        assert_eq!(report.info_leakage.status, CheckStatus::Warn);
        assert!(report.info_leakage.exposed_headers.len() >= 3);
    }
}
