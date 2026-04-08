use std::net::IpAddr;
use std::time::Duration;

use serde::Deserialize;
use tracing::Instrument;

use crate::error::AppError;
use crate::scoring::engine::{CheckResult, CheckVerdict};

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct IpBackendResult {
    pub checks: Vec<CheckResult>,
    pub addresses: Vec<IpInfo>,
    pub raw_headline: String,
    pub detail_url: String,
}

#[derive(Clone)]
pub struct IpInfo {
    pub ip: IpAddr,
    pub org: Option<String>,
    pub geo: Option<String>,
    pub network_type: String,
}

// ---------------------------------------------------------------------------
// ifconfig-rs response types (subset of what we need)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct EnrichmentEntry {
    #[serde(default)]
    network: NetworkInfo,
    #[serde(default)]
    location: LocationInfo,
}

#[derive(Deserialize, Default)]
struct NetworkInfo {
    #[serde(rename = "type", default)]
    network_type: String,
    org: Option<String>,
}

#[derive(Deserialize, Default)]
struct LocationInfo {
    city: Option<String>,
    country: Option<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Maximum number of IPs to query — cap for safety.
const MAX_IPS: usize = 5;

pub async fn check_ip(
    client: &reqwest::Client,
    ip_url: &str,
    ips: &[IpAddr],
    timeout: Duration,
) -> Result<IpBackendResult, AppError> {
    if ips.is_empty() {
        return Ok(IpBackendResult {
            checks: vec![],
            addresses: vec![],
            raw_headline: String::new(),
            detail_url: ip_url.to_string(),
        });
    }

    let capped: Vec<IpAddr> = ips.iter().copied().take(MAX_IPS).collect();
    let base = ip_url.trim_end_matches('/');

    let span = tracing::info_span!("backend_call", service = "ifconfig", url = %base, ip_count = capped.len());
    check_ip_inner(client, base, &capped, timeout)
        .instrument(span)
        .await
}

async fn check_ip_inner(
    client: &reqwest::Client,
    base: &str,
    capped: &[IpAddr],
    timeout: Duration,
) -> Result<IpBackendResult, AppError> {
    // Fire off concurrent requests for each IP.
    let futures: Vec<_> = capped
        .iter()
        .map(|ip| {
            let url = format!("{base}/json?ip={ip}");
            let client = client.clone();
            async move {
                let result = tokio::time::timeout(timeout, client.get(&url).send())
                    .await
                    .ok()
                    .and_then(|r| r.ok());
                if result.is_none() {
                    tracing::warn!(service = "ifconfig", url = %url, "enrichment call failed");
                }
                result
            }
        })
        .collect();

    let responses = futures::future::join_all(futures).await;

    let mut addresses: Vec<IpInfo> = Vec::new();
    let mut worst_verdict = CheckVerdict::Pass;
    let mut reputation_messages: Vec<String> = Vec::new();

    for (ip, maybe_resp) in capped.iter().zip(responses.into_iter()) {
        let entry = match maybe_resp {
            Some(resp) if resp.status().is_success() => resp.json::<EnrichmentEntry>().await.ok(),
            _ => None,
        };

        match entry {
            Some(e) => {
                let network_type = e.network.network_type.clone();
                let verdict = network_type_verdict(&network_type);
                if verdict_rank(&verdict) > verdict_rank(&worst_verdict) {
                    worst_verdict = verdict.clone();
                }
                if matches!(verdict, CheckVerdict::Fail | CheckVerdict::Warn) {
                    reputation_messages.push(format!("{ip}: {network_type}"));
                }

                let geo = build_geo(&e.location);
                let org = e.network.org.clone();

                addresses.push(IpInfo {
                    ip: *ip,
                    org,
                    geo,
                    network_type,
                });
            }
            None => {
                // Enrichment unavailable for this IP — include with unknown type.
                addresses.push(IpInfo {
                    ip: *ip,
                    org: None,
                    geo: None,
                    network_type: "unknown".to_string(),
                });
            }
        }
    }

    let reputation_check = CheckResult {
        name: "reputation".to_string(),
        verdict: worst_verdict,
        messages: reputation_messages,
    };

    let raw_headline = build_headline(&addresses);
    let detail_url = build_detail_url(base, capped);

    tracing::debug!(service = "ifconfig", url = %base, enriched = addresses.len(), "backend call succeeded");
    Ok(IpBackendResult {
        checks: vec![reputation_check],
        addresses,
        raw_headline,
        detail_url,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Rank verdicts so we can find the worst: higher rank = worse.
fn verdict_rank(v: &CheckVerdict) -> u8 {
    match v {
        CheckVerdict::Pass | CheckVerdict::Skip => 0,
        CheckVerdict::NotFound => 1,
        CheckVerdict::Warn => 2,
        CheckVerdict::Fail => 3,
    }
}

/// Map a network.type value to a CheckVerdict.
///
/// residential/cloud/datacenter/bot/education/government/business → Pass
/// vpn → Warn
/// tor/spamhaus/c2 → Fail
fn network_type_verdict(network_type: &str) -> CheckVerdict {
    match network_type {
        "tor" | "spamhaus" | "c2" => CheckVerdict::Fail,
        "vpn" => CheckVerdict::Warn,
        "residential" | "cloud" | "datacenter" | "bot" | "education" | "government"
        | "business" | "internal" => CheckVerdict::Pass,
        _ => CheckVerdict::Pass,
    }
}

fn build_geo(location: &LocationInfo) -> Option<String> {
    match (&location.city, &location.country) {
        (Some(city), Some(country)) => Some(format!("{city}, {country}")),
        (None, Some(country)) => Some(country.clone()),
        (Some(city), None) => Some(city.clone()),
        (None, None) => None,
    }
}

/// Build a short headline from org names and geo locations.
fn build_headline(addresses: &[IpInfo]) -> String {
    if addresses.is_empty() {
        return String::new();
    }

    // Collect unique org names (up to 3).
    let orgs: Vec<&str> = addresses
        .iter()
        .filter_map(|a| a.org.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(3)
        .collect();

    // Collect unique geo locations (up to 2).
    let geos: Vec<&str> = addresses
        .iter()
        .filter_map(|a| a.geo.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(2)
        .collect();

    match (orgs.is_empty(), geos.is_empty()) {
        (false, false) => format!("{} + {}", orgs.join(", "), geos.join(" + ")),
        (false, true) => orgs.join(", "),
        (true, false) => geos.join(" + "),
        (true, true) => format!("{} address(es)", addresses.len()),
    }
}

fn build_detail_url(ip_url: &str, ips: &[IpAddr]) -> String {
    let base = ip_url.trim_end_matches('/');
    if ips.len() == 1 {
        format!("{base}/?ip={}", ips[0])
    } else if ips.is_empty() {
        base.to_string()
    } else {
        let query = ips
            .iter()
            .map(|ip| format!("ip={ip}"))
            .collect::<Vec<_>>()
            .join("&");
        format!("{base}/?{query}")
    }
}
