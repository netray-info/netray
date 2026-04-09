use crate::inspect::assembler::InspectResponse;
use crate::quality::types::{CheckStatus, QualityCheck};

pub fn run_checks(resp: &InspectResponse) -> Vec<QualityCheck> {
    let mut checks = Vec::new();

    // HSTS check
    checks.push(QualityCheck {
        name: "hsts".into(),
        label: "HSTS".into(),
        status: resp.security.hsts.status.clone(),
        message: match resp.security.hsts.status {
            CheckStatus::Pass => None,
            CheckStatus::Warn => Some(format!(
                "HSTS max-age is {}; recommended >= 31536000",
                resp.security.hsts.max_age.unwrap_or(0)
            )),
            CheckStatus::Fail => Some("No HSTS header found".into()),
            CheckStatus::Skip => None,
        },
        explanation: Some("HTTP Strict Transport Security tells browsers to always use HTTPS. Missing or low max-age leaves users vulnerable to downgrade attacks.".into()),
    });

    // CSP check
    checks.push(QualityCheck {
        name: "csp".into(),
        label: "Content Security Policy".into(),
        status: resp.security.csp.status.clone(),
        message: if resp.security.csp.issues.is_empty() {
            None
        } else {
            Some(resp.security.csp.issues.join("; "))
        },
        explanation: Some("CSP restricts which resources a page can load, mitigating XSS and data injection attacks.".into()),
    });

    // X-Frame-Options
    checks.push(QualityCheck {
        name: "x_frame_options".into(),
        label: "X-Frame-Options".into(),
        status: resp.security.x_frame_options.status.clone(),
        message: resp.security.x_frame_options.message.clone(),
        explanation: Some("Prevents the page from being embedded in iframes, defending against clickjacking attacks.".into()),
    });

    // X-Content-Type-Options
    checks.push(QualityCheck {
        name: "x_content_type_options".into(),
        label: "X-Content-Type-Options".into(),
        status: resp.security.x_content_type_options.status.clone(),
        message: resp.security.x_content_type_options.message.clone(),
        explanation: Some("Prevents browsers from MIME-sniffing responses away from the declared Content-Type, blocking certain injection attacks.".into()),
    });

    // Referrer-Policy
    checks.push(QualityCheck {
        name: "referrer_policy".into(),
        label: "Referrer-Policy".into(),
        status: resp.security.referrer_policy.status.clone(),
        message: resp.security.referrer_policy.message.clone(),
        explanation: Some("Controls how much referrer information is sent with outgoing requests, protecting user privacy.".into()),
    });

    // Permissions-Policy
    checks.push(QualityCheck {
        name: "permissions_policy".into(),
        label: "Permissions-Policy".into(),
        status: resp.security.permissions_policy.status.clone(),
        message: resp.security.permissions_policy.message.clone(),
        explanation: Some("Restricts which browser features (camera, microphone, geolocation) the page and embedded iframes can access.".into()),
    });

    // CORS
    checks.push(QualityCheck {
        name: "cors".into(),
        label: "CORS".into(),
        status: resp.cors.status.clone(),
        message: if resp.cors.status == CheckStatus::Pass {
            None
        } else {
            Some(resp.cors.message.clone())
        },
        explanation: Some("Cross-Origin Resource Sharing controls which external origins may access the site's resources. Misconfigured CORS can expose data to malicious sites.".into()),
    });

    // Cookies lacking Secure
    let insecure_cookies: Vec<_> = resp
        .cookies
        .iter()
        .filter(|c| !c.secure)
        .map(|c| c.name.clone())
        .collect();
    if insecure_cookies.is_empty() {
        checks.push(QualityCheck {
            name: "cookie_secure".into(),
            label: "Cookie Security".into(),
            status: if resp.cookies.is_empty() {
                CheckStatus::Skip
            } else {
                CheckStatus::Pass
            },
            message: None,
            explanation: Some("Cookies without the Secure flag can be transmitted over unencrypted HTTP, exposing session tokens to interception.".into()),
        });
    } else {
        checks.push(QualityCheck {
            name: "cookie_secure".into(),
            label: "Cookie Security".into(),
            status: CheckStatus::Warn,
            message: Some(format!(
                "Cookies without Secure flag: {}",
                insecure_cookies.join(", ")
            )),
            explanation: Some("Cookies without the Secure flag can be transmitted over unencrypted HTTP, exposing session tokens to interception.".into()),
        });
    }

    // Deprecated headers
    if resp.deprecated_headers.is_empty() {
        checks.push(QualityCheck {
            name: "deprecated_headers".into(),
            label: "Deprecated Headers".into(),
            status: CheckStatus::Pass,
            message: None,
            explanation: Some("Obsolete headers add response noise and may signal an outdated or misconfigured server.".into()),
        });
    } else {
        checks.push(QualityCheck {
            name: "deprecated_headers".into(),
            label: "Deprecated Headers".into(),
            status: CheckStatus::Warn,
            message: Some(format!(
                "Deprecated headers present: {}",
                resp.deprecated_headers.join(", ")
            )),
            explanation: Some("Obsolete headers add response noise and may signal an outdated or misconfigured server.".into()),
        });
    }

    // Info leakage
    checks.push(QualityCheck {
        name: "info_leakage".into(),
        label: "Information Leakage".into(),
        status: resp.fingerprint.info_leakage.status.clone(),
        message: if resp.fingerprint.info_leakage.exposed_headers.is_empty() {
            None
        } else {
            Some(format!(
                "Info leakage headers: {}",
                resp.fingerprint.info_leakage.exposed_headers.join(", ")
            ))
        },
        explanation: Some("Headers that expose server software names, versions, or internal paths aid attackers in reconnaissance.".into()),
    });

    // Redirect limit
    if let Some(max_redirects) = resp.redirect_limit_reached {
        checks.push(QualityCheck {
            name: "redirect_limit".into(),
            label: "Redirect Limit".into(),
            status: CheckStatus::Warn,
            message: Some(format!("Redirect limit reached ({max_redirects})")),
            explanation: Some("Excessive redirects degrade performance and may indicate a redirect loop or misconfiguration.".into()),
        });
    }

    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::assembler::{
        CachingReport, CacheControlDirectives, CdnReport, CorsReport, CspReport,
        EnrichmentInfo, FingerprintReport, HeaderCheck, HstsCheck, InfoLeakage,
        ReportingReport, SecurityReport,
    };
    use crate::quality::types::QualityReport;
    use indexmap::IndexMap;

    fn minimal_response() -> InspectResponse {
        InspectResponse {
            url: "https://example.com".into(),
            final_url: "https://example.com".into(),
            timestamp: "2026-04-09T00:00:00Z".into(),
            duration_ms: 100,
            http_version: "HTTP/2".into(),
            alt_svc: None,
            status: 200,
            redirects: vec![],
            http_upgrade: None,
            headers: IndexMap::new(),
            security: SecurityReport {
                hsts: HstsCheck {
                    status: CheckStatus::Pass,
                    max_age: Some(31536000),
                    include_sub_domains: true,
                    preload: false,
                },
                csp: CspReport {
                    status: CheckStatus::Pass,
                    enforced: true,
                    report_only: false,
                    directives: IndexMap::new(),
                    issues: vec![],
                },
                x_frame_options: HeaderCheck { status: CheckStatus::Pass, value: Some("DENY".into()), message: None },
                permissions_policy: HeaderCheck { status: CheckStatus::Pass, value: None, message: None },
                x_content_type_options: HeaderCheck { status: CheckStatus::Pass, value: Some("nosniff".into()), message: None },
                referrer_policy: HeaderCheck { status: CheckStatus::Pass, value: None, message: None },
                coop: HeaderCheck { status: CheckStatus::Pass, value: None, message: None },
                coep: HeaderCheck { status: CheckStatus::Pass, value: None, message: None },
                corp: HeaderCheck { status: CheckStatus::Pass, value: None, message: None },
            },
            cors: CorsReport {
                allows_any_origin: false,
                reflects_origin: false,
                allows_credentials: false,
                status: CheckStatus::Pass,
                message: "No CORS headers".into(),
            },
            cookies: vec![],
            compression: None,
            caching: CachingReport {
                cache_control: None,
                directives: CacheControlDirectives {
                    public: false,
                    private: false,
                    max_age: None,
                    no_store: false,
                    no_cache: false,
                    must_revalidate: false,
                    immutable: false,
                },
                etag: false,
                last_modified: false,
                vary: vec![],
                age: None,
            },
            cdn: CdnReport { detected: None, cache_status: None, indicators: vec![] },
            fingerprint: FingerprintReport {
                server: None,
                info_leakage: InfoLeakage { status: CheckStatus::Pass, exposed_headers: vec![] },
            },
            deprecated_headers: vec![],
            reporting: ReportingReport { report_to: false, nel: false, csp_reporting: false },
            quality: QualityReport::from_checks(vec![]),
            enrichment: EnrichmentInfo {
                ip: "192.0.2.1".into(),
                org: None,
                detail_url: "https://ip.netray.info/192.0.2.1".into(),
                ..Default::default()
            },
            redirect_limit_reached: None,
        }
    }

    #[test]
    fn insecure_cookie_produces_warn() {
        use crate::inspect::assembler::CookieEntry;
        let mut resp = minimal_response();
        resp.cookies = vec![CookieEntry {
            name: "session".into(),
            secure: false,
            httponly: true,
            samesite: None,
            path: None,
            domain: None,
            expires: None,
        }];
        let checks = run_checks(&resp);
        let check = checks.iter().find(|c| c.name == "cookie_secure").unwrap();
        assert_eq!(check.status, CheckStatus::Warn);
    }

    #[test]
    fn deprecated_header_produces_warn() {
        let mut resp = minimal_response();
        resp.deprecated_headers = vec!["x-powered-by".into()];
        let checks = run_checks(&resp);
        let check = checks.iter().find(|c| c.name == "deprecated_headers").unwrap();
        assert_eq!(check.status, CheckStatus::Warn);
    }

    #[test]
    fn redirect_limit_produces_warn() {
        let mut resp = minimal_response();
        resp.redirect_limit_reached = Some(10);
        let checks = run_checks(&resp);
        let check = checks.iter().find(|c| c.name == "redirect_limit").unwrap();
        assert_eq!(check.status, CheckStatus::Warn);
    }

    #[test]
    fn all_checks_have_label_and_explanation() {
        let resp = minimal_response();
        let checks = run_checks(&resp);

        assert!(checks.len() >= 10, "expected at least 10 checks, got {}", checks.len());

        for check in &checks {
            assert!(!check.label.is_empty(), "check '{}' has empty label", check.name);
            assert!(
                check.explanation.is_some(),
                "check '{}' has no explanation",
                check.name,
            );
        }
    }
}
