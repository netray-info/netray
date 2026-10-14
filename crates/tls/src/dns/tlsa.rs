use std::collections::HashSet;

use mhost::RecordType;
use mhost::resolver::{ResolverGroup, UniQuery};
use mhost::resources::RData;
use mhost::resources::rdata::TLSA;
use serde::Serialize;
use utoipa::ToSchema;

use super::AnsweredRecord;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TlsaLookup {
    pub records: Vec<TlsaRecord>,
    pub dnssec_signed: bool,
    /// The answered records with owner and TTL, not serialized.
    #[serde(skip)]
    #[schema(ignore)]
    pub answered: Vec<AnsweredRecord>,
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
    pub cert_data: Vec<u8>,
}

impl TlsaLookup {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Fetch TLSA records for `_port._tcp.hostname`; `None` when the lookup failed or no
/// nameserver answered definitively.
pub async fn lookup_tlsa(
    resolvers: &ResolverGroup,
    hostname: &str,
    port: u16,
) -> Option<TlsaLookup> {
    let hostname = hostname.trim_end_matches('.');
    let qname = format!("_{port}._tcp.{hostname}");

    let query = UniQuery::new(&qname, RecordType::TLSA).ok()?;
    let lookups = resolvers.lookup(query).await.ok()?;
    if !super::answered(&lookups) {
        return None;
    }

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
                Some(TlsaRecord {
                    cert_usage: usage,
                    selector,
                    matching_type: matching,
                    display: presentation(tlsa),
                    cert_data: data,
                })
            } else {
                None
            }
        })
        .collect();

    let answered = super::answered_records(&lookups, |data| match data {
        RData::TLSA(tlsa) => Some(presentation(tlsa)),
        _ => None,
    });

    Some(TlsaLookup {
        records,
        dnssec_signed: false, // DNSSEC validation deferred
        answered,
    })
}

/// A TLSA record's data in presentation form, `usage selector matching hex`.
fn presentation(tlsa: &TLSA) -> String {
    format!(
        "{} {} {} {}",
        cert_usage_to_u8(tlsa.cert_usage()),
        selector_to_u8(tlsa.selector()),
        matching_to_u8(tlsa.matching()),
        hex_encode(tlsa.cert_data())
    )
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
