use std::collections::HashMap;
use std::net::IpAddr;

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
    Pass,
    Info,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, ToSchema)]
pub enum Grade {
    F,
    D,
    C,
    B,
    A,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema,
)]
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
    },
}

impl From<SseEvent> for axum::response::sse::Event {
    fn from(event: SseEvent) -> Self {
        axum::response::sse::Event::default()
            .data(serde_json::to_string(&event).expect("SseEvent serialization"))
    }
}

/// Collected results from all category checks, used by cross-validation.
#[allow(dead_code)]
pub struct AllResults {
    pub mx: CheckResult,
    pub mx_hosts: Vec<String>,
    pub mx_ips: Vec<IpAddr>,
    pub null_mx: bool,
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
    pub dnssec_ad: bool,
    pub bimi: CheckResult,
    pub bimi_present: bool,
    pub fcrdns: CheckResult,
    pub fcrdns_all_pass: bool,
    pub dnsbl: CheckResult,
}

pub struct SpfFlat {
    pub authorized_prefixes: Vec<ipnet::IpNet>,
}

#[allow(dead_code)]
pub struct MtaStsInfo {
    pub dns_id: String,
    pub policy_id: Option<String>,
    pub mode: Option<String>,
    pub mx_patterns: Vec<String>,
}
