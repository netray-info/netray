use crate::quality::{AllResults, Category, CheckResult, SubCheck, Verdict};

/// Run all cross-validation rules against collected results.
pub fn cross_validate(results: &AllResults) -> CheckResult {
    let mut sub_checks: Vec<SubCheck> = Vec::new();

    if let Some(sc) = check_dane_without_dnssec(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_mta_sts_without_tls_rpt(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_dane_without_tls_rpt(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_spf_mx_coverage(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_bimi_dmarc_policy(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_null_mx_spf(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_reject_no_dkim(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_mta_sts_id_mismatch(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_mta_sts_mx_coverage(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_dmarc_rua_auth(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_dmarc_sp_gap(results) {
        sub_checks.push(sc);
    }
    if let Some(sc) = check_fcrdns_mismatch(results) {
        sub_checks.push(sc);
    }

    let detail = if sub_checks.is_empty() {
        "all cross-validation checks passed".to_string()
    } else {
        format!("{} cross-validation issue(s)", sub_checks.len())
    };

    CheckResult::new(Category::CrossValidation, sub_checks, detail)
}

fn check_dane_without_dnssec(r: &AllResults) -> Option<SubCheck> {
    if r.dane_has_tlsa && !r.dnssec_ad {
        Some(SubCheck {
            name: "dane_without_dnssec".to_string(),
            verdict: Verdict::Fail,
            detail: "DANE TLSA records present but DNSSEC not validated".to_string(),
        })
    } else {
        None
    }
}

fn check_mta_sts_without_tls_rpt(r: &AllResults) -> Option<SubCheck> {
    if r.mta_sts_present && !r.tls_rpt_present {
        Some(SubCheck {
            name: "mta_sts_without_tls_rpt".to_string(),
            verdict: Verdict::Warn,
            detail: "MTA-STS enabled but no TLS-RPT record for reporting".to_string(),
        })
    } else {
        None
    }
}

fn check_dane_without_tls_rpt(r: &AllResults) -> Option<SubCheck> {
    if r.dane_has_tlsa && !r.tls_rpt_present {
        Some(SubCheck {
            name: "dane_without_tls_rpt".to_string(),
            verdict: Verdict::Warn,
            detail: "DANE TLSA present but no TLS-RPT record for reporting".to_string(),
        })
    } else {
        None
    }
}

fn check_spf_mx_coverage(r: &AllResults) -> Option<SubCheck> {
    let spf_flat = r.spf_flat.as_ref()?;
    if r.mx_ips.is_empty() {
        return None;
    }

    let uncovered: Vec<_> = r
        .mx_ips
        .iter()
        .filter(|ip| {
            !spf_flat
                .authorized_prefixes
                .iter()
                .any(|net| net.contains(*ip))
        })
        .collect();

    if uncovered.is_empty() {
        None
    } else {
        Some(SubCheck {
            name: "spf_mx_coverage".to_string(),
            verdict: Verdict::Fail,
            detail: format!(
                "MX IP(s) not in SPF: {}",
                uncovered
                    .iter()
                    .map(|ip| ip.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
    }
}

fn check_bimi_dmarc_policy(r: &AllResults) -> Option<SubCheck> {
    if !r.bimi_present {
        return None;
    }
    match r.dmarc_policy.as_deref() {
        Some("quarantine") | Some("reject") => None,
        _ => Some(SubCheck {
            name: "bimi_dmarc_policy".to_string(),
            verdict: Verdict::Warn,
            detail: "BIMI present but DMARC policy is not quarantine or reject".to_string(),
        }),
    }
}

fn check_null_mx_spf(r: &AllResults) -> Option<SubCheck> {
    if r.null_mx && !r.spf_has_dash_all {
        Some(SubCheck {
            name: "null_mx_spf".to_string(),
            verdict: Verdict::Warn,
            detail: "Null MX but SPF does not contain -all".to_string(),
        })
    } else {
        None
    }
}

fn check_reject_no_dkim(r: &AllResults) -> Option<SubCheck> {
    if r.dmarc_policy.as_deref() == Some("reject") && r.spf_has_dash_all && !r.dkim_found {
        Some(SubCheck {
            name: "reject_no_dkim".to_string(),
            verdict: Verdict::Warn,
            detail: "DMARC p=reject and SPF -all but no DKIM keys found".to_string(),
        })
    } else {
        None
    }
}

fn check_mta_sts_id_mismatch(r: &AllResults) -> Option<SubCheck> {
    let info = r.mta_sts_info.as_ref()?;
    let policy_id = info.policy_id.as_ref()?;
    if info.dns_id != *policy_id {
        Some(SubCheck {
            name: "mta_sts_id_mismatch".to_string(),
            verdict: Verdict::Fail,
            detail: format!(
                "MTA-STS DNS id ({}) != policy id ({})",
                info.dns_id, policy_id
            ),
        })
    } else {
        None
    }
}

fn check_mta_sts_mx_coverage(r: &AllResults) -> Option<SubCheck> {
    let info = r.mta_sts_info.as_ref()?;
    if info.mx_patterns.is_empty() || r.mx_hosts.is_empty() {
        return None;
    }

    let uncovered: Vec<_> = r
        .mx_hosts
        .iter()
        .filter(|host| {
            let h = host.trim_end_matches('.');
            !info
                .mx_patterns
                .iter()
                .any(|pat| mta_sts_glob_match(pat, h))
        })
        .collect();

    if uncovered.is_empty() {
        None
    } else {
        Some(SubCheck {
            name: "mta_sts_mx_coverage".to_string(),
            verdict: Verdict::Warn,
            detail: format!(
                "MX host(s) not covered by MTA-STS policy: {}",
                uncovered
                    .iter()
                    .map(|h| h.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
    }
}

/// Simple glob match for MTA-STS mx: patterns (only `*.` prefix wildcard).
fn mta_sts_glob_match(pattern: &str, hostname: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        hostname.ends_with(suffix) && hostname.len() > suffix.len() + 1
    } else {
        pattern.eq_ignore_ascii_case(hostname)
    }
}

fn check_dmarc_rua_auth(r: &AllResults) -> Option<SubCheck> {
    if !r.dmarc_rua_external_auth_ok {
        Some(SubCheck {
            name: "dmarc_rua_auth".to_string(),
            verdict: Verdict::Warn,
            detail: "external DMARC report destination not authorized".to_string(),
        })
    } else {
        None
    }
}

fn check_dmarc_sp_gap(r: &AllResults) -> Option<SubCheck> {
    if r.dmarc_policy.as_deref() == Some("reject") && r.dmarc_sp.as_deref() == Some("none") {
        Some(SubCheck {
            name: "dmarc_sp_gap".to_string(),
            verdict: Verdict::Warn,
            detail: "DMARC p=reject but sp=none leaves subdomains unprotected".to_string(),
        })
    } else {
        None
    }
}

fn check_fcrdns_mismatch(r: &AllResults) -> Option<SubCheck> {
    if !r.fcrdns_all_pass && !r.mx_ips.is_empty() {
        Some(SubCheck {
            name: "fcrdns_mismatch".to_string(),
            verdict: Verdict::Warn,
            detail: "one or more MX IPs failed forward-confirmed reverse DNS".to_string(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::{MtaStsInfo, SpfFlat};

    fn empty_result(cat: Category) -> CheckResult {
        CheckResult::new(cat, Vec::new(), String::new())
    }

    fn base_results() -> AllResults {
        AllResults {
            mx: empty_result(Category::Mx),
            mx_hosts: Vec::new(),
            mx_ips: Vec::new(),
            null_mx: false,
            spf: empty_result(Category::Spf),
            spf_flat: None,
            spf_has_dash_all: false,
            dkim: empty_result(Category::Dkim),
            dkim_found: false,
            dmarc: empty_result(Category::Dmarc),
            dmarc_policy: None,
            dmarc_sp: None,
            dmarc_rua_external_auth_ok: true,
            mta_sts: empty_result(Category::MtaSts),
            mta_sts_present: false,
            mta_sts_info: None,
            tls_rpt: empty_result(Category::TlsRpt),
            tls_rpt_present: false,
            dane: empty_result(Category::Dane),
            dane_has_tlsa: false,
            dnssec: empty_result(Category::Dnssec),
            dnssec_ad: false,
            bimi: empty_result(Category::Bimi),
            bimi_present: false,
            fcrdns: empty_result(Category::Fcrdns),
            fcrdns_all_pass: true,
            dnsbl: empty_result(Category::Dnsbl),
        }
    }

    #[test]
    fn dane_without_dnssec() {
        let mut r = base_results();
        r.dane_has_tlsa = true;
        r.dnssec_ad = false;
        let result = cross_validate(&r);
        let names: Vec<&str> = result.sub_checks.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"dane_without_dnssec"));
        assert_eq!(
            result
                .sub_checks
                .iter()
                .find(|s| s.name == "dane_without_dnssec")
                .unwrap()
                .verdict,
            Verdict::Fail
        );
    }

    #[test]
    fn mta_sts_id_mismatch() {
        let mut r = base_results();
        r.mta_sts_present = true;
        r.mta_sts_info = Some(MtaStsInfo {
            dns_id: "abc".to_string(),
            policy_id: Some("xyz".to_string()),
            mode: Some("enforce".to_string()),
            mx_patterns: Vec::new(),
        });
        let result = cross_validate(&r);
        let names: Vec<&str> = result.sub_checks.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"mta_sts_id_mismatch"));
    }

    #[test]
    fn null_mx_without_dash_all() {
        let mut r = base_results();
        r.null_mx = true;
        r.spf_has_dash_all = false;
        let result = cross_validate(&r);
        let names: Vec<&str> = result.sub_checks.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"null_mx_spf"));
    }

    #[test]
    fn fcrdns_mismatch_fires() {
        let mut r = base_results();
        r.mx_ips = vec!["192.0.2.1".parse().unwrap()];
        r.fcrdns_all_pass = false;
        let result = cross_validate(&r);
        let names: Vec<&str> = result.sub_checks.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"fcrdns_mismatch"));
    }

    #[test]
    fn bimi_dmarc_policy_none() {
        let mut r = base_results();
        r.bimi_present = true;
        r.dmarc_policy = Some("none".to_string());
        let result = cross_validate(&r);
        let names: Vec<&str> = result.sub_checks.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"bimi_dmarc_policy"));
    }

    #[test]
    fn dmarc_sp_gap_fires() {
        let mut r = base_results();
        r.dmarc_policy = Some("reject".to_string());
        r.dmarc_sp = Some("none".to_string());
        let result = cross_validate(&r);
        let names: Vec<&str> = result.sub_checks.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"dmarc_sp_gap"));
    }

    #[test]
    fn all_clean_no_issues() {
        let r = base_results();
        let result = cross_validate(&r);
        assert!(result.sub_checks.is_empty());
        assert_eq!(result.verdict, Verdict::Pass);
    }

    #[test]
    fn mta_sts_glob_match_works() {
        assert!(mta_sts_glob_match("*.example.com", "mail.example.com"));
        assert!(!mta_sts_glob_match("*.example.com", "example.com"));
        assert!(mta_sts_glob_match("mail.example.com", "mail.example.com"));
        assert!(!mta_sts_glob_match("mail.example.com", "other.example.com"));
    }

    #[test]
    fn mta_sts_mx_coverage_missing() {
        let mut r = base_results();
        r.mx_hosts = vec!["mx.example.com".to_string()];
        r.mta_sts_info = Some(MtaStsInfo {
            dns_id: "v1".to_string(),
            policy_id: Some("v1".to_string()),
            mode: Some("enforce".to_string()),
            mx_patterns: vec!["other.example.com".to_string()],
        });
        let result = cross_validate(&r);
        let names: Vec<&str> = result.sub_checks.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"mta_sts_mx_coverage"));
        assert_eq!(
            result
                .sub_checks
                .iter()
                .find(|s| s.name == "mta_sts_mx_coverage")
                .unwrap()
                .verdict,
            Verdict::Warn
        );
    }

    #[test]
    fn test_dmarc_sp_gap_detected() {
        let mut r = base_results();
        r.dmarc_policy = Some("reject".to_string());
        r.dmarc_sp = Some("none".to_string());
        let result = cross_validate(&r);
        let sc = result
            .sub_checks
            .iter()
            .find(|s| s.name == "dmarc_sp_gap")
            .unwrap();
        assert_eq!(sc.verdict, Verdict::Warn);
        assert!(sc.detail.contains("subdomains"));
    }
}
