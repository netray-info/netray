use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use mhost::RecordType;
use mhost::nameserver::NameServerConfig;
use mhost::nameserver::predefined::PredefinedProvider;
use mhost::resolver::{Error as ResolverError, MultiQuery, Resolver, ResolverGroup, ResolverGroupBuilder};

/// Map an mhost resolver error to the coarse taxonomy used by the
/// `beacon_dns_queries_total` metric (see SDD Decision Log §12).
fn classify_dns_error(e: &ResolverError) -> &'static str {
    match e {
        ResolverError::Timeout => "timeout",
        ResolverError::NoRecordsFound => "nxdomain",
        _ => "error",
    }
}

fn record_dns_outcome(rtype: &'static str, result: Result<(), &ResolverError>) {
    let outcome = match result {
        Ok(()) => "ok",
        Err(e) => classify_dns_error(e),
    };
    metrics::counter!(
        "beacon_dns_queries_total",
        "record_type" => rtype,
        "outcome" => outcome,
    )
    .increment(1);
}

pub struct DnsResolver {
    resolvers: Vec<Resolver>,
    index: AtomicUsize,
}

impl DnsResolver {
    pub async fn new(resolvers: &[String], timeout_ms: u64) -> Result<Self, mhost::Error> {
        let mut builder =
            ResolverGroupBuilder::new().timeout(Duration::from_millis(timeout_ms));

        for entry in resolvers {
            if entry == "system" {
                builder = builder.system();
            } else if let Ok(ip) = entry.parse::<IpAddr>() {
                // Explicit IP — default port 53, UDP
                builder = builder.nameserver(NameServerConfig::udp(SocketAddr::new(ip, 53)));
            } else if let Ok(sock) = entry.parse::<SocketAddr>() {
                // Explicit IP:port
                builder = builder.nameserver(NameServerConfig::udp(sock));
            } else if let Ok(provider) = PredefinedProvider::from_str(entry) {
                // Predefined provider: add IPv4-only configs (IPv6 times out on
                // machines where IPv6 is unavailable, causing 10 s stalls per query)
                for ns_config in provider.configs() {
                    if ns_config.ip_addr().is_ipv4() {
                        builder = builder.nameserver(ns_config);
                    }
                }
            } else {
                tracing::warn!(resolver = %entry, "unknown resolver entry, skipping");
            }
        }

        let group: ResolverGroup = builder.build().await?;
        let resolvers = group.resolvers().to_vec();

        tracing::info!(
            count = resolvers.len(),
            names = %resolvers.iter().map(|r| r.name()).collect::<Vec<_>>().join(", "),
            "DNS resolvers initialized"
        );

        Ok(Self {
            resolvers,
            index: AtomicUsize::new(0),
        })
    }

    pub fn is_initialized(&self) -> bool {
        !self.resolvers.is_empty()
    }

    fn pick(&self) -> &Resolver {
        let idx = self.index.fetch_add(1, Ordering::Relaxed) % self.resolvers.len();
        &self.resolvers[idx]
    }

    /// Resolve A + AAAA records for a hostname.
    pub async fn lookup_ips(&self, hostname: &str) -> Vec<IpAddr> {
        let hostname = hostname.trim_end_matches('.');
        let resolver = self.pick();
        let (a_result, aaaa_result) = tokio::join!(
            async {
                let query = match MultiQuery::single(hostname, RecordType::A) {
                    Ok(q) => q,
                    Err(e) => {
                        tracing::warn!(query_name = %hostname, record_type = "A", error = %e, "DNS lookup failed");
                        record_dns_outcome("A", Err(&e));
                        return None;
                    }
                };
                let result = resolver.lookup(query).await;
                record_dns_outcome("A", result.as_ref().map(|_| ()));
                match result {
                    Ok(lookups) => Some(lookups.ips()),
                    Err(e) => {
                        tracing::warn!(query_name = %hostname, record_type = "A", error = %e, "DNS lookup failed");
                        None
                    }
                }
            },
            async {
                let query = match MultiQuery::single(hostname, RecordType::AAAA) {
                    Ok(q) => q,
                    Err(e) => {
                        tracing::warn!(query_name = %hostname, record_type = "AAAA", error = %e, "DNS lookup failed");
                        record_dns_outcome("AAAA", Err(&e));
                        return None;
                    }
                };
                let result = resolver.lookup(query).await;
                record_dns_outcome("AAAA", result.as_ref().map(|_| ()));
                match result {
                    Ok(lookups) => Some(lookups.ips()),
                    Err(e) => {
                        tracing::warn!(query_name = %hostname, record_type = "AAAA", error = %e, "DNS lookup failed");
                        None
                    }
                }
            },
        );

        let mut ips = Vec::new();
        if let Some(addrs) = a_result {
            ips.extend(addrs);
        }
        if let Some(addrs) = aaaa_result {
            ips.extend(addrs);
        }

        let mut seen = std::collections::HashSet::new();
        ips.retain(|ip| seen.insert(*ip));
        ips
    }

