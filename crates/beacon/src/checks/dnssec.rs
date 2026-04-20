use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check DNSSEC for the domain.
/// Returns (CheckResult, dnssec_ad).
pub async fn check_dnssec(
    domain: &str,
    resolver: &DnsResolver,
    tls_base_url: &str,
) -> (CheckResult, bool) {
    let signed = resolver.check_dnssec_signed(domain).await;

    let mut sub_checks = Vec::new();

    if signed {
        sub_checks.push(SubCheck {
            name: "ad_set".to_string(),
            verdict: Verdict::Pass,
            detail: "DNSSEC validated (RRSIG records present)".to_string(),
        });
    } else {
        let base = tls_base_url.trim_end_matches('/');
        let host = base
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        sub_checks.push(SubCheck {
            name: "ad_not_set".to_string(),
            verdict: Verdict::Info,
            detail: format!(
                "DNSSEC not validated; see [{host}]({base}/{domain}) for full chain validation"
            ),
        });
    }

    let detail = if signed {
        "DNSSEC signed".to_string()
    } else {
        "DNSSEC not validated".to_string()
    };

    let result = CheckResult::new(Category::Dnssec, sub_checks, detail);
    (result, signed)
}
