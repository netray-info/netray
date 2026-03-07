//! DNS delegation walk for the trace endpoint.
//!
//! Performs iterative resolution from root servers without recursion (RD=0),
//! following the delegation chain from root → TLD → authoritative nameservers.
//!
//! Inspired by the algorithm in mhost's `resolver::raw` and `app::trace` modules.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use hickory_proto::rr::{Name, RecordType};
use serde::Serialize;

use crate::dns_raw::{
    DnsRecord, RawQueryResult, ROOT_SERVERS,
    build_server_list, parallel_queries, record_to_dns_record, resolve_missing_glue,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum TraceError {
    InvalidDomain(String),
    InvalidRecordType(String),
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceError::InvalidDomain(s) => write!(f, "invalid domain: {s}"),
            TraceError::InvalidRecordType(s) => write!(f, "invalid record type: {s}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Public data model (serialised into SSE events)
// ---------------------------------------------------------------------------

/// A single delegation hop in the trace.
#[derive(Debug, Clone, Serialize)]
pub struct TraceHop {
    pub level: usize,
    pub zone: String,
    pub servers_queried: usize,
    pub server_results: Vec<ServerResult>,
    pub referral_groups: Vec<ReferralGroup>,
    pub is_final: bool,
}

/// One server's outcome within a hop.
#[derive(Debug, Clone, Serialize)]
pub struct ServerResult {
    pub server_ip: IpAddr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    pub latency_ms: f64,
    pub outcome: ServerOutcome,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub answer_records: Vec<DnsRecord>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub referral_ns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority_zone: Option<String>,
}

/// Outcome classification for a single server response.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerOutcome {
    Referral,
    Answer,
    Error { message: String },
}

/// A group of servers that returned the same NS referral set.
#[derive(Debug, Clone, Serialize)]
pub struct ReferralGroup {
    pub ns_names: Vec<String>,
    pub servers: Vec<IpAddr>,
    pub is_majority: bool,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Parse and validate a domain name string for tracing.
pub fn parse_name(domain: &str) -> Result<Name, TraceError> {
    Name::from_str(domain).map_err(|e| TraceError::InvalidDomain(e.to_string()))
}

/// Parse a record type string (e.g. "A", "AAAA", "MX").
pub fn parse_record_type(s: &str) -> Result<RecordType, TraceError> {
    RecordType::from_str(s).map_err(|_| TraceError::InvalidRecordType(s.to_owned()))
}

/// Walk the DNS delegation chain for `name`/`record_type`, emitting `TraceHop`
/// values to the returned `Vec`. Stops when an authoritative answer is received
/// or `max_hops` is reached.
pub async fn walk(
    name: Name,
    record_type: RecordType,
    max_hops: usize,
    query_timeout: Duration,
) -> Vec<TraceHop> {
    // Start with all IPv4 root servers.
    let mut current_servers: Vec<(SocketAddr, Option<String>)> = ROOT_SERVERS
        .iter()
        .map(|ip| (SocketAddr::new(IpAddr::V4(*ip), 53), None))
        .collect();

    let mut current_zone = ".".to_string();
    let mut hops = Vec::new();

    for level in 1..=max_hops {
        if current_servers.is_empty() {
            tracing::warn!(level, "no servers to query at hop");
            break;
        }

        let server_addrs: Vec<SocketAddr> = current_servers.iter().map(|(addr, _)| *addr).collect();
        let server_names: HashMap<SocketAddr, Option<String>> =
            current_servers.iter().cloned().collect();

        tracing::debug!(
            level,
            servers = server_addrs.len(),
            zone = %current_zone,
            "trace hop"
        );

        let results = parallel_queries(&server_addrs, &name, record_type, query_timeout).await;

        let (server_results, next_servers, is_final) =
            process_hop(&results, &server_names);

        let referral_groups = compute_referral_groups(&server_results);

        // Determine zone for the next hop from the authority section of a referral.
        let next_zone = server_results
            .iter()
            .find_map(|sr| sr.authority_zone.clone())
            .unwrap_or_else(|| current_zone.clone());

        let hop = TraceHop {
            level,
            zone: current_zone.clone(),
            servers_queried: server_addrs.len(),
            server_results,
            referral_groups,
            is_final,
        };

        hops.push(hop);

        if is_final {
            tracing::debug!(level, "authoritative answer received");
            break;
        }

        if next_servers.is_empty() {
            tracing::warn!(level, "no referral servers at hop");
            break;
        }

        // Resolve NS names that have no glue IP addresses.
        let mut resolved = next_servers;
        resolve_missing_glue(&mut resolved).await;

        // Build next hop server list (IPv4 only — avoids IPv6 connectivity issues).
        current_servers = build_server_list(&resolved, |ip| ip.is_ipv4());

        current_zone = next_zone;
    }

    hops
}

// ---------------------------------------------------------------------------
// Internal: hop processing
// ---------------------------------------------------------------------------

fn process_hop(
    results: &[RawQueryResult],
    server_names: &HashMap<SocketAddr, Option<String>>,
) -> (
    Vec<ServerResult>,
    HashMap<String, Vec<IpAddr>>, // ns_name → glue IPs (empty = needs resolution)
    bool,                         // is_final
) {
    let mut server_results = Vec::new();
    let mut next_servers: HashMap<String, Vec<IpAddr>> = HashMap::new();
    let mut is_final = false;

    for rqr in results {
        let server_name = server_names.get(&rqr.server).cloned().flatten();

        match &rqr.result {
            Ok(response) => {
                if response.is_authoritative() {
                    // Authoritative — either has answers (NOERROR) or is NXDOMAIN/NODATA.
                    let answer_records = response
                        .answers()
                        .iter()
                        .map(record_to_dns_record)
                        .collect();

                    let rcode = response.response_code();
                    tracing::debug!(
                        server = %rqr.server,
                        authoritative = true,
                        rcode = ?rcode,
                        "response"
                    );

                    server_results.push(ServerResult {
                        server_ip: rqr.server.ip(),
                        server_name,
                        latency_ms: response.latency.as_secs_f64() * 1000.0,
                        outcome: ServerOutcome::Answer,
                        answer_records,
                        referral_ns: Vec::new(),
                        authority_zone: None,
                    });
                    is_final = true;
                } else {
                    // Non-authoritative — extract NS referral and glue.
                    let ns_names = response.referral_ns_names();
                    if ns_names.is_empty() {
                        // No NS in authority — treat as error.
                        server_results.push(ServerResult {
                            server_ip: rqr.server.ip(),
                            server_name,
                            latency_ms: response.latency.as_secs_f64() * 1000.0,
                            outcome: ServerOutcome::Error {
                                message: "no NS records in authority section".to_owned(),
                            },
                            answer_records: Vec::new(),
                            referral_ns: Vec::new(),
                            authority_zone: None,
                        });
                        continue;
                    }

                    let glue = response.glue_ips();
                    let referral_ns: Vec<String> =
                        ns_names.iter().map(|n| n.to_ascii()).collect();

                    let authority_zone = response
                        .authority()
                        .iter()
                        .find(|r| r.record_type() == RecordType::NS)
                        .map(|r| r.name().to_ascii());

                    // Collect glue IPs per NS name (empty Vec = needs resolution).
                    for ns_name in &ns_names {
                        let ips: Vec<IpAddr> = glue
                            .iter()
                            .filter(|(name, _)| name == ns_name)
                            .map(|(_, ip)| *ip)
                            .collect();
                        let entry = next_servers.entry(ns_name.to_ascii()).or_default();
                        for ip in ips {
                            if !entry.contains(&ip) {
                                entry.push(ip);
                            }
                        }
                    }

                    server_results.push(ServerResult {
                        server_ip: rqr.server.ip(),
                        server_name,
                        latency_ms: response.latency.as_secs_f64() * 1000.0,
                        outcome: ServerOutcome::Referral,
                        answer_records: Vec::new(),
                        referral_ns,
                        authority_zone,
                    });
                }
            }
            Err(e) => {
                server_results.push(ServerResult {
                    server_ip: rqr.server.ip(),
                    server_name,
                    latency_ms: 0.0,
                    outcome: ServerOutcome::Error {
                        message: e.to_string(),
                    },
                    answer_records: Vec::new(),
                    referral_ns: Vec::new(),
                    authority_zone: None,
                });
            }
        }
    }

    (server_results, next_servers, is_final)
}

/// Group servers by their referral NS set; compute majority flag.
fn compute_referral_groups(server_results: &[ServerResult]) -> Vec<ReferralGroup> {
    let mut groups: HashMap<Vec<String>, Vec<IpAddr>> = HashMap::new();

    for sr in server_results {
        if !matches!(sr.outcome, ServerOutcome::Referral) {
            continue;
        }
        let mut ns_names = sr.referral_ns.clone();
        ns_names.sort();
        groups.entry(ns_names).or_default().push(sr.server_ip);
    }

    let total_referrals: usize = groups.values().map(|v| v.len()).sum();
    let mut referral_groups: Vec<ReferralGroup> = groups
        .into_iter()
        .map(|(ns_names, servers)| {
            let is_majority = total_referrals > 0 && servers.len() * 2 > total_referrals;
            ReferralGroup { ns_names, servers, is_majority }
        })
        .collect();

    // Sort: majority first, then by group size descending.
    referral_groups.sort_by(|a, b| {
        b.is_majority
            .cmp(&a.is_majority)
            .then_with(|| b.servers.len().cmp(&a.servers.len()))
    });

    referral_groups
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn parse_record_type_valid() {
        assert_eq!(parse_record_type("A").unwrap(), RecordType::A);
        assert_eq!(parse_record_type("AAAA").unwrap(), RecordType::AAAA);
        assert_eq!(parse_record_type("MX").unwrap(), RecordType::MX);
        assert_eq!(parse_record_type("NS").unwrap(), RecordType::NS);
        assert_eq!(parse_record_type("TXT").unwrap(), RecordType::TXT);
    }

    #[test]
    fn parse_record_type_invalid() {
        assert!(parse_record_type("BOGUS").is_err());
    }

    #[test]
    fn compute_referral_groups_single_group() {
        let results = vec![
            ServerResult {
                server_ip: IpAddr::V4(Ipv4Addr::new(198, 41, 0, 4)),
                server_name: None,
                latency_ms: 15.0,
                outcome: ServerOutcome::Referral,
                answer_records: Vec::new(),
                referral_ns: vec!["a.gtld-servers.net.".to_owned()],
                authority_zone: Some("com.".to_owned()),
            },
            ServerResult {
                server_ip: IpAddr::V4(Ipv4Addr::new(170, 247, 170, 2)),
                server_name: None,
                latency_ms: 20.0,
                outcome: ServerOutcome::Referral,
                answer_records: Vec::new(),
                referral_ns: vec!["a.gtld-servers.net.".to_owned()],
                authority_zone: Some("com.".to_owned()),
            },
        ];

        let groups = compute_referral_groups(&results);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].servers.len(), 2);
        assert!(groups[0].is_majority);
    }

    #[test]
    fn compute_referral_groups_divergence() {
        let results = vec![
            ServerResult {
                server_ip: IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
                server_name: None,
                latency_ms: 15.0,
                outcome: ServerOutcome::Referral,
                answer_records: Vec::new(),
                referral_ns: vec!["ns1.example.com.".to_owned()],
                authority_zone: Some("example.com.".to_owned()),
            },
            ServerResult {
                server_ip: IpAddr::V4(Ipv4Addr::new(2, 2, 2, 2)),
                server_name: None,
                latency_ms: 20.0,
                outcome: ServerOutcome::Referral,
                answer_records: Vec::new(),
                referral_ns: vec!["ns2.example.com.".to_owned()],
                authority_zone: Some("example.com.".to_owned()),
            },
        ];

        let groups = compute_referral_groups(&results);
        assert_eq!(groups.len(), 2);
        // Exactly split — neither is majority.
        assert!(!groups[0].is_majority);
        assert!(!groups[1].is_majority);
    }

    #[test]
    fn compute_referral_groups_skips_answers() {
        let results = vec![ServerResult {
            server_ip: IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            server_name: None,
            latency_ms: 5.0,
            outcome: ServerOutcome::Answer,
            answer_records: Vec::new(),
            referral_ns: Vec::new(),
            authority_zone: None,
        }];

        let groups = compute_referral_groups(&results);
        assert!(groups.is_empty());
    }
}
