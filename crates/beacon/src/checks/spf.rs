use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;

use ipnet::IpNet;

use crate::dns::DnsResolver;
use crate::quality::{Category, CheckResult, SpfFlat, SubCheck, Verdict};

/// Check SPF records for the domain.
/// Returns (CheckResult, Option<SpfFlat>, has_dash_all).
pub async fn check_spf(
    domain: &str,
    resolver: &DnsResolver,
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
            detail: format!("{} SPF records found (must be exactly one)", spf_records.len()),
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

        match body {
            _ if body == "all" => {
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
            }
            _ if body.starts_with("ip4:") || body.starts_with("ip6:") => {
                if let Some((_, cidr)) = body.split_once(':') {
                    // Add /32 or /128 if no prefix length specified
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
            }
            _ if body.starts_with("include:") => {
                if let Some(include_domain) = body.strip_prefix("include:") {
                    lookup_count = lookup_count.saturating_add(1);
                    if !visited.contains(include_domain) {
                        expand_spf(
                            include_domain,
                            resolver,
                            &mut visited,
                            1,
                            &mut lookup_count,
                            &mut void_count,
                            &mut authorized_prefixes,
                            &mut sub_checks,
                        )
                        .await;
                    } else {
                        sub_checks.push(SubCheck {
                            name: "spf_loop".to_string(),
                            verdict: Verdict::Warn,
                            detail: format!(
                                "SPF include loop detected at {}",
                                include_domain
                            ),
                        });
                    }
                }
            }
            _ if body.starts_with("redirect=") => {
                if let Some(redirect_domain) = body.strip_prefix("redirect=") {
                    lookup_count = lookup_count.saturating_add(1);
                    if !visited.contains(redirect_domain) {
                        expand_spf(
                            redirect_domain,
                            resolver,
                            &mut visited,
                            1,
                            &mut lookup_count,
                            &mut void_count,
                            &mut authorized_prefixes,
                            &mut sub_checks,
                        )
                        .await;
                    }
                }
            }
            _ if body == "a" || body.starts_with("a:") => {
                lookup_count = lookup_count.saturating_add(1);
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
            }
            _ if body.starts_with("mx:") || body == "mx" => {
                lookup_count = lookup_count.saturating_add(1);
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
            }
            _ if body.starts_with("ptr") => {
                has_ptr = true;
            }
            _ if body.starts_with("exists:") => {
                lookup_count = lookup_count.saturating_add(1);
                if let Some(exists_domain) = body.strip_prefix("exists:") {
                    let found = resolver.lookup_exists(exists_domain).await;
                    if !found {
                        void_count = void_count.saturating_add(1);
                    }
                }
            }
            _ => {} // Unknown mechanism, skip
        }
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
fn expand_spf<'a>(
    domain: &'a str,
    resolver: &'a DnsResolver,
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
