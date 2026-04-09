use reqwest::header::HeaderMap;

use crate::quality::types::CheckStatus;

use super::assembler::{HeaderCheck, HstsCheck, SecurityReport};
use super::csp;

pub fn analyze_security_headers(headers: &HeaderMap) -> SecurityReport {
    SecurityReport {
        hsts: analyze_hsts(headers),
        csp: csp::analyze_csp(headers),
        x_frame_options: analyze_header_presence(headers, "x-frame-options", |v| {
            let v_upper = v.to_uppercase();
            if v_upper == "DENY" || v_upper == "SAMEORIGIN" {
                (CheckStatus::Pass, None)
            } else {
                (
                    CheckStatus::Warn,
                    Some(format!("Unrecognized value: {v}")),
                )
            }
        }),
        permissions_policy: analyze_header_presence(headers, "permissions-policy", |_| {
            (CheckStatus::Pass, None)
        }),
        x_content_type_options: analyze_header_presence(
            headers,
            "x-content-type-options",
            |v| {
                if v.eq_ignore_ascii_case("nosniff") {
                    (CheckStatus::Pass, None)
                } else {
                    (
                        CheckStatus::Warn,
                        Some(format!("Expected 'nosniff', got '{v}'")),
                    )
                }
            },
        ),
        referrer_policy: analyze_header_presence(headers, "referrer-policy", |v| {
            let safe_values = [
                "no-referrer",
                "same-origin",
                "strict-origin",
                "strict-origin-when-cross-origin",
            ];
            if safe_values.iter().any(|s| v.eq_ignore_ascii_case(s)) {
                (CheckStatus::Pass, None)
            } else if v.eq_ignore_ascii_case("unsafe-url") || v.eq_ignore_ascii_case("no-referrer-when-downgrade") {
                (CheckStatus::Warn, Some(format!("Permissive policy: {v}")))
            } else {
                (CheckStatus::Pass, None)
            }
        }),
        coop: analyze_header_presence(headers, "cross-origin-opener-policy", |_| {
            (CheckStatus::Pass, None)
        }),
        coep: analyze_header_presence(headers, "cross-origin-embedder-policy", |_| {
            (CheckStatus::Pass, None)
        }),
        corp: analyze_header_presence(headers, "cross-origin-resource-policy", |_| {
            (CheckStatus::Pass, None)
        }),
    }
}

fn analyze_hsts(headers: &HeaderMap) -> HstsCheck {
    let value = match headers.get("strict-transport-security") {
        Some(v) => match v.to_str() {
            Ok(s) => s,
            Err(_) => {
                return HstsCheck {
                    status: CheckStatus::Fail,
                    max_age: None,
                    include_sub_domains: false,
                    preload: false,
                };
            }
        },
        None => {
            return HstsCheck {
                status: CheckStatus::Fail,
                max_age: None,
                include_sub_domains: false,
                preload: false,
            };
        }
    };

    let lower = value.to_lowercase();
    let mut max_age: Option<u64> = None;
    let mut include_sub_domains = false;
    let mut preload = false;

    for part in lower.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("max-age=") {
            max_age = val.trim().parse().ok();
        } else if part == "includesubdomains" {
            include_sub_domains = true;
        } else if part == "preload" {
            preload = true;
        }
    }

    let status = match max_age {
        Some(age) if age >= 31_536_000 => CheckStatus::Pass,
        Some(_) => CheckStatus::Warn,
        None => CheckStatus::Fail,
    };

    HstsCheck {
        status,
        max_age,
        include_sub_domains,
        preload,
    }
}

fn analyze_header_presence(
    headers: &HeaderMap,
    name: &str,
    scorer: impl FnOnce(&str) -> (CheckStatus, Option<String>),
) -> HeaderCheck {
    match headers.get(name) {
        Some(v) => match v.to_str() {
            Ok(val) => {
                let (status, message) = scorer(val);
                HeaderCheck {
                    status,
                    value: Some(val.to_string()),
                    message,
                }
            }
            Err(_) => HeaderCheck {
                status: CheckStatus::Warn,
                value: None,
                message: Some("Header value contains non-ASCII".into()),
            },
        },
        None => HeaderCheck {
            status: CheckStatus::Warn,
            value: None,
            message: Some(format!("Missing {name} header")),
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
    fn hsts_pass_with_year() {
        let h = headers_with(&[("strict-transport-security", "max-age=31536000; includeSubDomains")]);
        let result = analyze_hsts(&h);
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.max_age, Some(31536000));
        assert!(result.include_sub_domains);
    }

    #[test]
    fn hsts_warn_with_low_max_age() {
        let h = headers_with(&[("strict-transport-security", "max-age=3600")]);
        let result = analyze_hsts(&h);
        assert_eq!(result.status, CheckStatus::Warn);
        assert_eq!(result.max_age, Some(3600));
    }

    #[test]
    fn hsts_fail_when_missing() {
        let h = HeaderMap::new();
        let result = analyze_hsts(&h);
        assert_eq!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn hsts_preload_flag() {
        let h = headers_with(&[("strict-transport-security", "max-age=31536000; preload")]);
        let result = analyze_hsts(&h);
        assert!(result.preload);
    }

    #[test]
    fn xfo_deny_is_pass() {
        let h = headers_with(&[("x-frame-options", "DENY")]);
        let report = analyze_security_headers(&h);
        assert_eq!(report.x_frame_options.status, CheckStatus::Pass);
    }

    #[test]
    fn xcto_nosniff_is_pass() {
        let h = headers_with(&[("x-content-type-options", "nosniff")]);
        let report = analyze_security_headers(&h);
        assert_eq!(report.x_content_type_options.status, CheckStatus::Pass);
    }

    #[test]
    fn missing_header_is_warn() {
        let h = HeaderMap::new();
        let report = analyze_security_headers(&h);
        assert_eq!(report.x_frame_options.status, CheckStatus::Warn);
        assert_eq!(report.permissions_policy.status, CheckStatus::Warn);
    }
}
