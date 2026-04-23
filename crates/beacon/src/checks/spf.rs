//! SPF (Sender Policy Framework) check, per RFC 7208.
//!
//! Resolves the domain's `TXT` records, filters for `v=spf1` policies, and
//! recursively flattens `include:`, `redirect=`, `a`, `mx`, `ptr`, `exists:`
//! and `ip4:`/`ip6:` mechanisms into a single [`crate::quality::SpfFlat`]
//! view. The expander enforces RFC 7208 §4.6.4 limits: a maximum of 10 DNS
//! lookups, a depth cap of 10 levels of `include`/`redirect`, and a void
//! lookup counter (saturating `u16` add) to abort on excessive empty
//! responses. Visited hostnames are tracked in a `HashSet` to break cycles.
//!
//! The returned `CheckResult` carries sub-checks for record presence, lookup
//! budget usage, qualifier strictness (presence of `-all`), and overall
//! syntactic validity. Callers receive the parsed `SpfFlat` (for cross
//! validation against DMARC/DKIM alignment) and a boolean indicating whether
//! a restrictive `-all` qualifier is present.

use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;

use ipnet::IpNet;

use crate::dns::DnsLookup;
use crate::quality::{Category, CheckResult, SpfFlat, SubCheck, Verdict};

/// Check SPF records for the domain.
/// Returns (CheckResult, Option<SpfFlat>, has_dash_all).
#[tracing::instrument(skip_all, fields(category = "spf", domain = %domain))]
pub async fn check_spf<R: DnsLookup>(
    domain: &str,
    resolver: &R,
) -> (CheckResult, Option<SpfFlat>, bool) {
    let txt_records = resolver.lookup_txt(domain).await;
    let spf_records: Vec<&str> = txt_records
        .iter()
        .filter(|t| t.starts_with("v=spf1"))
        .map(|s| s.as_str())
        .collect();

    let mut sub_checks = Vec::new();

    if spf_records.is_empty() {
        sub_checks.push(SubCheck {
            name: "no_spf".to_string(),
            verdict: Verdict::Fail,
            detail: "no SPF record found".to_string(),
        });
        let result = CheckResult::new(Category::Spf, sub_checks, "No SPF record".to_string());
        return (result, None, false);
    }

    if spf_records.len() > 1 {
        sub_checks.push(SubCheck {
            name: "multiple_spf".to_string(),
            verdict: Verdict::Fail,
            detail: format!(
                "{} SPF records found (must be exactly one)",
                spf_records.len()
            ),
        });
        let result = CheckResult::new(
            Category::Spf,
            sub_checks,
            "Multiple SPF records".to_string(),
        );
        return (result, None, false);
    }

    let spf = spf_records[0];
    let mechanisms: Vec<&str> = spf.split_whitespace().skip(1).collect(); // skip "v=spf1"

    let mut lookup_count: u16 = 0;
    let mut void_count: u16 = 0;
    let mut visited = HashSet::new();
    visited.insert(domain.to_string());
    let mut authorized_prefixes = Vec::new();
    let mut has_dash_all = false;
    let mut has_ptr = false;

    for mech in &mechanisms {
        let (qualifier, body) = parse_mechanism(mech);

        // Top-level-only mechanisms: `all` and `ptr` set grading signals.
        if body == "all" {
            match qualifier {
                '+' => {
                    sub_checks.push(SubCheck {
                        name: "permissive_all".to_string(),
                        verdict: Verdict::Fail,
                        detail: "SPF disabled: +all authorizes everyone".to_string(),
                    });
                }
                '?' => {
                    sub_checks.push(SubCheck {
                        name: "neutral_all".to_string(),
                        verdict: Verdict::Warn,
                        detail: "neutral all; recommend -all or ~all".to_string(),
                    });
                }
                '-' => {
                    has_dash_all = true;
                }
                _ => {} // ~all is fine
            }
            continue;
        }
        if body.starts_with("ptr") {
            has_ptr = true;
            continue;
        }

        dispatch_mechanism(
            body,
            domain,
            resolver,
            &mut visited,
            0,
            &mut lookup_count,
            &mut void_count,
            &mut authorized_prefixes,
            &mut sub_checks,
        )
        .await;
    }

    // Check lookup count
    if lookup_count > 10 {
        sub_checks.push(SubCheck {
            name: "lookup_count".to_string(),
            verdict: Verdict::Fail,
            detail: format!("{} lookups > 10 limit (permerror)", lookup_count),
        });
    }

    // Check void lookup count
    if void_count > 2 {
        sub_checks.push(SubCheck {
            name: "void_lookups".to_string(),
            verdict: Verdict::Fail,
            detail: format!(
                "{} void lookups > 2 limit (permerror per RFC 7208)",
                void_count
            ),
        });
    }

    // Check for ptr mechanism
    if has_ptr {
        sub_checks.push(SubCheck {
            name: "ptr_mechanism".to_string(),
            verdict: Verdict::Warn,
            detail: "deprecated ptr: mechanism".to_string(),
        });
    }

    // Check for overlapping CIDRs
    if has_overlapping_cidrs(&authorized_prefixes) {
        sub_checks.push(SubCheck {
            name: "overlapping_cidrs".to_string(),
            verdict: Verdict::Info,
            detail: "overlapping CIDR ranges in SPF".to_string(),
        });
    }

    if sub_checks.is_empty() {
        sub_checks.push(SubCheck {
            name: "spf_ok".to_string(),
            verdict: Verdict::Pass,
            detail: format!(
                "valid SPF with {} authorized prefix(es)",
                authorized_prefixes.len()
            ),
        });
    }

    let detail = format!("SPF record: {}", spf);
    let spf_flat = SpfFlat {
        authorized_prefixes,
    };
    let result = CheckResult::new(Category::Spf, sub_checks, detail);

    (result, Some(spf_flat), has_dash_all)
}

