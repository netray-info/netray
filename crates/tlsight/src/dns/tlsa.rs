use std::collections::HashSet;

use mhost::RecordType;
use mhost::resolver::{ResolverGroup, UniQuery};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TlsaLookup {
    pub records: Vec<TlsaRecord>,
    pub dnssec_signed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TlsaRecord {
    pub cert_usage: u8,
    pub selector: u8,
    pub matching_type: u8,
    /// Hex-encoded certificate association data.
    pub display: String,
    /// Raw certificate association data (not serialized). Used by DANE matching.
    #[serde(skip)]
    #[schema(ignore)]
    #[allow(dead_code)] // Used in validate::dane tests, runtime DANE deferred until DNSSEC
    pub cert_data: Vec<u8>,
}

impl TlsaLookup {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Fetch TLSA records for `_port._tcp.hostname`.
pub async fn lookup_tlsa(resolvers: &ResolverGroup, hostname: &str, port: u16) -> TlsaLookup {
    let hostname = hostname.trim_end_matches('.');
    let qname = format!("_{port}._tcp.{hostname}");

    let query = match UniQuery::new(&qname, RecordType::TLSA) {
        Ok(q) => q,
        Err(_) => {
            return TlsaLookup {
                records: vec![],
                dnssec_signed: false,
            };
        }
    };

    let lookups = match resolvers.lookup(query).await {
        Ok(l) => l,
        Err(_) => {
            return TlsaLookup {
                records: vec![],
                dnssec_signed: false,
            };
        }
    };

    // Deduplicate by (usage, selector, matching, data)
    let mut seen = HashSet::new();
    let records: Vec<TlsaRecord> = lookups
        .tlsa()
        .into_iter()
        .filter_map(|tlsa| {
            let usage = cert_usage_to_u8(tlsa.cert_usage());
            let selector = selector_to_u8(tlsa.selector());
            let matching = matching_to_u8(tlsa.matching());
            let data = tlsa.cert_data().to_vec();
            let key = (usage, selector, matching, data.clone());
            if seen.insert(key) {
                let hex = hex_encode(&data);
                Some(TlsaRecord {
                    cert_usage: usage,
                    selector,
                    matching_type: matching,
                    display: format!("{usage} {selector} {matching} {hex}"),
                    cert_data: data,
                })
            } else {
                None
            }
        })
        .collect();

    TlsaLookup {
        records,
        dnssec_signed: false, // DNSSEC validation deferred
    }
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
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
