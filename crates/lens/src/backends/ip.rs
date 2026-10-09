use std::net::IpAddr;
use std::time::Duration;

use serde::Deserialize;
use tracing::Instrument;

use crate::backends::{Backend, BackendContext, BackendExtra, BackendResult};
use crate::check::SectionError;
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

#[derive(Clone, Debug)]
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
    network: NetworkInfo,
    #[serde(default)]
    location: LocationInfo,
}

#[derive(Deserialize)]
struct NetworkInfo {
    #[serde(rename = "type", default)]
    network_type: String,
    org: Option<String>,
    is_spamhaus: bool,
    is_c2: bool,
    is_tor: bool,
    is_vpn: bool,
}

#[derive(Deserialize, Default)]
struct LocationInfo {
    city: Option<String>,
    country: Option<String>,
}

// ---------------------------------------------------------------------------
// Backend trait implementation
// ---------------------------------------------------------------------------

pub struct IpBackend {
    pub ip_url: String,
    pub public_url: String,
    pub timeout: Duration,
    pub client: reqwest::Client,
    pub allow: fn(IpAddr) -> bool,
}

impl Backend for IpBackend {
    fn section(&self) -> &'static str {
        "ip"
    }

    fn run(
        &self,
        _domain: &str,
        context: &BackendContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    > {
        if context.resolved_ips.is_empty() {
            return Box::pin(async { Err(SectionError::NoDnsResults) });
        }
        let client = self.client.clone();
        let ips = context.resolved_ips.clone();
        let ip_url = self.ip_url.clone();
        let public_url = self.public_url.clone();
        let timeout = self.timeout;
        let fwd = context.forward_headers.clone();
        let allow = self.allow;
        Box::pin(async move {
            let mut result = check_ip(&client, &ip_url, &ips, timeout, &fwd, allow)
                .await
                .map_err(|e| match e {
                    AppError::Timeout => SectionError::Timeout,
                    other => SectionError::BackendError(other.to_string()),
                })?;
            if result.checks.is_empty() {
                return Err(SectionError::NotApplicable {
                    reason: "no public addresses".into(),
                });
            }
            result.detail_url = public_url;
            Ok(BackendResult {
                checks: result.checks,
                extra: BackendExtra::Ip {
                    addresses: result.addresses,
                    raw_headline: result.raw_headline,
                    detail_url: result.detail_url,
                },
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Addresses queried per family; the rest of a larger answer is not looked at.
const MAX_PER_FAMILY: usize = 4;

/// Enrich the public addresses among `ips` (those `allow` accepts), at most four IPv4 and
/// four IPv6 in ascending order. An empty sample is `Ok` with no checks.
pub async fn check_ip(
    client: &reqwest::Client,
    ip_url: &str,
    ips: &[IpAddr],
    timeout: Duration,
    fwd: &reqwest::header::HeaderMap,
    allow: fn(IpAddr) -> bool,
) -> Result<IpBackendResult, AppError> {
    let base = ip_url.trim_end_matches('/');

    let (mut v4, mut v6): (Vec<IpAddr>, Vec<IpAddr>) = ips
        .iter()
        .copied()
        .filter(|ip| allow(*ip))
        .partition(IpAddr::is_ipv4);
    v4.sort();
    v6.sort();
    let sample: Vec<IpAddr> = v4
        .into_iter()
        .take(MAX_PER_FAMILY)
        .chain(v6.into_iter().take(MAX_PER_FAMILY))
        .collect();

    if sample.is_empty() {
        return Ok(IpBackendResult {
            checks: vec![],
            addresses: vec![],
            raw_headline: String::new(),
            detail_url: base.to_string(),
        });
    }

    let span = tracing::info_span!("backend_call", service = "ifconfig", url = %base, ip_count = sample.len());
    check_ip_inner(client, base, &sample, ips.len(), timeout, fwd)
        .instrument(span)
        .await
}

async fn check_ip_inner(
    client: &reqwest::Client,
    base: &str,
    sample: &[IpAddr],
    total: usize,
    timeout: Duration,
    fwd: &reqwest::header::HeaderMap,
) -> Result<IpBackendResult, AppError> {
    // Fire off concurrent requests for each IP.
    let futures: Vec<_> = sample
        .iter()
        .map(|ip| {
            let url = format!("{base}/json?ip={ip}&dns=false");
            let client = client.clone();
            let fwd = fwd.clone();
            async move {
                let result = tokio::time::timeout(timeout, async {
                    let resp = client
                        .get(&url)
                        .headers(fwd)
                        .send()
                        .await
                        .map_err(|e| enrichment_error(format!("request failed: {e}")))?;
                    if !resp.status().is_success() {
                        return Err(enrichment_error(format!("HTTP {}", resp.status())));
                    }
                    resp.json::<EnrichmentEntry>()
                        .await
                        .map_err(|e| enrichment_error(format!("undecodable response: {e}")))
                })
                .await
                .unwrap_or(Err(AppError::Timeout));
                if let Err(e) = &result {
                    tracing::warn!(service = "ifconfig", url = %url, error = %e, "enrichment call failed");
                }
                result
            }
        })
        .collect();

    let responses = futures::future::join_all(futures).await;

    let mut entries = Vec::with_capacity(responses.len());
    let mut first_err: Option<AppError> = None;
    for r in responses {
        match r {
            Ok(e) => entries.push(e),
            Err(AppError::Timeout) => first_err = Some(AppError::Timeout),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    if let Some(e) = first_err {
        return Err(e);
    }

    let mut addresses: Vec<IpInfo> = Vec::new();
    let mut worst_verdict = CheckVerdict::Pass;
    let mut reputation_messages: Vec<String> = Vec::new();

    for (ip, e) in sample.iter().zip(entries) {
        let n = &e.network;
        let mut flags: Vec<&str> = Vec::new();
        if n.is_spamhaus {
            flags.push("spamhaus");
        }
        if n.is_c2 {
            flags.push("c2");
        }
        if n.is_tor {
            flags.push("tor");
        }
        if n.is_vpn {
            flags.push("vpn");
        }
        let verdict = if n.is_spamhaus || n.is_c2 || n.is_tor {
            CheckVerdict::Fail
        } else if n.is_vpn {
            CheckVerdict::Warn
        } else {
            CheckVerdict::Pass
        };
        if verdict_rank(&verdict) > verdict_rank(&worst_verdict) {
            worst_verdict = verdict;
        }
        for flag in flags {
            reputation_messages.push(format!("{ip}: {flag}"));
        }

        addresses.push(IpInfo {
            ip: *ip,
            org: n.org.clone(),
            geo: build_geo(&e.location),
            network_type: n.network_type.clone(),
        });
    }

    if sample.len() < total {
        reputation_messages.push(format!("checked {} of {total} addresses", sample.len()));
    }

    let reputation_check = CheckResult {
        name: "reputation".to_string(),
        verdict: worst_verdict,
        messages: reputation_messages,
    };

    let raw_headline = build_headline(&addresses);
    let detail_url = build_detail_url(base, sample);

    tracing::debug!(service = "ifconfig", url = %base, enriched = addresses.len(), "backend call succeeded");
    Ok(IpBackendResult {
        checks: vec![reputation_check],
        addresses,
        raw_headline,
        detail_url,
    })
}

fn enrichment_error(message: String) -> AppError {
    AppError::BackendError {
        backend: "ip",
        message,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ip_backend_returns_no_dns_results_when_empty() {
        let backend = IpBackend {
            ip_url: "https://ip.example.com".to_string(),
            public_url: "https://ip.example.com".to_string(),
            timeout: Duration::from_secs(5),
            client: reqwest::Client::new(),
            allow: netray_common::target_policy::is_allowed_target,
        };
        let context = BackendContext {
            resolved_ips: vec![],
            dkim_selectors: None,
            forward_headers: Default::default(),
        };
        let result = backend.run("example.com", &context).await;
        assert!(
            matches!(result, Err(SectionError::NoDnsResults)),
            "expected NoDnsResults, got: {result:?}"
        );
    }
}
