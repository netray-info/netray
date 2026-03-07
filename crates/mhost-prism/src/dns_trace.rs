//! DNS delegation walk for the trace endpoint.
//!
//! Performs iterative resolution from root servers without recursion (RD=0),
//! following the delegation chain from root → TLD → authoritative nameservers.
//!
//! Inspired by the algorithm in mhost's `resolver::raw` and `app::trace` modules.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

// ---------------------------------------------------------------------------
// Root servers (IPv4, a–m.root-servers.net)
// ---------------------------------------------------------------------------

static ROOT_SERVERS: &[Ipv4Addr] = &[
    Ipv4Addr::new(198, 41, 0, 4),     // a.root-servers.net
    Ipv4Addr::new(170, 247, 170, 2),  // b.root-servers.net
    Ipv4Addr::new(192, 33, 4, 12),    // c.root-servers.net
    Ipv4Addr::new(199, 7, 91, 13),    // d.root-servers.net
    Ipv4Addr::new(192, 203, 230, 10), // e.root-servers.net
    Ipv4Addr::new(192, 5, 5, 241),    // f.root-servers.net
    Ipv4Addr::new(192, 112, 36, 4),   // g.root-servers.net
    Ipv4Addr::new(198, 97, 190, 53),  // h.root-servers.net
    Ipv4Addr::new(192, 36, 148, 17),  // i.root-servers.net
    Ipv4Addr::new(192, 58, 128, 30),  // j.root-servers.net
    Ipv4Addr::new(193, 0, 14, 129),   // k.root-servers.net
    Ipv4Addr::new(199, 7, 83, 42),    // l.root-servers.net
    Ipv4Addr::new(202, 12, 27, 33),   // m.root-servers.net
];

// Concurrent query limit per hop (avoids overwhelming nameservers).
const MAX_CONCURRENT: usize = 8;

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

// Internal low-level error — not exposed to callers.
#[derive(Debug)]
enum RawError {
    Io(std::io::Error),
    Decode(String),
    Timeout(Duration),
    IdMismatch { expected: u16, got: u16 },
}

impl std::fmt::Display for RawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RawError::Io(e) => write!(f, "IO error: {e}"),
            RawError::Decode(s) => write!(f, "decode error: {s}"),
            RawError::Timeout(d) => write!(f, "timeout after {d:.0?}"),
            RawError::IdMismatch { expected, got } => {
                write!(f, "ID mismatch: expected {expected}, got {got}")
            }
        }
    }
}

