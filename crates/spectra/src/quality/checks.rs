use crate::inspect::assembler::InspectResponse;
use crate::quality::types::{CheckStatus, QualityCheck};

pub fn run_checks(resp: &InspectResponse) -> Vec<QualityCheck> {
    let mut checks = Vec::new();

    // HSTS check
    checks.push(QualityCheck {
        name: "hsts".into(),
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
    });

    // CSP check
    checks.push(QualityCheck {
        name: "csp".into(),
        status: resp.security.csp.status.clone(),
        message: if resp.security.csp.issues.is_empty() {
            None
        } else {
            Some(resp.security.csp.issues.join("; "))
        },
    });

    // X-Frame-Options
    checks.push(QualityCheck {
        name: "x_frame_options".into(),
        status: resp.security.x_frame_options.status.clone(),
        message: resp.security.x_frame_options.message.clone(),
    });

    // X-Content-Type-Options
    checks.push(QualityCheck {
        name: "x_content_type_options".into(),
        status: resp.security.x_content_type_options.status.clone(),
        message: resp.security.x_content_type_options.message.clone(),
    });

    // Referrer-Policy
    checks.push(QualityCheck {
        name: "referrer_policy".into(),
        status: resp.security.referrer_policy.status.clone(),
        message: resp.security.referrer_policy.message.clone(),
    });

    // Permissions-Policy
    checks.push(QualityCheck {
        name: "permissions_policy".into(),
        status: resp.security.permissions_policy.status.clone(),
        message: resp.security.permissions_policy.message.clone(),
    });

    // CORS
    checks.push(QualityCheck {
        name: "cors".into(),
        status: resp.cors.status.clone(),
        message: if resp.cors.status == CheckStatus::Pass {
            None
        } else {
            Some(resp.cors.message.clone())
        },
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
            status: if resp.cookies.is_empty() {
                CheckStatus::Skip
            } else {
                CheckStatus::Pass
            },
            message: None,
        });
    } else {
        checks.push(QualityCheck {
            name: "cookie_secure".into(),
            status: CheckStatus::Warn,
            message: Some(format!(
                "Cookies without Secure flag: {}",
                insecure_cookies.join(", ")
            )),
        });
    }

    // Deprecated headers
    if resp.deprecated_headers.is_empty() {
        checks.push(QualityCheck {
            name: "deprecated_headers".into(),
            status: CheckStatus::Pass,
            message: None,
        });
    } else {
        checks.push(QualityCheck {
            name: "deprecated_headers".into(),
            status: CheckStatus::Warn,
            message: Some(format!(
                "Deprecated headers present: {}",
                resp.deprecated_headers.join(", ")
            )),
        });
    }

    // Info leakage
    checks.push(QualityCheck {
        name: "info_leakage".into(),
        status: resp.fingerprint.info_leakage.status.clone(),
        message: if resp.fingerprint.info_leakage.exposed_headers.is_empty() {
            None
        } else {
            Some(format!(
                "Info leakage headers: {}",
                resp.fingerprint.info_leakage.exposed_headers.join(", ")
            ))
        },
    });

    // Redirect limit
    if let Some(max_redirects) = resp.redirect_limit_reached {
        checks.push(QualityCheck {
            name: "redirect_limit".into(),
            status: CheckStatus::Warn,
            message: Some(format!("Redirect limit reached ({max_redirects})")),
        });
    }

    checks
}
