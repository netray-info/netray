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
                (CheckStatus::Warn, Some(format!("Unrecognized value: {v}")))
            }
        }),
        permissions_policy: analyze_header_presence(headers, "permissions-policy", |_| {
            (CheckStatus::Pass, None)
        }),
        x_content_type_options: analyze_header_presence(headers, "x-content-type-options", |v| {
            if v.eq_ignore_ascii_case("nosniff") {
                (CheckStatus::Pass, None)
            } else {
                (
                    CheckStatus::Warn,
                    Some(format!("Expected 'nosniff', got '{v}'")),
                )
            }
        }),
        referrer_policy: analyze_list_header(headers, "referrer-policy", |v| {
            // W3C Referrer Policy, "parse a referrer policy from a Referrer-Policy header":
            // the last recognised token of the comma-separated list is the effective policy.
            let pass_values = [
                "no-referrer",
                "no-referrer-when-downgrade",
                "strict-origin",
                "strict-origin-when-cross-origin",
                "same-origin",
            ];
            let warn_values = ["origin", "origin-when-cross-origin", "unsafe-url"];
            let effective = v.split(',').map(str::trim).rfind(|t| {
                pass_values
                    .iter()
                    .chain(warn_values.iter())
                    .any(|s| t.eq_ignore_ascii_case(s))
            });
            match effective {
                Some(t) if pass_values.iter().any(|s| t.eq_ignore_ascii_case(s)) => {
                    (CheckStatus::Pass, None)
                }
                Some(t) => (CheckStatus::Warn, Some(format!("Permissive policy: {t}"))),
                None => (CheckStatus::Warn, Some(format!("Unrecognized value: {v}"))),
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

/// As [`analyze_header_presence`], over every line of a list-valued header joined with `, `, as
/// RFC 9110 §5.3 combines them.
fn analyze_list_header(
    headers: &HeaderMap,
    name: &str,
    scorer: impl FnOnce(&str) -> (CheckStatus, Option<String>),
) -> HeaderCheck {
    let values = headers.get_all(name);
    if values.iter().next().is_none() {
        return analyze_header_presence(headers, name, scorer);
    }
    let lines: Option<Vec<&str>> = values.iter().map(|v| v.to_str().ok()).collect();
    match lines {
        Some(lines) => {
            let val = lines.join(", ");
            let (status, message) = scorer(&val);
            HeaderCheck {
                status,
                value: Some(val),
                message,
            }
        }
        None => HeaderCheck {
            status: CheckStatus::Warn,
            value: None,
            message: Some("Header value contains non-ASCII".into()),
        },
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
        let h = headers_with(&[(
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        )]);
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

    #[test]
    fn referrer_policy_safe_values_are_pass() {
        for value in &[
            "no-referrer",
            "no-referrer-when-downgrade",
            "strict-origin",
            "strict-origin-when-cross-origin",
            "same-origin",
        ] {
            let h = headers_with(&[("referrer-policy", value)]);
            let report = analyze_security_headers(&h);
            assert_eq!(
                report.referrer_policy.status,
                CheckStatus::Pass,
                "expected Pass for '{value}'"
            );
        }
    }

    #[test]
    fn referrer_policy_risky_values_are_warn() {
        for value in &["origin", "origin-when-cross-origin", "unsafe-url"] {
            let h = headers_with(&[("referrer-policy", value)]);
            let report = analyze_security_headers(&h);
            assert_eq!(
                report.referrer_policy.status,
                CheckStatus::Warn,
                "expected Warn for '{value}'"
            );
        }
    }

    #[test]
    fn referrer_policy_lines_are_one_list() {
        let mut h = HeaderMap::new();
        h.append("referrer-policy", "no-referrer".parse().unwrap());
        h.append("referrer-policy", "unsafe-url".parse().unwrap());
        let report = analyze_security_headers(&h);
        assert_eq!(report.referrer_policy.status, CheckStatus::Warn);
        assert_eq!(
            report.referrer_policy.message.as_deref(),
            Some("Permissive policy: unsafe-url")
        );
        assert_eq!(
            report.referrer_policy.value.as_deref(),
            Some("no-referrer, unsafe-url")
        );
    }

    #[test]
    fn referrer_policy_unknown_value_is_warn_with_message() {
        let h = headers_with(&[("referrer-policy", "some-unknown-value")]);
        let report = analyze_security_headers(&h);
        assert_eq!(report.referrer_policy.status, CheckStatus::Warn);
        let msg = report.referrer_policy.message.unwrap_or_default();
        assert!(
            msg.contains("Unrecognized value"),
            "expected 'Unrecognized value' in message, got: '{msg}'"
        );
    }
}