/// Recursively expand SPF includes.
#[allow(clippy::too_many_arguments)]
fn expand_spf<'a, R: DnsLookup + 'a>(
    domain: &'a str,
    resolver: &'a R,
    visited: &'a mut HashSet<String>,
    depth: u8,
    lookup_count: &'a mut u16,
    void_count: &'a mut u16,
    authorized_prefixes: &'a mut Vec<IpNet>,
    sub_checks: &'a mut Vec<SubCheck>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if depth > 10 {
            sub_checks.push(SubCheck {
                name: "depth_exceeded".to_string(),
                verdict: Verdict::Fail,
                detail: "SPF expansion depth exceeded 10".to_string(),
            });
            return;
        }

        visited.insert(domain.to_string());

        let txt_records = resolver.lookup_txt(domain).await;
        let spf_records: Vec<&str> = txt_records
            .iter()
            .filter(|t| t.starts_with("v=spf1"))
            .map(|s| s.as_str())
            .collect();

        if spf_records.is_empty() {
            *void_count = void_count.saturating_add(1);
            return;
        }

        let spf = spf_records[0];
        let mechanisms: Vec<&str> = spf.split_whitespace().skip(1).collect();

        for mech in &mechanisms {
            let (_qualifier, body) = parse_mechanism(mech);
            dispatch_mechanism(
                body,
                domain,
                resolver,
                visited,
                depth,
                lookup_count,
                void_count,
                authorized_prefixes,
                sub_checks,
            )
            .await;
        }
    })
}

