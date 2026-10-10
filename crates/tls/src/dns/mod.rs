pub mod caa;
pub mod https_record;
pub mod tlsa;

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use mhost::RecordType;
use mhost::resolver::{ResolverGroup, ResolverGroupBuilder, UniQuery};

pub use caa::CaaLookup;
pub use tlsa::{TlsaLookup, TlsaRecord};

pub struct DnsResolver {
    resolvers: Arc<ResolverGroup>,
}

impl DnsResolver {
    pub async fn new(timeout_secs: u64) -> Result<Self, mhost::Error> {
        let resolvers = ResolverGroupBuilder::new()
            .system()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .await?;
        Ok(Self {
            resolvers: Arc::new(resolvers),
        })
    }

    pub async fn lookup_caa(&self, hostname: &str) -> CaaLookup {
        caa::lookup_caa(&self.resolvers, hostname).await
    }

    pub async fn lookup_tlsa(&self, hostname: &str, port: u16) -> TlsaLookup {
        tlsa::lookup_tlsa(&self.resolvers, hostname, port).await
    }

    /// Check if the HTTPS DNS record for `hostname` advertises ECH support.
    pub async fn lookup_ech_advertised(&self, hostname: &str) -> bool {
        https_record::has_ech_advertised(&self.resolvers, hostname).await
    }

    /// Resolve a hostname to IPv4 and IPv6 addresses using mhost.
    /// Returns an empty vec on resolution failure (caller handles the error).
    pub async fn lookup_ips(&self, hostname: &str) -> Vec<IpAddr> {
        let hostname = hostname.trim_end_matches('.');

        // Query A and AAAA in parallel, collect results from both.
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

        // Deduplicate preserving order.
        let mut seen = std::collections::HashSet::new();
        ips.retain(|ip| seen.insert(*ip));
        ips
    }
}
