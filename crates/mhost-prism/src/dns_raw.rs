//! Shared low-level DNS query infrastructure.
//!
//! Provides raw (non-recursive, RD=0) DNS queries over UDP/TCP, parallel
//! fan-out, glue resolution, and record conversion. Used by both `dns_trace`
//! and `dns_dnssec`.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

pub(crate) static ROOT_SERVERS: &[Ipv4Addr] = &[
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
pub(crate) const MAX_CONCURRENT: usize = 8;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) enum RawError {
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

pub(crate) struct RawResponse {
    pub(crate) message: Message,
    pub(crate) latency: Duration,
}

impl RawResponse {
    pub(crate) fn answers(&self) -> &[hickory_proto::rr::Record] {
        self.message.answers()
    }

    pub(crate) fn authority(&self) -> &[hickory_proto::rr::Record] {
        self.message.name_servers()
    }

    pub(crate) fn additional(&self) -> &[hickory_proto::rr::Record] {
        self.message.additionals()
    }

    pub(crate) fn is_authoritative(&self) -> bool {
        self.message.authoritative()
    }

    pub(crate) fn is_truncated(&self) -> bool {
        self.message.truncated()
    }

    pub(crate) fn response_code(&self) -> hickory_proto::op::ResponseCode {
        self.message.response_code()
    }

    /// NS names from the authority section.
    pub(crate) fn referral_ns_names(&self) -> Vec<Name> {
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
    pub(crate) fn glue_ips(&self) -> Vec<(Name, IpAddr)> {
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
// Raw query result
// ---------------------------------------------------------------------------

pub(crate) struct RawQueryResult {
    pub(crate) server: SocketAddr,
    pub(crate) result: Result<RawResponse, RawError>,
}

// ---------------------------------------------------------------------------
// Minimal DNS record representation for SSE output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DnsRecord {
    pub name: String,
    pub ttl: u32,
    pub record_type: String,
    pub rdata: String,
}

// ---------------------------------------------------------------------------
// Raw DNS queries
// ---------------------------------------------------------------------------

pub(crate) async fn parallel_queries(
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
pub(crate) async fn raw_query(
    server: SocketAddr,
    name: &Name,
    record_type: RecordType,
    timeout: Duration,
) -> Result<RawResponse, RawError> {
    let msg = build_query(name, record_type, false);
    let response = send_udp(server, &msg, timeout).await?;
    if response.is_truncated() {
        tracing::debug!(%server, "UDP response truncated, retrying over TCP");
        let msg = build_query(name, record_type, false);
        send_tcp(server, &msg, timeout).await
    } else {
        Ok(response)
    }
}

/// Send a non-recursive DNS query with the DO (DNSSEC OK) bit set.
pub(crate) async fn raw_query_dnssec(
    server: SocketAddr,
    name: &Name,
    record_type: RecordType,
    timeout: Duration,
) -> Result<RawResponse, RawError> {
    let msg = build_query(name, record_type, true);
    let response = send_udp(server, &msg, timeout).await?;
    if response.is_truncated() {
        tracing::debug!(%server, "UDP response truncated, retrying over TCP");
        let msg = build_query(name, record_type, true);
        send_tcp(server, &msg, timeout).await
    } else {
        Ok(response)
    }
}

/// Build a non-recursive (RD=0) DNS query message, optionally with the DO bit.
pub(crate) fn build_query(name: &Name, record_type: RecordType, dnssec_ok: bool) -> Message {
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
    if dnssec_ok {
        let mut edns = hickory_proto::op::Edns::new();
        edns.set_dnssec_ok(true);
        edns.set_max_payload(4096);
        msg.set_edns(edns);
    }
    msg
}

pub(crate) async fn send_udp(
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

pub(crate) async fn send_tcp(
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
// Glue resolution
// ---------------------------------------------------------------------------

/// Build a server list from a NS → IPs map, filtering by the predicate.
pub(crate) fn build_server_list(
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
pub(crate) async fn resolve_missing_glue(ns_servers: &mut HashMap<String, Vec<IpAddr>>) {
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
// Record conversion
// ---------------------------------------------------------------------------

pub(crate) fn record_to_dns_record(record: &hickory_proto::rr::Record) -> DnsRecord {
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
    use std::str::FromStr;

    #[test]
    fn root_servers_count() {
        assert_eq!(ROOT_SERVERS.len(), 13);
    }

    #[test]
    fn build_query_sets_rd_false() {
        let name = Name::from_str("example.com.").unwrap();
        let msg = build_query(&name, RecordType::A, false);
        assert!(!msg.recursion_desired());
        assert_eq!(msg.queries().len(), 1);
        assert_eq!(msg.queries()[0].query_type(), RecordType::A);
        assert!(msg.extensions().is_none());
    }

    #[test]
    fn build_query_with_dnssec_ok_sets_do_bit() {
        let name = Name::from_str("example.com.").unwrap();
        let msg = build_query(&name, RecordType::DNSKEY, true);
        assert!(!msg.recursion_desired());
        // EDNS should be present when DO bit is requested.
        assert!(msg.extensions().is_some());
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
