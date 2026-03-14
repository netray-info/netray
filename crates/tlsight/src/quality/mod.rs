pub mod checks;
pub mod http;
pub mod types;

pub use types::{HealthCheck, PortQualityResult, QualityResult, RedirectInfo};

use crate::routes::ConsistencyResult;
use crate::tls::IpInspectionResult;
use crate::validate::CheckStatus;

use self::http::{HstsCheckResult, RedirectCheckResult};

/// Pick the port to use for HSTS check: prefer 443 if present, else first port.
pub fn hsts_port(ports: &[u16]) -> u16 {
    if ports.contains(&443) { 443 } else { ports[0] }
}

/// Assess quality for a single port's inspection data.
pub fn assess_port(
    ips: &[IpInspectionResult],
    port: u16,
    is_hostname: bool,
    caa_status: CheckStatus,
    dane_status: CheckStatus,
    ocsp_stapled: bool,
    consistency: Option<&ConsistencyResult>,
    ct_enabled: bool,
) -> PortQualityResult {
    // Find first successful IP for data
    let first_ok = ips.iter().find(|r| r.error.is_none());

    let Some(ip_result) = first_ok else {
        return PortQualityResult {
            verdict: CheckStatus::Skip,
            checks: vec![],
        };
    };

    let mut all_checks = Vec::new();

    // Certificate checks
    if let Some(validation) = &ip_result.validation {
        all_checks.push(checks::check_chain_trusted(validation));
        all_checks.push(checks::check_not_expired(validation));
        all_checks.push(checks::check_hostname_match(validation, is_hostname));
        all_checks.push(checks::check_chain_complete(validation));
        all_checks.push(checks::check_strong_signature(validation));
    }

    if let Some(chain) = &ip_result.chain {
        all_checks.push(checks::check_key_strength(chain));
        all_checks.push(checks::check_expiry_window(chain));
        all_checks.push(checks::check_cert_lifetime(chain));
    }

    // Protocol checks
    if let Some(tls) = &ip_result.tls {
        all_checks.push(checks::check_tls_version(&tls.version));
        all_checks.push(checks::check_forward_secrecy(&tls.cipher_suite));
        all_checks.push(checks::check_aead_cipher(&tls.cipher_suite));
    }

    let sct_count = ip_result.ct.as_ref().map(|ct| ct.sct_count);
    all_checks.push(checks::check_ct_logged(ct_enabled, sct_count));
    all_checks.push(checks::check_ocsp_stapled(ocsp_stapled));

    // Configuration checks
    all_checks.push(checks::check_caa_compliant(caa_status));
    all_checks.push(checks::check_dane_valid(dane_status));
    all_checks.push(checks::check_consistency(consistency));
    all_checks.push(checks::check_alpn_consistency(ips));

    // ECH check: read from the first successful IP's tls params
    let ech_advertised = first_ok
        .and_then(|r| r.tls.as_ref())
        .and_then(|t| t.ech_advertised);
    all_checks.push(checks::check_ech_advertised(ech_advertised, port));

    let verdict = types::compute_verdict(&all_checks);
    PortQualityResult {
        verdict,
        checks: all_checks,
    }
}

/// Assess hostname-scoped quality (HSTS + HTTPS redirect).
pub fn assess_hostname(
    hsts: Option<HstsCheckResult>,
    redirect: Option<RedirectCheckResult>,
) -> QualityResult {
    let mut checks = Vec::new();

    let hsts_info = hsts.as_ref().and_then(|h| h.info.clone());
    let redirect_info = redirect.as_ref().map(|r| RedirectInfo {
        status: r.status,
        redirect_url: r.redirect_url.clone(),
    });

    if let Some(ref h) = hsts {
        checks.push(HealthCheck {
            id: "hsts".to_string(),
            category: types::Category::Configuration,
            status: h.status,
            label: "HSTS".to_string(),
            detail: h.detail.clone(),
        });
    }

    if let Some(ref r) = redirect {
        checks.push(HealthCheck {
            id: "https_redirect".to_string(),
            category: types::Category::Configuration,
            status: r.status,
            label: "HTTPS redirect".to_string(),
            detail: r.detail.clone(),
        });
    }

    let verdict = types::compute_verdict(&checks);

    QualityResult {
        verdict,
        checks,
        hsts: hsts_info,
        https_redirect: redirect_info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsts_port_prefers_443() {
        assert_eq!(hsts_port(&[8443, 443, 993]), 443);
    }

    #[test]
    fn hsts_port_falls_back_to_first() {
        assert_eq!(hsts_port(&[8443, 993]), 8443);
    }
}
