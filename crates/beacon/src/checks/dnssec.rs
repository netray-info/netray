use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, SubCheck, Verdict};

/// Check DNSSEC for the domain.
/// Returns (CheckResult, dnssec_dnskey_present).
#[tracing::instrument(skip_all, fields(category = "dnssec", domain = %domain))]
pub async fn check_dnssec(domain: &str, resolver: &impl DnsLookup) -> (CheckResult, bool) {
    let signed = resolver.check_dnssec_signed(domain).await;

    let mut sub_checks = Vec::new();

    if signed {
        sub_checks.push(SubCheck {
            name: "ad_set".to_string(),
            verdict: Verdict::Pass,
            detail: "DNSSEC signed (DNSKEY records present)".to_string(),
        });
    } else {
        sub_checks.push(SubCheck {
            name: "ad_not_set".to_string(),
            verdict: Verdict::Info,
            detail: "DNSSEC not validated".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::test_support::TestDnsResolver;

    #[tokio::test]
    async fn dnskey_present_passes() {
        let resolver = TestDnsResolver::new().with_dnssec_signed("example.com");
        let (result, present) = check_dnssec("example.com", &resolver).await;
        assert!(present, "dnssec_dnskey_present should be true");
        assert_eq!(result.sub_checks.len(), 1);
        assert_eq!(result.sub_checks[0].name, "ad_set");
        assert_eq!(result.sub_checks[0].verdict, Verdict::Pass);
    }

    #[tokio::test]
    async fn dnskey_absent_returns_info() {
        let resolver = TestDnsResolver::new();
        let (result, present) = check_dnssec("example.com", &resolver).await;
        assert!(!present, "dnssec_dnskey_present should be false");
        assert_eq!(result.sub_checks.len(), 1);
        assert_eq!(result.sub_checks[0].name, "ad_not_set");
        assert_eq!(result.sub_checks[0].verdict, Verdict::Info);
    }
}

