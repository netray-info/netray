pub mod caa;
pub mod https_record;
pub mod tlsa;

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use mhost::RecordType;
use mhost::resolver::lookup::LookupResult;
use mhost::resolver::{Lookups, ResolverGroup, ResolverGroupBuilder, UniQuery};

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

    /// The CAA records that apply to `hostname`; `None` when the lookup failed.
    pub async fn lookup_caa(&self, hostname: &str) -> Option<CaaLookup> {
        caa::lookup_caa(&self.resolvers, hostname).await
    }

    /// The TLSA records for `hostname` and `port`; `None` when the lookup failed.
    pub async fn lookup_tlsa(&self, hostname: &str, port: u16) -> Option<TlsaLookup> {
        tlsa::lookup_tlsa(&self.resolvers, hostname, port).await
    }

    /// Check if the HTTPS DNS record for `hostname` advertises ECH support; `None` when the
    /// lookup failed.
    pub async fn lookup_ech_advertised(&self, hostname: &str) -> Option<bool> {
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

/// Whether some nameserver answered definitively: records, NXDOMAIN or NODATA.
fn answered(lookups: &Lookups) -> bool {
    lookups.iter().any(|l| match l.result() {
        LookupResult::Response(_) => true,
        LookupResult::NxDomain(nx) => nx.is_definite(),
        LookupResult::Error(_) => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn lookup(result: Value) -> Value {
        json!({
            "query": { "name": "example.com.", "record_type": "CAA" },
            "name_server": "udp:192.0.2.53:53",
            "result": result
        })
    }

    fn lookups(results: Vec<Value>) -> Lookups {
        let items: Vec<Value> = results.into_iter().map(lookup).collect();
        serde_json::from_value(json!({ "lookups": items })).expect("lookups deserialise")
    }

    fn nx(code: &str) -> Value {
        json!({ "NxDomain": {
            "response_time": { "secs": 0, "nanos": 5_000_000 },
            "response_code": code
        }})
    }

    #[test]
    fn answered_is_a_response_or_a_definite_negative() {
        let response = json!({ "Response": {
            "records": [],
            "response_time": { "secs": 0, "nanos": 5_000_000 },
            "valid_until": "2026-01-01T00:00:00Z"
        }});
        assert!(answered(&lookups(vec![response])));
        assert!(answered(&lookups(vec![nx("NXDOMAIN")])));
        assert!(answered(&lookups(vec![nx("NOERROR")])));
        assert!(answered(&lookups(vec![
            json!({ "Error": "Timeout" }),
            nx("NXDOMAIN")
        ])));
        assert!(!answered(&lookups(vec![nx("SERVFAIL")])));
        assert!(!answered(&lookups(vec![json!({ "Error": "Timeout" })])));
        assert!(!answered(&lookups(vec![])));
    }
}
