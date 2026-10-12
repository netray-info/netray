use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use mhost::RecordType;
use mhost::nameserver::predefined::PredefinedProvider;
use mhost::nameserver::{NameServerConfig, Protocol};
use mhost::resolver::lookup::LookupResult;
use mhost::resolver::{
    Error as ResolverError, Lookups, MultiQuery, Resolver, ResolverGroup, ResolverGroupBuilder,
};

use super::{AddressLookup, LookupError};

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
        let mut builder = ResolverGroupBuilder::new().timeout(Duration::from_millis(timeout_ms));

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
                // machines where IPv6 is unavailable, causing 10 s stalls per query),
                // UDP/TCP only: in the one `netray` binary prism enables mhost's
                // DoH/DoT, which would otherwise join beacon's rotation.
                for ns_config in provider.configs() {
                    if ns_config.ip_addr().is_ipv4()
                        && matches!(ns_config.protocol(), Protocol::Udp | Protocol::Tcp)
                    {
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

    /// Resolve A + AAAA records for a hostname. An error only when both halves failed; when
    /// one did, the addresses of the other come with its error, so the missing family is never
    /// read as a measured absence.
    pub async fn lookup_ips(&self, hostname: &str) -> Result<AddressLookup, LookupError> {
        let hostname = hostname.trim_end_matches('.');
        let resolver = self.pick();
        let (a_result, aaaa_result) = tokio::join!(
            lookup_on(resolver, hostname, RecordType::A, "A"),
            lookup_on(resolver, hostname, RecordType::AAAA, "AAAA"),
        );
        combine(a_result.map(|l| l.ips()), aaaa_result.map(|l| l.ips()))
    }

    /// Resolve MX records. Returns (preference, exchange) tuples.
    pub async fn lookup_mx(&self, domain: &str) -> Result<Vec<(u16, String)>, LookupError> {
        let domain = domain.trim_end_matches('.');
        let lookups = lookup_on(self.pick(), domain, RecordType::MX, "MX").await?;

        let mut results: Vec<(u16, String)> = lookups
            .mx()
            .iter()
            .map(|mx| (mx.preference(), mx.exchange().to_string()))
            .collect();

        // Deduplicate
        results.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        results.dedup();
        Ok(results)
    }

    /// Resolve TXT records for a name.
    pub async fn lookup_txt(&self, name: &str) -> Result<Vec<String>, LookupError> {
        let name = name.trim_end_matches('.');
        let lookups = lookup_on(self.pick(), name, RecordType::TXT, "TXT").await?;

        let mut results: Vec<String> = lookups.txt().iter().map(|t| t.as_string()).collect();
        results.sort();
        results.dedup();
        Ok(results)
    }

    /// Resolve PTR records for an IP address.
    pub async fn lookup_ptr(&self, ip: IpAddr) -> Result<Vec<String>, LookupError> {
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

        let lookups = lookup_on(self.pick(), &arpa, RecordType::PTR, "PTR").await?;

        let mut results: Vec<String> = lookups.ptr().iter().map(|n| n.to_string()).collect();
        results.sort();
        results.dedup();
        Ok(results)
    }

    /// Resolve TLSA records at a given name (e.g., `_25._tcp.mx.example.com`).
    pub async fn lookup_tlsa(&self, name: &str) -> Result<Vec<TlsaRecord>, LookupError> {
        let lookups = lookup_on(self.pick(), name, RecordType::TLSA, "TLSA").await?;

        Ok(lookups
            .tlsa()
            .iter()
            .map(|t| TlsaRecord {
                usage: cert_usage_to_u8(t.cert_usage()),
                selector: selector_to_u8(t.selector()),
                matching_type: matching_to_u8(t.matching()),
            })
            .collect())
    }

    /// Check CNAME for a given name.
    pub async fn lookup_cname(&self, name: &str) -> Result<Vec<String>, LookupError> {
        let lookups = lookup_on(self.pick(), name, RecordType::CNAME, "CNAME").await?;
        Ok(lookups.cname().iter().map(|n| n.to_string()).collect())
    }

    /// Query a name and check if we get any result (used for SPF exists:).
    pub async fn lookup_exists(&self, name: &str) -> bool {
        self.lookup_a(name).await.is_ok_and(|ips| !ips.is_empty())
    }

    /// Resolve A records for a name, returning the actual Ipv4 values.
    /// DNSBL responses are typed: `127.0.0.x` values encode listing codes,
    /// `127.255.255.x` values encode error/policy responses.
    pub async fn lookup_a(&self, name: &str) -> Result<Vec<Ipv4Addr>, LookupError> {
        let lookups = lookup_on(self.pick(), name, RecordType::A, "A").await?;
        Ok(lookups.a().iter().map(|v| **v).collect())
    }

    /// Check for DNSSEC: query DNSKEY and treat presence of any DNSKEY
    /// records as a signed-zone signal. We don't query RRSIG directly —
    /// it's a meta-type many authoritative servers refuse (REFUSED/SERVFAIL),
    /// which made the previous check report false negatives on properly
    /// signed zones (e.g. NSOne-hosted domains). An error when no nameserver
    /// answered definitively: unknown, not unsigned.
    pub async fn check_dnssec_signed(&self, domain: &str) -> Result<bool, LookupError> {
        let lookups = lookup_on(self.pick(), domain, RecordType::DNSKEY, "DNSKEY").await?;
        Ok(!lookups.dnskey().is_empty())
    }
}

/// Merges the A and the AAAA half of an address lookup: an error only when both failed,
/// otherwise the answered addresses with the failed half's error, if any.
fn combine(
    a: Result<Vec<IpAddr>, LookupError>,
    aaaa: Result<Vec<IpAddr>, LookupError>,
) -> Result<AddressLookup, LookupError> {
    if let (Err(e), Err(_)) = (&a, &aaaa) {
        return Err(LookupError {
            record_type: "address",
            ..e.clone()
        });
    }
    let mut lookup = AddressLookup::default();
    for half in [a, aaaa] {
        match half {
            Ok(ips) => lookup.ips.extend(ips),
            Err(e) => lookup.error = Some(e),
        }
    }
    let mut seen = std::collections::HashSet::new();
    lookup.ips.retain(|ip| seen.insert(*ip));
    Ok(lookup)
}

/// One query of `record_type` at `name` on `resolver`, as a definite answer or a [`LookupError`].
async fn lookup_on(
    resolver: &Resolver,
    name: &str,
    record_type: RecordType,
    label: &'static str,
) -> Result<Lookups, LookupError> {
    let query = match MultiQuery::single(name, record_type) {
        Ok(q) => q,
        Err(e) => {
            record_dns_outcome(label, Err(&e));
            tracing::warn!(query_name = %name, record_type = label, error = %e, "DNS lookup failed");
            return Err(LookupError {
                record_type: label,
                name: name.to_string(),
                cause: e.to_string(),
            });
        }
    };
    let result = resolver.lookup(query).await;
    record_dns_outcome(label, result.as_ref().map(|_| ()));
    let answer = answer(label, name, result);
    if let Err(e) = &answer {
        tracing::warn!(query_name = %name, record_type = label, error = %e.cause, "DNS lookup failed");
    }
    answer
}

/// The lookups when some nameserver answered definitively (records, NXDOMAIN or NODATA);
/// otherwise the cause: the resolver error, a referral, the rcode, or no answer.
fn answer(
    record_type: &'static str,
    name: &str,
    result: Result<Lookups, ResolverError>,
) -> Result<Lookups, LookupError> {
    let error = |cause: String| LookupError {
        record_type,
        name: name.to_string(),
        cause,
    };
    let lookups = result.map_err(|e| error(e.to_string()))?;
    let definite = lookups.iter().any(|l| match l.result() {
        LookupResult::Response(_) => true,
        LookupResult::NxDomain(nx) => nx.is_definite(),
        LookupResult::Error(_) => false,
    });
    if definite {
        return Ok(lookups);
    }
    let cause = lookups
        .iter()
        .find_map(|l| match l.result() {
            LookupResult::Error(e) => Some(e.to_string()),
            LookupResult::NxDomain(nx) if nx.is_referral() => Some("referral".to_string()),
            LookupResult::NxDomain(nx) => nx.response_code().map(|rc| rc.to_string()),
            LookupResult::Response(_) => None,
        })
        .unwrap_or_else(|| "no answer".to_string());
    Err(error(cause))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn lookups(results: Vec<Value>) -> Result<Lookups, ResolverError> {
        let items: Vec<Value> = results
            .into_iter()
            .map(|result| {
                json!({
                    "query": { "name": "example.com.", "record_type": "TXT" },
                    "name_server": "udp:192.0.2.53:53",
                    "result": result
                })
            })
            .collect();
        Ok(serde_json::from_value(json!({ "lookups": items })).expect("lookups deserialise"))
    }

    fn nx(code: &str, referral: bool) -> Value {
        json!({ "NxDomain": {
            "response_time": { "secs": 0, "nanos": 5_000_000 },
            "response_code": code,
            "referral": referral
        }})
    }

    fn cause(results: Vec<Value>) -> Option<String> {
        answer("TXT", "example.com", lookups(results))
            .err()
            .map(|e| e.cause)
    }

    fn failed(record_type: &'static str) -> LookupError {
        LookupError {
            record_type,
            name: "mail.example.com".to_string(),
            cause: "SERVFAIL".to_string(),
        }
    }

    #[test]
    fn combine_keeps_the_answered_half() {
        let v4: IpAddr = "192.0.2.10".parse().unwrap();
        let v6: IpAddr = "2001:db8::25".parse().unwrap();

        let both = combine(Ok(vec![v4]), Ok(vec![v6])).unwrap();
        assert_eq!(both.ips, vec![v4, v6]);
        assert_eq!(both.error, None);

        let aaaa_failed = combine(Ok(vec![v4]), Err(failed("AAAA"))).unwrap();
        assert_eq!(aaaa_failed.ips, vec![v4]);
        assert_eq!(aaaa_failed.error, Some(failed("AAAA")));

        let a_failed = combine(Err(failed("A")), Ok(vec![v6])).unwrap();
        assert_eq!(a_failed.ips, vec![v6]);
        assert_eq!(a_failed.error, Some(failed("A")));

        let none = combine(Err(failed("A")), Err(failed("AAAA"))).unwrap_err();
        assert_eq!(none.record_type, "address");
        assert_eq!(none.name, "mail.example.com");
    }

    #[test]
    fn answer_is_a_response_or_a_definite_negative() {
        let response = json!({ "Response": {
            "records": [],
            "response_time": { "secs": 0, "nanos": 5_000_000 },
            "valid_until": "2026-01-01T00:00:00Z"
        }});
        assert_eq!(cause(vec![response]), None);
        assert_eq!(cause(vec![nx("NXDOMAIN", false)]), None);
        assert_eq!(cause(vec![nx("NOERROR", false)]), None);
        assert_eq!(
            cause(vec![json!({ "Error": "Timeout" }), nx("NXDOMAIN", false)]),
            None
        );
        assert_eq!(
            cause(vec![nx("SERVFAIL", false)]).as_deref(),
            Some("SERVFAIL")
        );
        assert_eq!(
            cause(vec![nx("NOERROR", true)]).as_deref(),
            Some("referral")
        );
        assert_eq!(
            cause(vec![json!({ "Error": "Timeout" })]).as_deref(),
            Some("request timed out")
        );
        assert_eq!(cause(vec![]).as_deref(), Some("no answer"));
        assert!(answer("TXT", "example.com", Err(ResolverError::Timeout)).is_err());
    }

    // In the one `netray` binary, prism turns on mhost's `dot`/`doh` features for
    // every crate. beacon must still resolve over plain UDP/TCP only, as the
    // separate beacon binary did.
    #[tokio::test]
    async fn predefined_provider_uses_only_udp_and_tcp() {
        let resolver = DnsResolver::new(&["cloudflare".to_string()], 1000)
            .await
            .expect("resolver group builds");
        let names: Vec<String> = resolver.resolvers.iter().map(|r| r.name()).collect();
        assert!(!names.is_empty(), "no resolvers built");
        for name in &names {
            assert!(
                name.starts_with("udp:") || name.starts_with("tcp:"),
                "unexpected transport in {name}; all: {names:?}"
            );
        }
    }
}