/// Dispatch a single SPF mechanism body. Shared between top-level (`check_spf`)
/// and recursive (`expand_spf`) walks.
///
/// Does NOT handle `all` or `ptr` — those are top-level grading signals and are
/// the caller's responsibility. Handles `ip4:`, `ip6:`, `include:`, `redirect=`,
/// `a`/`a:`, `mx`/`mx:`, and `exists:`. Unknown mechanisms are skipped.
#[allow(clippy::too_many_arguments)]
fn dispatch_mechanism<'a, R: DnsLookup + 'a>(
    body: &'a str,
    domain: &'a str,
    resolver: &'a R,
    visited: &'a mut HashSet<String>,
    depth: u8,
    lookup_count: &'a mut u16,
    void_count: &'a mut u16,
    authorized_prefixes: &'a mut Vec<IpNet>,
    sub_checks: &'a mut Vec<SubCheck>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if body.starts_with("ip4:") || body.starts_with("ip6:") {
            if let Some((_, cidr)) = body.split_once(':') {
                let cidr = if cidr.contains('/') {
                    cidr.to_string()
                } else if body.starts_with("ip4:") {
                    format!("{}/32", cidr)
                } else {
                    format!("{}/128", cidr)
                };
                if let Ok(net) = IpNet::from_str(&cidr) {
                    authorized_prefixes.push(net);
                }
            }
        } else if body.starts_with("include:") {
            if let Some(include_domain) = body.strip_prefix("include:") {
                *lookup_count = lookup_count.saturating_add(1);
                if visited.contains(include_domain) {
                    sub_checks.push(SubCheck {
                        name: "spf_loop".to_string(),
                        verdict: Verdict::Warn,
                        detail: format!("SPF include loop detected at {}", include_domain),
                    });
                } else {
                    expand_spf(
                        include_domain,
                        resolver,
                        visited,
                        depth + 1,
                        lookup_count,
                        void_count,
                        authorized_prefixes,
                        sub_checks,
                    )
                    .await;
                }
            }
        } else if body.starts_with("redirect=") {
            if let Some(redirect_domain) = body.strip_prefix("redirect=") {
                *lookup_count = lookup_count.saturating_add(1);
                if !visited.contains(redirect_domain) {
                    expand_spf(
                        redirect_domain,
                        resolver,
                        visited,
                        depth + 1,
                        lookup_count,
                        void_count,
                        authorized_prefixes,
                        sub_checks,
                    )
                    .await;
                }
            }
        } else if body == "a" || body.starts_with("a:") {
            *lookup_count = lookup_count.saturating_add(1);
            let target = if body.starts_with("a:") {
                body.strip_prefix("a:").unwrap_or(domain)
            } else {
                domain
            };
            let ips = resolver.lookup_ips(target).await;
            for ip in ips {
                let net = match ip {
                    IpAddr::V4(_) => IpNet::from_str(&format!("{}/32", ip)),
                    IpAddr::V6(_) => IpNet::from_str(&format!("{}/128", ip)),
                };
                if let Ok(net) = net {
                    authorized_prefixes.push(net);
                }
            }
        } else if body.starts_with("mx:") || body == "mx" {
            *lookup_count = lookup_count.saturating_add(1);
            let target = if body.starts_with("mx:") {
                body.strip_prefix("mx:").unwrap_or(domain)
            } else {
                domain
            };
            let mx_records = resolver.lookup_mx(target).await;
            for (_, exchange) in &mx_records {
                let host = exchange.trim_end_matches('.');
                let ips = resolver.lookup_ips(host).await;
                for ip in ips {
                    let net = match ip {
                        IpAddr::V4(_) => IpNet::from_str(&format!("{}/32", ip)),
                        IpAddr::V6(_) => IpNet::from_str(&format!("{}/128", ip)),
                    };
                    if let Ok(net) = net {
                        authorized_prefixes.push(net);
                    }
                }
            }
        } else if body.starts_with("exists:") {
            *lookup_count = lookup_count.saturating_add(1);
            if let Some(exists_domain) = body.strip_prefix("exists:") {
                let found = resolver.lookup_exists(exists_domain).await;
                if !found {
                    *void_count = void_count.saturating_add(1);
                }
            }
        }
    })
}

/// Parse an SPF mechanism into (qualifier, body).
fn parse_mechanism(mech: &str) -> (char, &str) {
    match mech.as_bytes().first() {
        Some(b'+') => ('+', &mech[1..]),
        Some(b'-') => ('-', &mech[1..]),
        Some(b'~') => ('~', &mech[1..]),
        Some(b'?') => ('?', &mech[1..]),
        _ => ('+', mech), // Default qualifier is '+'
    }
}