    /// Resolve MX records. Returns (preference, exchange) tuples.
    pub async fn lookup_mx(&self, domain: &str) -> Vec<(u16, String)> {
        let domain = domain.trim_end_matches('.');
        let query = match MultiQuery::single(domain, RecordType::MX) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(query_name = %domain, record_type = "MX", error = %e, "DNS lookup failed");
                record_dns_outcome("MX", Err(&e));
                return Vec::new();
            }
        };
        let result = self.pick().lookup(query).await;
        record_dns_outcome("MX", result.as_ref().map(|_| ()));
        let lookups = match result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(query_name = %domain, record_type = "MX", error = %e, "DNS lookup failed");
                return Vec::new();
            }
        };

        let mut results: Vec<(u16, String)> = lookups
            .mx()
            .iter()
            .map(|mx| (mx.preference(), mx.exchange().to_string()))
            .collect();

        // Deduplicate
        results.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        results.dedup();
        results
    }

    /// Resolve TXT records for a name.
    pub async fn lookup_txt(&self, name: &str) -> Vec<String> {
        let name = name.trim_end_matches('.');
        let query = match MultiQuery::single(name, RecordType::TXT) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "TXT", error = %e, "DNS lookup failed");
                record_dns_outcome("TXT", Err(&e));
                return Vec::new();
            }
        };
        let result = self.pick().lookup(query).await;
        record_dns_outcome("TXT", result.as_ref().map(|_| ()));
        let lookups = match result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "TXT", error = %e, "DNS lookup failed");
                return Vec::new();
            }
        };

        let mut results: Vec<String> = lookups.txt().iter().map(|t| t.as_string()).collect();
        results.sort();
        results.dedup();
        results
    }

    /// Resolve PTR records for an IP address.
    pub async fn lookup_ptr(&self, ip: IpAddr) -> Vec<String> {
        let arpa = match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                format!(
                    "{}.{}.{}.{}.in-addr.arpa",
                    octets[3], octets[2], octets[1], octets[0]
                )
            }
            IpAddr::V6(v6) => {
                let segments = v6.octets();
                let nibbles: String = segments
                    .iter()
                    .rev()
                    .flat_map(|b| [b & 0x0f, (b >> 4) & 0x0f])
                    .map(|n| format!("{:x}.", n))
                    .collect();
                format!("{}ip6.arpa", nibbles)
            }
        };

        let query = match MultiQuery::single(arpa.as_str(), RecordType::PTR) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(query_name = %arpa, record_type = "PTR", error = %e, "DNS lookup failed");
                record_dns_outcome("PTR", Err(&e));
                return Vec::new();
            }
        };
        let result = self.pick().lookup(query).await;
        record_dns_outcome("PTR", result.as_ref().map(|_| ()));
        let lookups = match result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(query_name = %arpa, record_type = "PTR", error = %e, "DNS lookup failed");
                return Vec::new();
            }
        };

        let mut results: Vec<String> = lookups.ptr().iter().map(|n| n.to_string()).collect();
        results.sort();
        results.dedup();
        results
    }

    /// Resolve TLSA records at a given name (e.g., `_25._tcp.mx.example.com`).
    pub async fn lookup_tlsa(&self, name: &str) -> Vec<TlsaRecord> {
        let query = match MultiQuery::single(name, RecordType::TLSA) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "TLSA", error = %e, "DNS lookup failed");
                record_dns_outcome("TLSA", Err(&e));
                return Vec::new();
            }
        };
        let result = self.pick().lookup(query).await;
        record_dns_outcome("TLSA", result.as_ref().map(|_| ()));
        let lookups = match result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "TLSA", error = %e, "DNS lookup failed");
                return Vec::new();
            }
        };

        lookups
            .tlsa()
            .iter()
            .map(|t| TlsaRecord {
                usage: cert_usage_to_u8(t.cert_usage()),
                selector: selector_to_u8(t.selector()),
                matching_type: matching_to_u8(t.matching()),
            })
            .collect()
    }

    /// Check CNAME for a given name.
    pub async fn lookup_cname(&self, name: &str) -> Vec<String> {
        let query = match MultiQuery::single(name, RecordType::CNAME) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "CNAME", error = %e, "DNS lookup failed");
                record_dns_outcome("CNAME", Err(&e));
                return Vec::new();
            }
        };
        let result = self.pick().lookup(query).await;
        record_dns_outcome("CNAME", result.as_ref().map(|_| ()));
        let lookups = match result {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "CNAME", error = %e, "DNS lookup failed");
                return Vec::new();
            }
        };

        lookups.cname().iter().map(|n| n.to_string()).collect()
    }

    /// Query a name and check if we get any result (used for SPF exists:).
    pub async fn lookup_exists(&self, name: &str) -> bool {
        !self.lookup_a(name).await.is_empty()
    }

    /// Resolve A records for a name, returning the actual Ipv4 values.
    /// DNSBL responses are typed: `127.0.0.x` values encode listing codes,
    /// `127.255.255.x` values encode error/policy responses.
    pub async fn lookup_a(&self, name: &str) -> Vec<Ipv4Addr> {
        let query = match MultiQuery::single(name, RecordType::A) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "A", error = %e, "DNS lookup failed");
                record_dns_outcome("A", Err(&e));
                return Vec::new();
            }
        };
        let result = self.pick().lookup(query).await;
        record_dns_outcome("A", result.as_ref().map(|_| ()));
        match result {
            Ok(l) => l.a().iter().map(|v| **v).collect(),
            Err(e) => {
                tracing::warn!(query_name = %name, record_type = "A", error = %e, "DNS lookup failed");
                Vec::new()
            }
        }
    }

    /// Check for DNSSEC: query DNSKEY and treat presence of any DNSKEY
    /// records as a signed-zone signal. We don't query RRSIG directly —
    /// it's a meta-type many authoritative servers refuse (REFUSED/SERVFAIL),
    /// which made the previous check report false negatives on properly
    /// signed zones (e.g. NSOne-hosted domains).
    pub async fn check_dnssec_signed(&self, domain: &str) -> bool {
        let query = match MultiQuery::single(domain, RecordType::DNSKEY) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(query_name = %domain, record_type = "DNSKEY", error = %e, "DNS lookup failed");
                record_dns_outcome("DNSKEY", Err(&e));
                return false;
            }
        };
        let result = self.pick().lookup(query).await;
        record_dns_outcome("DNSKEY", result.as_ref().map(|_| ()));
        match result {
            Ok(l) => !l.dnskey().is_empty(),
            Err(e) => {
                tracing::warn!(query_name = %domain, record_type = "DNSKEY", error = %e, "DNS lookup failed");
                false
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TlsaRecord {
    pub usage: u8,
    pub selector: u8,
    pub matching_type: u8,
}

fn cert_usage_to_u8(cu: mhost::resources::rdata::CertUsage) -> u8 {
    use mhost::resources::rdata::CertUsage;
    match cu {
        CertUsage::PkixTa => 0,
        CertUsage::PkixEe => 1,
        CertUsage::DaneTa => 2,
        CertUsage::DaneEe => 3,
        CertUsage::Private => 255,
        CertUsage::Unassigned(v) => v,
    }
}

fn selector_to_u8(s: mhost::resources::rdata::Selector) -> u8 {
    use mhost::resources::rdata::Selector;
    match s {
        Selector::Full => 0,
        Selector::Spki => 1,
        Selector::Private => 255,
        Selector::Unassigned(v) => v,
    }
}

fn matching_to_u8(m: mhost::resources::rdata::Matching) -> u8 {
    use mhost::resources::rdata::Matching;
    match m {
        Matching::Raw => 0,
        Matching::Sha256 => 1,
        Matching::Sha512 => 2,
        Matching::Private => 255,
        Matching::Unassigned(v) => v,
    }
}
