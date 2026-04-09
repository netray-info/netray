use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use mhost::RecordType;
use mhost::resolver::{ResolverGroup, ResolverGroupBuilder, UniQuery};
use mhost::nameserver::predefined::PredefinedProvider;

use crate::config::DnsConfig;

pub struct DnsResolver {
    resolvers: Arc<ResolverGroup>,
}

impl DnsResolver {
    pub async fn new(cfg: &DnsConfig) -> Result<Self, mhost::Error> {
        let mut builder = ResolverGroupBuilder::new()
            .timeout(Duration::from_millis(cfg.timeout_ms));

        for name in &cfg.resolvers {
            if name == "system" {
                builder = builder.system();
            } else if let Ok(provider) = PredefinedProvider::from_str(name) {
                builder = builder.predefined(provider);
            } else {
                tracing::warn!(resolver = %name, "unknown resolver name, skipping");
            }
        }

        let resolvers = builder.build().await?;
        Ok(Self {
            resolvers: Arc::new(resolvers),
        })
    }

    pub fn is_initialized(&self) -> bool {
        !self.resolvers.is_empty()
    }

    /// Resolve A + AAAA records for a hostname.
    pub async fn lookup_ips(&self, hostname: &str) -> Vec<IpAddr> {
        let hostname = hostname.trim_end_matches('.');
        let (a_result, aaaa_result) = tokio::join!(
            async {
                let query = UniQuery::new(hostname, RecordType::A).ok()?;
                let lookups = self.resolvers.lookup(query).await.ok()?;
                Some(lookups.ips())
            },
            async {
                let query = UniQuery::new(hostname, RecordType::AAAA).ok()?;
                let lookups = self.resolvers.lookup(query).await.ok()?;
                Some(lookups.ips())
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
        let query = match UniQuery::new(domain, RecordType::MX) {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };
        let lookups = match self.resolvers.lookup(query).await {
            Ok(l) => l,
            Err(_) => return Vec::new(),
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
        let query = match UniQuery::new(name, RecordType::TXT) {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };
        let lookups = match self.resolvers.lookup(query).await {
            Ok(l) => l,
            Err(_) => return Vec::new(),
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

        let query = match UniQuery::new(&arpa, RecordType::PTR) {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };
        let lookups = match self.resolvers.lookup(query).await {
            Ok(l) => l,
            Err(_) => return Vec::new(),
        };

        let mut results: Vec<String> = lookups.ptr().iter().map(|n| n.to_string()).collect();
        results.sort();
        results.dedup();
        results
    }

    /// Resolve TLSA records at a given name (e.g., `_25._tcp.mx.example.com`).
    pub async fn lookup_tlsa(&self, name: &str) -> Vec<TlsaRecord> {
        let query = match UniQuery::new(name, RecordType::TLSA) {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };
        let lookups = match self.resolvers.lookup(query).await {
            Ok(l) => l,
            Err(_) => return Vec::new(),
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
        let query = match UniQuery::new(name, RecordType::CNAME) {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };
        let lookups = match self.resolvers.lookup(query).await {
            Ok(l) => l,
            Err(_) => return Vec::new(),
        };

        lookups.cname().iter().map(|n| n.to_string()).collect()
    }

    /// Query a name and check if we get any result (used for DNSBL).
    pub async fn lookup_exists(&self, name: &str) -> bool {
        let query = match UniQuery::new(name, RecordType::A) {
            Ok(q) => q,
            Err(_) => return false,
        };
        match self.resolvers.lookup(query).await {
            Ok(l) => !l.a().is_empty(),
            Err(_) => false,
        }
    }

    /// Check for DNSSEC: query with RRSIG to see if the domain is signed.
    /// Since mhost doesn't expose the AD bit, we check if RRSIG records
    /// are returned for the domain's SOA record as a proxy.
    pub async fn check_dnssec_signed(&self, domain: &str) -> bool {
        let query = match UniQuery::new(domain, RecordType::RRSIG) {
            Ok(q) => q,
            Err(_) => return false,
        };
        match self.resolvers.lookup(query).await {
            Ok(l) => !l.rrsig().is_empty(),
            Err(_) => false,
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