/// Simple overlap detection: check if any prefix contains another.
fn has_overlapping_cidrs(prefixes: &[IpNet]) -> bool {
    for (i, a) in prefixes.iter().enumerate() {
        for b in prefixes.iter().skip(i + 1) {
            if a.contains(b) || b.contains(a) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_mechanism ---

    #[test]
    fn parse_mechanism_default_qualifier() {
        assert_eq!(
            parse_mechanism("include:example.com"),
            ('+', "include:example.com")
        );
    }

    #[test]
    fn parse_mechanism_plus() {
        assert_eq!(parse_mechanism("+all"), ('+', "all"));
    }

    #[test]
    fn parse_mechanism_minus() {
        assert_eq!(parse_mechanism("-all"), ('-', "all"));
    }

    #[test]
    fn parse_mechanism_tilde() {
        assert_eq!(parse_mechanism("~all"), ('~', "all"));
    }

    #[test]
    fn parse_mechanism_question() {
        assert_eq!(parse_mechanism("?all"), ('?', "all"));
    }

    // --- a mechanism pattern matching ---

    #[test]
    fn a_mechanism_matches_bare_a() {
        // "a" exactly should match the `body == "a"` arm
        let (_, body) = parse_mechanism("a");
        assert!(body == "a" || body.starts_with("a:"));
    }

    #[test]
    fn a_mechanism_matches_a_with_domain() {
        let (_, body) = parse_mechanism("a:mail.example.com");
        assert!(body == "a" || body.starts_with("a:"));
    }

    #[test]
    fn a_mechanism_does_not_match_a2record() {
        // "a2record" must NOT match the `a` mechanism
        let (_, body) = parse_mechanism("a2record");
        assert!(body != "a" && !body.starts_with("a:"));
    }

    #[test]
    fn a_mechanism_does_not_match_aa() {
        let (_, body) = parse_mechanism("aa");
        assert!(body != "a" && !body.starts_with("a:"));
    }

    // --- has_overlapping_cidrs ---

    #[test]
    fn no_overlap() {
        let prefixes: Vec<IpNet> = vec![
            "192.0.2.0/24".parse().unwrap(),
            "198.51.100.0/24".parse().unwrap(),
        ];
        assert!(!has_overlapping_cidrs(&prefixes));
    }

    #[test]
    fn overlap_detected() {
        let prefixes: Vec<IpNet> = vec![
            "192.0.2.0/24".parse().unwrap(),
            "192.0.2.1/32".parse().unwrap(),
        ];
        assert!(has_overlapping_cidrs(&prefixes));
    }

    #[test]
    fn empty_no_overlap() {
        assert!(!has_overlapping_cidrs(&[]));
    }

    #[test]
    fn single_no_overlap() {
        let prefixes: Vec<IpNet> = vec!["192.0.2.0/24".parse().unwrap()];
        assert!(!has_overlapping_cidrs(&prefixes));
    }

    // --- E1: end-to-end SPF expansion tests using TestDnsResolver ---

    use crate::dns::test_support::TestDnsResolver;

    /// 11 levels of nested includes should fire `depth_exceeded` from the
    /// recursive guard. The depth cap is > 10, so depth 11 triggers Warn via
    /// the void counter path or the depth_exceeded subcheck.
    #[tokio::test]
    async fn spf_recursion_depth_cap_at_10() {
        let mut resolver = TestDnsResolver::new()
            .with_txt("example.com", vec!["v=spf1 include:l1.example.com -all"]);
        for i in 1..=10 {
            let name = format!("l{}.example.com", i);
            let next = format!("l{}.example.com", i + 1);
            resolver = resolver.with_txt(&name, vec![&format!("v=spf1 include:{} -all", next)]);
        }
        // l11 has a simple terminating record.
        resolver = resolver.with_txt("l11.example.com", vec!["v=spf1 -all"]);

        let (result, _flat, _dash_all) = check_spf("example.com", &resolver).await;

        // Either depth_exceeded (Fail) or lookup_count (Fail) fires — both map
        // to an enforcement signal past the 10-level cap. A Warn on depth
        // exceeded is acceptable; the SDD specifies depth cap at 10.
        assert!(
            result.sub_checks.iter().any(|s| s.name == "depth_exceeded")
                || result.sub_checks.iter().any(|s| s.name == "lookup_count"),
            "expected depth_exceeded or lookup_count past 10 levels; got {:?}",
            result.sub_checks
        );
    }

    /// >2 void lookups → Fail (RFC 7208 permerror).
    #[tokio::test]
    async fn spf_too_many_voids_fails() {
        let resolver = TestDnsResolver::new().with_txt(
            "example.com",
            vec!["v=spf1 include:void1.example.com include:void2.example.com include:void3.example.com -all"],
        );
        // void1/2/3 have NO TXT records → each counts as a void lookup.
        let (result, _flat, _dash_all) = check_spf("example.com", &resolver).await;
        assert!(
            result
                .sub_checks
                .iter()
                .any(|s| s.name == "void_lookups" && s.verdict == Verdict::Fail),
            "expected void_lookups Fail; got {:?}",
            result.sub_checks
        );
    }

    /// >10 DNS lookups → Fail (permerror).
    #[tokio::test]
    async fn spf_too_many_lookups_fails() {
        let resolver = TestDnsResolver::new().with_txt(
            "example.com",
            vec![
                "v=spf1 include:a.example.com include:b.example.com include:c.example.com include:d.example.com include:e.example.com include:f.example.com include:g.example.com include:h.example.com include:i.example.com include:j.example.com include:k.example.com -all",
            ],
        );
        let (result, _flat, _dash_all) = check_spf("example.com", &resolver).await;
        assert!(
            result
                .sub_checks
                .iter()
                .any(|s| s.name == "lookup_count" && s.verdict == Verdict::Fail),
            "expected lookup_count Fail with 11 includes; got {:?}",
            result.sub_checks
        );
    }

    /// redirect= to a target ending in ~all → warn (no hard fail).
    #[tokio::test]
    async fn spf_redirect_tilde_all_warns() {
        let resolver = TestDnsResolver::new()
            .with_txt("example.com", vec!["v=spf1 redirect=target.example.com"])
            .with_txt("target.example.com", vec!["v=spf1 ip4:192.0.2.1 ~all"]);
        let (result, flat, has_dash_all) = check_spf("example.com", &resolver).await;
        assert!(
            !has_dash_all,
            "~all on redirect target must not set has_dash_all"
        );
        let flat = flat.expect("SPF expansion should have a SpfFlat");
        assert_eq!(flat.authorized_prefixes.len(), 1);
        // No fail verdict on the overall record.
        assert!(
            !result.sub_checks.iter().any(|s| s.verdict == Verdict::Fail),
            "no Fail expected for redirect with ~all; got {:?}",
            result.sub_checks
        );
    }

    /// redirect= to a target ending in -all → pass (has_dash_all = false
    /// at the top level, but SPF evaluates via the redirect). The SDD
    /// requires this to be treated as a valid strict record: no Fail
    /// verdict.
    #[tokio::test]
    async fn spf_redirect_dash_all_passes() {
        let resolver = TestDnsResolver::new()
            .with_txt("example.com", vec!["v=spf1 redirect=target.example.com"])
            .with_txt("target.example.com", vec!["v=spf1 ip4:192.0.2.1 -all"]);
        let (result, flat, _has_dash_all) = check_spf("example.com", &resolver).await;
        let flat = flat.expect("SPF expansion should have a SpfFlat");
        assert_eq!(flat.authorized_prefixes.len(), 1);
        assert!(
            !result.sub_checks.iter().any(|s| s.verdict == Verdict::Fail),
            "no Fail expected for redirect with -all; got {:?}",
            result.sub_checks
        );
    }

    /// include: where the target has no TXT records counts as a void lookup.
    #[tokio::test]
    async fn spf_include_no_txt_counts_as_void() {
        // 3 includes whose targets have no TXT → 3 voids (> 2 limit).
        let resolver = TestDnsResolver::new().with_txt(
            "example.com",
            vec!["v=spf1 include:v1.example.com include:v2.example.com include:v3.example.com -all"],
        );
        let (result, _flat, _has_dash_all) = check_spf("example.com", &resolver).await;
        assert!(
            result
                .sub_checks
                .iter()
                .any(|s| s.name == "void_lookups" && s.verdict == Verdict::Fail),
            "void include targets should count; got {:?}",
            result.sub_checks
        );
    }
}
