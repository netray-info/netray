use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check DNSSEC for the domain.
/// Returns (CheckResult, dnssec_ad).
pub async fn check_dnssec(domain: &str, resolver: &DnsResolver) -> (CheckResult, bool) {
    let signed = resolver.check_dnssec_signed(domain).await;

    let mut sub_checks = Vec::new();

    if signed {
        sub_checks.push(SubCheck {
            name: "ad_set".to_string(),
            verdict: Verdict::Pass,
            detail: "DNSSEC validated (RRSIG records present)".to_string(),
        });
    } else {
        sub_checks.push(SubCheck {
            name: "ad_not_set".to_string(),
            verdict: Verdict::Info,
            detail: "DNSSEC not validated; see tls.netray.info for full chain validation"
                .to_string(),
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
