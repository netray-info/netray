use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Enrichment data for an IP address, included in MX results for frontend display.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IpEnrichment {
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asn: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_type: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Skip,
    Info,
    Pass,
    Warn,
    Fail,
}

/// Maps onto the shared model vocabulary; `Skip` is `NotApplicable`, `Info` is `Pass`.
/// The caller keeps `Info`'s message as a finding.
impl From<Verdict> for netray_model::Status {
    fn from(verdict: Verdict) -> Self {
        match verdict {
            Verdict::Skip => netray_model::Status::NotApplicable,
            Verdict::Info => netray_model::Status::Pass,
            Verdict::Pass => netray_model::Status::Pass,
            Verdict::Warn => netray_model::Status::Warn,
            Verdict::Fail => netray_model::Status::Fail,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, ToSchema)]
pub enum Grade {
    F,
    D,
    C,
    B,
    A,
    #[serde(rename = "skipped")]
    Skipped,
}

impl Grade {
    pub fn as_str(self) -> &'static str {
        match self {
            Grade::A => "A",
            Grade::B => "B",
            Grade::C => "C",
            Grade::D => "D",
            Grade::F => "F",
            Grade::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Mx,
    Spf,
    Dkim,
    Dmarc,
    MtaSts,
    TlsRpt,
    Dane,
    Dnssec,
    Bimi,
    Fcrdns,
    Dnsbl,
    CrossValidation,
}

impl Category {
    /// Every category, in declaration order.
    pub const ALL: [Category; 12] = [
        Category::Mx,
        Category::Spf,
        Category::Dkim,
        Category::Dmarc,
        Category::MtaSts,
        Category::TlsRpt,
        Category::Dane,
        Category::Dnssec,
        Category::Bimi,
        Category::Fcrdns,
        Category::Dnsbl,
        Category::CrossValidation,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Self::Mx => "MX",
            Self::Spf => "SPF",
            Self::Dkim => "DKIM",
            Self::Dmarc => "DMARC",
            Self::MtaSts => "MTA-STS",
            Self::TlsRpt => "TLS-RPT",
            Self::Dane => "DANE",
            Self::Dnssec => "DNSSEC",
            Self::Bimi => "BIMI",
            Self::Fcrdns => "FCrDNS",
            Self::Dnsbl => "DNSBL",
            Self::CrossValidation => "Cross-Validation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubCheck {
    pub name: String,
    pub verdict: Verdict,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CheckResult {
    pub category: Category,
    pub verdict: Verdict,
    pub title: String,
    pub detail: String,
    pub sub_checks: Vec<SubCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enrichment: Vec<IpEnrichment>,
}

impl CheckResult {
    pub fn new(category: Category, sub_checks: Vec<SubCheck>, detail: String) -> Self {
        let verdict = sub_checks
            .iter()
            .map(|s| s.verdict)
            .max()
            .unwrap_or(Verdict::Pass);
        let title = category.title().to_string();
        Self {
            category,
            verdict,
            title,
            detail,
            sub_checks,
            enrichment: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseEvent {
    Category(CheckResult),
    Summary {
        grade: Grade,
        verdicts: HashMap<String, Verdict>,
        duration_ms: u64,
    },
    /// The evidence blocks of the run, sent once right before `Summary`.
    Evidence {
        blocks: Vec<netray_model::evidence::Block>,
    },
}

impl From<SseEvent> for axum::response::sse::Event {
    fn from(event: SseEvent) -> Self {
        axum::response::sse::Event::default()
            .data(serde_json::to_string(&event).expect("SseEvent serialization"))
    }
}

/// Collected results from all category checks, used by cross-validation.
pub struct AllResults {
    pub mx: CheckResult,
    pub mx_hosts: Vec<String>,
    pub mx_ips: Vec<IpAddr>,
    pub null_mx: bool,
    pub sends_no_mail: bool,
    pub spf: CheckResult,
    pub spf_flat: Option<SpfFlat>,
    pub spf_has_dash_all: bool,
    pub dkim: CheckResult,
    pub dkim_found: bool,
    pub dmarc: CheckResult,
    pub dmarc_policy: Option<String>,
    pub dmarc_sp: Option<String>,
    pub dmarc_rua_external_auth_ok: bool,
    pub mta_sts: CheckResult,
    pub mta_sts_present: bool,
    pub mta_sts_info: Option<MtaStsInfo>,
    pub tls_rpt: CheckResult,
    pub tls_rpt_present: bool,
    pub dane: CheckResult,
    pub dane_has_tlsa: bool,
    pub dnssec: CheckResult,
    pub dnssec_dnskey_present: bool,
    pub bimi: CheckResult,
    pub bimi_present: bool,
    pub fcrdns: CheckResult,
    pub fcrdns_all_pass: bool,
    pub dnsbl: CheckResult,
}

/// What the checks measured, kept for the evidence blocks; `None` where the check's lookup got
/// no answer.
#[derive(Debug, Clone, Default)]
pub struct Measured {
    pub mx: Option<Vec<MxHost>>,
    pub spf: Option<SpfMeasured>,
    pub dmarc: Option<DmarcMeasured>,
    /// The `v=TLSRPTv1` records at `_smtp._tls.<domain>`.
    pub tls_rpt: Option<Vec<String>>,
    pub bimi: Option<BimiMeasured>,
    pub mta_sts: Option<MtaStsMeasured>,
    /// Whether the domain has DNSKEY records.
    pub dnssec: Option<bool>,
    pub dkim: Option<Vec<DkimKey>>,
    pub dane: Option<Vec<DaneRecord>>,
    pub dnsbl: Option<Vec<DnsblAnswer>>,
    pub fcrdns: Option<Vec<FcrdnsAnswer>>,
}

/// One MX record and the addresses its host resolved to.
#[derive(Debug, Clone)]
pub struct MxHost {
    pub preference: u16,
    pub host: String,
    /// The CNAME target of the host, empty when it has none; `None` when the lookup failed.
    pub cname: Option<String>,
    pub addresses: Vec<IpAddr>,
}

/// The `v=spf1` records; the authorised prefixes and the lookup counts when exactly one record
/// was evaluated.
#[derive(Debug, Clone, Default)]
pub struct SpfMeasured {
    pub records: Vec<String>,
    pub authorized_prefixes: Vec<ipnet::IpNet>,
    pub lookup_count: Option<u16>,
    pub void_lookups: Option<u16>,
}

/// The `v=DMARC1` records at `_dmarc.<domain>` and whether the external report destinations
/// authorise them; `None` when no external destination was checked in full.
#[derive(Debug, Clone, Default)]
pub struct DmarcMeasured {
    pub records: Vec<String>,
    pub rua_authorized: Option<bool>,
    pub ruf_authorized: Option<bool>,
}

/// The `v=BIMI1` records at `default._bimi.<domain>` and the outcome kind of the logo fetch.
#[derive(Debug, Clone, Default)]
pub struct BimiMeasured {
    pub records: Vec<String>,
    pub logo_fetch: Option<String>,
}

/// The `v=STSv1` records at `_mta-sts.<domain>` and what the policy fetch got: its status and
/// Content-Type, the policy file when it came with a 2xx status as UTF-8, or the error kind.
#[derive(Debug, Clone, Default)]
pub struct MtaStsMeasured {
    pub records: Vec<String>,
    pub policy: Option<String>,
    pub policy_status: Option<u16>,
    pub content_type: Option<String>,
    pub fetch_error: Option<String>,
}

/// A DKIM key record of a selector: its state (`ok`, `revoked`, `cname_loop`, `parse_error`,
/// `no_key`), its key type and its size when read.
#[derive(Debug, Clone)]
pub struct DkimKey {
    pub selector: String,
    pub state: String,
    pub key_type: String,
    pub bits: Option<usize>,
}

/// A TLSA record at `_25._tcp.<host>`.
#[derive(Debug, Clone)]
pub struct DaneRecord {
    pub host: String,
    pub usage: u8,
    pub selector: u8,
    pub matching_type: u8,
}

/// One DNSBL query: the zone, the MX address or domain asked about, whether the zone answered
/// (not on a timeout, an error or a policy response) and the raw answer codes.
#[derive(Debug, Clone)]
pub struct DnsblAnswer {
    pub zone: String,
    pub target: String,
    pub answered: bool,
    pub answer: Vec<Ipv4Addr>,
}

/// The PTR names of one MX address and whether one of them resolves back to it.
#[derive(Debug, Clone)]
pub struct FcrdnsAnswer {
    pub address: IpAddr,
    pub ptr: Vec<String>,
    pub confirmed: bool,
}

pub struct SpfFlat {
    pub authorized_prefixes: Vec<ipnet::IpNet>,
}

pub struct MtaStsInfo {
    pub dns_id: String,
    pub policy_id: Option<String>,
    #[allow(dead_code)]
    pub mode: Option<String>,
    pub mx_patterns: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(verdict: Verdict) -> SubCheck {
        SubCheck {
            name: "x".to_string(),
            verdict,
            detail: String::new(),
        }
    }

    fn category_verdict(verdicts: &[Verdict]) -> Verdict {
        let subs = verdicts.iter().copied().map(sub).collect();
        CheckResult::new(Category::Spf, subs, String::new()).verdict
    }

    #[test]
    fn c7_info_ranks_below_pass() {
        assert!(Verdict::Info < Verdict::Pass);
    }

    #[test]
    fn c7_category_verdict_info_does_not_outrank_pass() {
        let rows: &[(&[Verdict], Verdict)] = &[
            (&[Verdict::Pass, Verdict::Info], Verdict::Pass),
            (&[Verdict::Info], Verdict::Info),
            (&[Verdict::Info, Verdict::Warn], Verdict::Warn),
            (&[], Verdict::Pass),
        ];
        for (subs, expected) in rows {
            assert_eq!(category_verdict(subs), *expected, "sub-checks {subs:?}");
        }
    }
}
