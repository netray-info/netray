use std::net::IpAddr;

use futures::future::join_all;

use super::{LOOKUP_FAILED, join_errors, lookup_failed, unmeasured_result};
use crate::dns::{AddressLookup, DnsLookup, LookupError};
use crate::quality::{Category, CheckResult, FcrdnsAnswer, SubCheck, Verdict};

/// Check Forward-Confirmed reverse DNS for each MX IP. `addr_errors` are the MX-host address
/// lookups that failed: with no MX IP the check is unmeasured, otherwise each is an Info.
/// Also returns the answer of each IP whose outcome was measured, `None` when none was.
#[tracing::instrument(skip_all, fields(category = "fcrdns"))]
pub async fn check_fcrdns(
    mx_ips: &[IpAddr],
    addr_errors: &[LookupError],
    resolver: &impl DnsLookup,
) -> (CheckResult, Option<Vec<FcrdnsAnswer>>) {
    if mx_ips.is_empty() && !addr_errors.is_empty() {
        return (
            unmeasured_result(Category::Fcrdns, join_errors(addr_errors)),
            None,
        );
    }
    if mx_ips.is_empty() {
        let sub_checks = vec![SubCheck {
            name: "no_ips".to_string(),
            verdict: Verdict::Info,
            detail: "no MX IPs to check".to_string(),
        }];
        return (
            CheckResult::new(Category::Fcrdns, sub_checks, "No MX IPs".to_string()),
            None,
        );
    }

    // Parallelize PTR lookups for all IPs
    let ptr_futures = mx_ips.iter().map(|ip| async move {
        let ptr_names = resolver.lookup_ptr(*ip).await;
        (*ip, ptr_names)
    });
    let ptr_results: Vec<(IpAddr, Result<Vec<String>, LookupError>)> = join_all(ptr_futures).await;

    // Collect all forward lookup requirements: (ip, ptr_name)
    let forward_futures: Vec<_> = ptr_results
        .iter()
        .flat_map(|(ip, ptr_names)| {
            ptr_names.iter().flatten().map(move |ptr_name| {
                let ptr_name = ptr_name.trim_end_matches('.').to_string();
                let ip = *ip;
                async move {
                    let forward_ips = resolver.lookup_ips(&ptr_name).await;
                    (ip, forward_ips)
                }
            })
        })
        .collect();

    // Group forward results by IP
    let forward_results: Vec<(IpAddr, Result<AddressLookup, LookupError>)> =
        join_all(forward_futures).await;

    let mut sub_checks = Vec::new();
    let mut unmeasured = Vec::new();
    let mut answers = Vec::new();

    for (ip, ptr_names) in &ptr_results {
        let ptr_names = match ptr_names {
            Ok(names) => names,
            Err(e) => {
                unmeasured.push(e.to_string());
                sub_checks.push(unmeasured_ip(*ip, e));
                continue;
            }
        };
        let seen = answers.iter().any(|a: &FcrdnsAnswer| a.address == *ip);
        let mut answer = |confirmed: bool| {
            if !seen {
                answers.push(FcrdnsAnswer {
                    address: *ip,
                    ptr: ptr_names.clone(),
                    confirmed,
                });
            }
        };
        if ptr_names.is_empty() {
            answer(false);
            sub_checks.push(SubCheck {
                name: "fcrdns_fail".to_string(),
                verdict: Verdict::Warn,
                detail: format!(
                    "FCrDNS failed for {}: no PTR record (required by Google/Yahoo since Feb 2024)",
                    ip
                ),
            });
            continue;
        }

        // Check if any forward lookup confirmed this IP
        let forwards = forward_results.iter().filter(|(fwd_ip, _)| fwd_ip == ip);
        let confirmed = forwards
            .clone()
            .any(|(_, fwd)| fwd.as_ref().is_ok_and(|a| a.ips.contains(ip)));
        // Unconfirmed is unmeasured when a forward lookup failed for this IP's family.
        let forward_error = forwards
            .filter_map(|(_, fwd)| match fwd {
                Ok(a) => a.missed(ip),
                Err(e) => Some(e),
            })
            .next();

        if confirmed {
            answer(true);
            sub_checks.push(SubCheck {
                name: "fcrdns_pass".to_string(),
                verdict: Verdict::Pass,
                detail: format!("FCrDNS confirmed for {}", ip),
            });
        } else if let Some(e) = forward_error {
            unmeasured.push(e.to_string());
            sub_checks.push(unmeasured_ip(*ip, e));
        } else {
            answer(false);
            sub_checks.push(SubCheck {
                name: "fcrdns_fail".to_string(),
                verdict: Verdict::Warn,
                detail: format!(
                    "FCrDNS failed for {}: PTR does not resolve back (required by Google/Yahoo since Feb 2024)",
                    ip
                ),
            });
        }
    }

    if unmeasured.len() == ptr_results.len() {
        return (
            unmeasured_result(Category::Fcrdns, unmeasured.join("; ")),
            None,
        );
    }
    sub_checks.extend(addr_errors.iter().map(lookup_failed));

    let detail = format!("FCrDNS checked for {} IP(s)", mx_ips.len());
    (
        CheckResult::new(Category::Fcrdns, sub_checks, detail),
        Some(answers),
    )
}

/// Info for an MX IP whose PTR or forward lookup failed, naming the IP and the cause.
fn unmeasured_ip(ip: IpAddr, e: &LookupError) -> SubCheck {
    SubCheck {
        name: LOOKUP_FAILED.to_string(),
        verdict: Verdict::Info,
        detail: format!("FCrDNS not checked for {ip}: {e}"),
    }
}
