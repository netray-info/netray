pub mod caa;
pub mod https_record;
pub mod tlsa;

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use mhost::RecordType;
use mhost::resolver::lookup::LookupResult;
use mhost::resolver::{Lookups, ResolverGroup, ResolverGroupBuilder, UniQuery};
use mhost::resources::RData;

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

    /// The CAA records that apply to `hostname`, with the answered records of the level they
    /// were found at; `None` when the lookup failed.
    pub async fn lookup_caa(&self, hostname: &str) -> Option<(CaaLookup, Vec<AnsweredRecord>)> {
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

/// A record as answered: its owner, its data in presentation form and the highest TTL across
/// the resolvers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsweredRecord {
    pub owner: String,
    pub data: String,
    pub ttl: u32,
}

/// The records of `lookups` that `present` renders, each (owner, data) once with the highest
/// TTL seen, in (owner, data) order.
fn answered_records(
    lookups: &Lookups,
    present: impl Fn(&RData) -> Option<String>,
) -> Vec<AnsweredRecord> {
    let mut ttls: BTreeMap<(String, String), u32> = BTreeMap::new();
    for record in lookups.records() {
        if let Some(data) = present(record.data()) {
            let ttl = ttls.entry((record.name().to_string(), data)).or_default();
            *ttl = (*ttl).max(record.ttl());
        }
    }
    ttls.into_iter()
        .map(|((owner, data), ttl)| AnsweredRecord { owner, data, ttl })
        .collect()
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

    fn caa_rr(owner: &str, ttl: u32, critical: bool, tag: &str, value: &str) -> Value {
        json!({
            "name": owner,
            "type": "CAA",
            "ttl": ttl,
            "data": { "CAA": { "issuer_critical": critical, "tag": tag, "value": value } }
        })
    }

    fn response(records: Vec<Value>) -> Value {
        json!({ "Response": {
            "records": records,
            "response_time": { "secs": 0, "nanos": 5_000_000 },
            "valid_until": "2026-01-01T00:00:00Z"
        }})
    }

    fn caa_answered(lookups: &Lookups) -> Vec<AnsweredRecord> {
        answered_records(lookups, |data| match data {
            RData::CAA(caa) => Some(caa::presentation(caa)),
            _ => None,
        })
    }

    fn answered_rr(owner: &str, data: &str, ttl: u32) -> AnsweredRecord {
        AnsweredRecord {
            owner: owner.into(),
            data: data.into(),
            ttl,
        }
    }

    #[test]
    fn answered_records_keep_the_highest_ttl_across_resolvers() {
        let got = caa_answered(&lookups(vec![
            response(vec![caa_rr(
                "example.com.",
                300,
                false,
                "issue",
                "ca.example.com",
            )]),
            response(vec![caa_rr(
                "example.com.",
                3600,
                false,
                "issue",
                "ca.example.com",
            )]),
            response(vec![caa_rr(
                "example.com.",
                60,
                false,
                "issue",
                "ca.example.com",
            )]),
        ]));
        assert_eq!(
            got,
            [answered_rr(
                "example.com.",
                "0 issue \"ca.example.com\"",
                3600
            )]
        );
    }

    #[test]
    fn answered_records_dedupe_by_owner_and_data() {
        let rr = caa_rr("example.com.", 300, false, "issue", "ca.example.com");
        let got = caa_answered(&lookups(vec![
            response(vec![rr.clone(), rr.clone()]),
            response(vec![rr]),
        ]));
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn answered_records_are_in_owner_then_data_order() {
        let got = caa_answered(&lookups(vec![
            response(vec![
                caa_rr("www.example.com.", 300, false, "issue", "b.example.com"),
                caa_rr("example.com.", 300, false, "issuewild", "a.example.com"),
            ]),
            response(vec![caa_rr(
                "example.com.",
                300,
                false,
                "issue",
                "c.example.com",
            )]),
        ]));
        let order: Vec<(&str, &str)> = got
            .iter()
            .map(|r| (r.owner.as_str(), r.data.as_str()))
            .collect();
        assert_eq!(
            order,
            [
                ("example.com.", "0 issue \"c.example.com\""),
                ("example.com.", "0 issuewild \"a.example.com\""),
                ("www.example.com.", "0 issue \"b.example.com\""),
            ]
        );
    }

    #[test]
    fn caa_presentation_carries_the_flags_byte() {
        let got = caa_answered(&lookups(vec![response(vec![
            caa_rr("example.com.", 300, false, "issue", "ca.example.com"),
            caa_rr("example.com.", 300, true, "tbs", "unknown"),
        ])]));
        let data: Vec<&str> = got.iter().map(|r| r.data.as_str()).collect();
        assert_eq!(data, ["0 issue \"ca.example.com\"", "128 tbs \"unknown\""]);
    }
}