impl From<std::io::Error> for RawError {
    fn from(e: std::io::Error) -> Self {
        RawError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Raw DNS response wrapper
// ---------------------------------------------------------------------------

struct RawResponse {
    message: Message,
    latency: Duration,
}

impl RawResponse {
    fn answers(&self) -> &[hickory_proto::rr::Record] {
        self.message.answers()
    }

    fn authority(&self) -> &[hickory_proto::rr::Record] {
        self.message.name_servers()
    }

    fn additional(&self) -> &[hickory_proto::rr::Record] {
        self.message.additionals()
    }

    fn is_authoritative(&self) -> bool {
        self.message.authoritative()
    }

    fn is_truncated(&self) -> bool {
        self.message.truncated()
    }

    fn response_code(&self) -> hickory_proto::op::ResponseCode {
        self.message.response_code()
    }

    /// NS names from the authority section.
    fn referral_ns_names(&self) -> Vec<Name> {
        self.authority()
            .iter()
            .filter(|r| r.record_type() == RecordType::NS)
            .filter_map(|r| match r.data() {
                RData::NS(ns) => Some(ns.0.clone()),
                _ => None,
            })
            .collect()
    }

    /// Glue A/AAAA records from the additional section.
    fn glue_ips(&self) -> Vec<(Name, IpAddr)> {
        self.additional()
            .iter()
            .filter_map(|r| match r.data() {
                RData::A(a) => Some((r.name().clone(), IpAddr::V4(a.0))),
                RData::AAAA(aaaa) => Some((r.name().clone(), IpAddr::V6(aaaa.0))),
                _ => None,
            })
            .collect()
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

/// Minimal DNS record representation for SSE output.
#[derive(Debug, Clone, Serialize)]
pub struct DnsRecord {
    pub name: String,
    pub ttl: u32,
    pub record_type: String,
    pub rdata: String,
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

struct RawQueryResult {
    server: SocketAddr,
    result: Result<RawResponse, RawError>,
}

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
// Internal: glue resolution
// ---------------------------------------------------------------------------

/// Build a server list from a NS → IPs map, filtering to IPv4 addresses.
fn build_server_list(
    ns_servers: &HashMap<String, Vec<IpAddr>>,
    ip_allowed: impl Fn(IpAddr) -> bool,
) -> Vec<(SocketAddr, Option<String>)> {
    let mut servers = Vec::new();
    for (ns_name, ips) in ns_servers {
        for ip in ips {
            if ip_allowed(*ip) {
                servers.push((SocketAddr::new(*ip, 53), Some(ns_name.clone())));
            }
        }
    }
    servers
}

/// Resolve NS names with no glue IPs using the system resolver.
///
/// Mutates `ns_servers` in place, filling in IPv4 addresses for empty entries.
async fn resolve_missing_glue(ns_servers: &mut HashMap<String, Vec<IpAddr>>) {
    let missing: Vec<String> = ns_servers
        .iter()
        .filter(|(_, ips)| ips.is_empty())
        .map(|(name, _)| name.clone())
        .collect();

    if missing.is_empty() {
        return;
    }

    tracing::debug!(count = missing.len(), "resolving NS names without glue");

    for ns_name in missing {
        // lookup_host expects "host:port"; strip trailing dot from FQDN.
        let host = ns_name.trim_end_matches('.');
        match tokio::net::lookup_host(format!("{host}:53")).await {
            Ok(addrs) => {
                let entry = ns_servers.entry(ns_name.clone()).or_default();
                for addr in addrs {
                    let ip = addr.ip();
                    if ip.is_ipv4() && !entry.contains(&ip) {
                        entry.push(ip);
                    }
                }
                if entry.is_empty() {
                    tracing::debug!(ns = %ns_name, "glue resolution returned no IPv4 addresses");
                }
            }
            Err(e) => {
                tracing::warn!(ns = %ns_name, error = %e, "glue resolution failed");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal: raw DNS queries
// ---------------------------------------------------------------------------

async fn parallel_queries(
    servers: &[SocketAddr],
    name: &Name,
    record_type: RecordType,
    timeout: Duration,
) -> Vec<RawQueryResult> {
    let futures = servers.iter().copied().map(|server| {
        let name = name.clone();
        async move {
            let result = raw_query(server, &name, record_type, timeout).await;
            RawQueryResult { server, result }
        }
    });
    stream::iter(futures)
        .buffer_unordered(MAX_CONCURRENT)
        .collect()
        .await
}

/// Send a non-recursive DNS query over UDP, with TCP fallback on truncation.
async fn raw_query(
    server: SocketAddr,
    name: &Name,
    record_type: RecordType,
    timeout: Duration,
) -> Result<RawResponse, RawError> {
    let msg = build_query(name, record_type);
    let response = send_udp(server, &msg, timeout).await?;
    if response.is_truncated() {
        tracing::debug!(%server, "UDP response truncated, retrying over TCP");
        let msg = build_query(name, record_type);
        send_tcp(server, &msg, timeout).await
    } else {
        Ok(response)
    }
}

/// Build a non-recursive (RD=0) DNS query message.
fn build_query(name: &Name, record_type: RecordType) -> Message {
    let mut msg = Message::new();
    msg.set_id(rand::random::<u16>());
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    msg.set_recursion_desired(false);
    let mut query = Query::new();
    query.set_name(name.clone());
    query.set_query_type(record_type);
    query.set_query_class(DNSClass::IN);
    msg.add_query(query);
    msg
}

async fn send_udp(
    server: SocketAddr,
    msg: &Message,
    timeout: Duration,
) -> Result<RawResponse, RawError> {
    let msg_bytes = msg.to_vec().map_err(|e| RawError::Decode(e.to_string()))?;
    let expected_id = msg.id();

    let bind_addr: SocketAddr = if server.is_ipv6() {
        "[::]:0".parse().unwrap()
    } else {
        "0.0.0.0:0".parse().unwrap()
    };
    let socket = UdpSocket::bind(bind_addr).await?;

    let start = Instant::now();
    socket.send_to(&msg_bytes, server).await?;

    let mut buf = vec![0u8; 4096];
    let len = match tokio::time::timeout(timeout, socket.recv(&mut buf)).await {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => return Err(RawError::Io(e)),
        Err(_) => return Err(RawError::Timeout(timeout)),
    };
    let latency = start.elapsed();

    let response =
        Message::from_vec(&buf[..len]).map_err(|e| RawError::Decode(e.to_string()))?;
    if response.id() != expected_id {
        return Err(RawError::IdMismatch { expected: expected_id, got: response.id() });
    }

    Ok(RawResponse { message: response, latency })
}

async fn send_tcp(
    server: SocketAddr,
    msg: &Message,
    timeout: Duration,
) -> Result<RawResponse, RawError> {
    let msg_bytes = msg.to_vec().map_err(|e| RawError::Decode(e.to_string()))?;
    let expected_id = msg.id();

    let start = Instant::now();
    let mut stream = match tokio::time::timeout(timeout, TcpStream::connect(server)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(RawError::Io(e)),
        Err(_) => return Err(RawError::Timeout(timeout)),
    };

    let len = msg_bytes.len() as u16;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&msg_bytes).await?;

    // Cap at 16 KB — standard DNS responses rarely exceed a few KB.
    const MAX_TCP_LEN: usize = 16_384;

    let response_len = match tokio::time::timeout(timeout, stream.read_u16()).await {
        Ok(Ok(n)) => n as usize,
        Ok(Err(e)) => return Err(RawError::Io(e)),
        Err(_) => return Err(RawError::Timeout(timeout)),
    };

    if response_len > MAX_TCP_LEN {
        return Err(RawError::Decode(format!(
            "TCP response length {response_len} exceeds {MAX_TCP_LEN}"
        )));
    }

    let mut buf = vec![0u8; response_len];
    match tokio::time::timeout(timeout, stream.read_exact(&mut buf)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(RawError::Io(e)),
        Err(_) => return Err(RawError::Timeout(timeout)),
    };
    let latency = start.elapsed();

    let response = Message::from_vec(&buf).map_err(|e| RawError::Decode(e.to_string()))?;
    if response.id() != expected_id {
        return Err(RawError::IdMismatch { expected: expected_id, got: response.id() });
    }

    Ok(RawResponse { message: response, latency })
}

// ---------------------------------------------------------------------------
// Internal: record conversion
// ---------------------------------------------------------------------------

fn record_to_dns_record(record: &hickory_proto::rr::Record) -> DnsRecord {
    DnsRecord {
        name: record.name().to_ascii(),
        ttl: record.ttl(),
        record_type: record.record_type().to_string(),
        rdata: format!("{}", record.data()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn root_servers_count() {
        assert_eq!(ROOT_SERVERS.len(), 13);
    }

    #[test]
    fn build_query_sets_rd_false() {
        let name = Name::from_str("example.com.").unwrap();
        let msg = build_query(&name, RecordType::A);
        assert!(!msg.recursion_desired());
        assert_eq!(msg.queries().len(), 1);
        assert_eq!(msg.queries()[0].query_type(), RecordType::A);
    }

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

    #[test]
    fn build_server_list_ipv4_only() {
        let mut ns_servers = HashMap::new();
        ns_servers.insert(
            "ns1.example.com.".to_owned(),
            vec![
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
                IpAddr::V6("2001:db8::1".parse().unwrap()),
            ],
        );

        let servers = build_server_list(&ns_servers, |ip| ip.is_ipv4());
        assert_eq!(servers.len(), 1);
        assert!(servers[0].0.ip().is_ipv4());
        assert_eq!(servers[0].1, Some("ns1.example.com.".to_owned()));
    }
}
