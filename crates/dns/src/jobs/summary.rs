//! The summary of a batch job: every lookup as a per-server answer, the answers of one record
//! type compared, and the servers' counts and median response times.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use chrono::{NaiveDate, Utc};
use mhost::nameserver::NameServerConfig;
use mhost::resolver::Lookup;
use mhost::resolver::lookup::LookupResult;
use mhost::resources::RData;
use serde::Serialize;
use utoipa::ToSchema;

use super::events::{AuthLookup, AuthResult, BatchLookups, DnsBatch, WireDuration};
use crate::module::rdata_text;

/// What one server answered for one record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnswerOutcome {
    Answered,
    NoRecords,
    Nxdomain,
    Unmeasured,
}

impl AnswerOutcome {
    fn measured(self) -> bool {
        self != Self::Unmeasured
    }
}

/// Whether the servers asked for one record type answered alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Agreement {
    Agree,
    Diverge,
    Single,
    Unmeasured,
}

/// One record of a server's answer.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct DnsRecordDetail {
    pub name: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub ttl: u32,
    /// The record data in zone-file form.
    pub data: String,
    /// Days since the date an SOA serial in `YYYYMMDDnn` form names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soa_serial_age_days: Option<i64>,
}

/// One server's answer to one record type.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct DnsServerAnswer {
    /// The server as mhost names it, e.g. `udp:8.8.8.8:53,name=Google 1`.
    pub server: String,
    /// The server's socket address.
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `udp`, `tcp`, `tls` or `https`.
    pub transport: String,
    /// `recursive` or `authoritative` (authcompare only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub outcome: AnswerOutcome,
    /// The response code of an answer that is no answer, e.g. `SERVFAIL`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_code: Option<String>,
    /// Why an unmeasured answer is no answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<f64>,
    /// Sorted by type, then data.
    pub records: Vec<DnsRecordDetail>,
}

/// The answers of every server to one record type.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct DnsTypeComparison {
    pub record_type: String,
    pub agreement: Agreement,
    /// Sorted by source, transport, server.
    pub answers: Vec<DnsServerAnswer>,
}

/// One server's lookups over the whole job.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct DnsServerSummary {
    pub server: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub lookups: u32,
    pub measured: u32,
    pub unmeasured: u32,
    /// Over the measured lookups; absent when there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_response_time_ms: Option<f64>,
}

/// A batch job condensed: per record type who agreed, per server how it fared.
#[derive(Debug, Clone, PartialEq, Default, Serialize, ToSchema)]
pub struct DnsJobSummary {
    pub record_types: u32,
    pub lookups: u32,
    pub agree: u32,
    pub diverge: u32,
    pub single: u32,
    pub unmeasured: u32,
    /// Sorted by record type.
    pub types: Vec<DnsTypeComparison>,
    /// Sorted by source, transport, server.
    pub servers: Vec<DnsServerSummary>,
}

/// Summarises the batches of a finished job against today's date.
pub fn summarise(batches: &[DnsBatch]) -> DnsJobSummary {
    summarise_on(batches, Utc::now().date_naive())
}

fn summarise_on(batches: &[DnsBatch], today: NaiveDate) -> DnsJobSummary {
    let mut by_type: BTreeMap<&str, Vec<DnsServerAnswer>> = BTreeMap::new();
    for batch in batches {
        by_type
            .entry(&batch.record_type)
            .or_default()
            .extend(batch_answers(batch, today));
    }

    let mut summary = DnsJobSummary::default();
    let mut servers: BTreeMap<_, (DnsServerSummary, Vec<f64>)> = BTreeMap::new();
    for (record_type, mut answers) in by_type {
        answers.sort_by_key(answer_key);
        for answer in &answers {
            let (server, times) = servers.entry(answer_key(answer)).or_insert_with(|| {
                (
                    DnsServerSummary {
                        server: answer.server.clone(),
                        name: answer.name.clone(),
                        transport: answer.transport.clone(),
                        source: answer.source.clone(),
                        lookups: 0,
                        measured: 0,
                        unmeasured: 0,
                        median_response_time_ms: None,
                    },
                    Vec::new(),
                )
            });
            server.lookups += 1;
            if answer.outcome.measured() {
                server.measured += 1;
                times.extend(answer.response_time_ms);
            } else {
                server.unmeasured += 1;
            }
        }
        let agreement = agreement(&answers);
        match agreement {
            Agreement::Agree => summary.agree += 1,
            Agreement::Diverge => summary.diverge += 1,
            Agreement::Single => summary.single += 1,
            Agreement::Unmeasured => summary.unmeasured += 1,
        }
        summary.lookups += u32::try_from(answers.len()).unwrap_or(u32::MAX);
        summary.types.push(DnsTypeComparison {
            record_type: record_type.to_owned(),
            agreement,
            answers,
        });
    }
    summary.record_types = u32::try_from(summary.types.len()).unwrap_or(u32::MAX);
    summary.servers = servers
        .into_values()
        .map(|(mut server, times)| {
            server.median_response_time_ms = median_ms(&times);
            server
        })
        .collect();
    summary
}

fn answer_key(answer: &DnsServerAnswer) -> (Option<String>, String, String) {
    (
        answer.source.clone(),
        answer.transport.clone(),
        answer.server.clone(),
    )
}

/// The record types whose data mhost's and hickory's printers write alike.
const TEXT_ALIGNED: &[&str] = &[
    "A", "AAAA", "CNAME", "NS", "PTR", "MX", "SOA", "SRV", "TXT", "CAA", "HTTPS", "SVCB", "TLSA",
    "SSHFP", "NAPTR", "HINFO",
];

/// Whether the measured answers agree: at least two, with the same outcome and the same
/// records, TTL aside. One measured answer is `Single`, none `Unmeasured`.
pub fn agreement(answers: &[DnsServerAnswer]) -> Agreement {
    let measured: Vec<_> = answers.iter().filter(|a| a.outcome.measured()).collect();
    let Some(first) = measured.first() else {
        return Agreement::Unmeasured;
    };
    if measured.len() == 1 {
        return Agreement::Single;
    }
    // Recursive records are printed from mhost's data, authoritative ones from hickory's: the
    // two printers agree only on the types below. Across sources, any other type compares by
    // record count, so a printing difference is not reported as a divergence.
    let mixed = measured.iter().any(|a| a.source != first.source);
    let comparable = |answer: &DnsServerAnswer| {
        answer
            .records
            .iter()
            .all(|r| TEXT_ALIGNED.contains(&r.record_type.as_str()))
    };
    let by_count = mixed && !measured.iter().all(|a| comparable(a));
    let data = |answer: &DnsServerAnswer| -> Vec<(String, String)> {
        let mut data: Vec<_> = answer
            .records
            .iter()
            .map(|r| {
                let text = if by_count {
                    String::new()
                } else {
                    r.data.clone()
                };
                (r.record_type.clone(), text)
            })
            .collect();
        data.sort();
        data
    };
    let reference = data(first);
    if measured
        .iter()
        .all(|a| a.outcome == first.outcome && data(a) == reference)
    {
        Agreement::Agree
    } else {
        Agreement::Diverge
    }
}

/// The median of `values`; the mean of the two middle values for an even count.
pub fn median_ms(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    Some(if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    })
}

/// The age in days of an SOA serial in `YYYYMMDDnn` form; `None` for a serial that is no real
/// date between 1990 and 2100.
pub fn soa_serial_age_days(serial: u32, today: NaiveDate) -> Option<i64> {
    let year = i32::try_from(serial / 1_000_000).ok()?;
    if !(1990..=2100).contains(&year) {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(year, (serial / 10_000) % 100, (serial / 100) % 100)?;
    Some((today - date).num_days())
}

fn millis(secs: u64, nanos: u32) -> f64 {
    secs as f64 * 1000.0 + f64::from(nanos) / 1e6
}

fn batch_answers(batch: &DnsBatch, today: NaiveDate) -> Vec<DnsServerAnswer> {
    let source = batch.source.clone();
    match &batch.lookups {
        BatchLookups::Resolved(lookups) => lookups
            .iter()
            .map(|lookup| resolved_answer(lookup, source.clone(), today))
            .collect(),
        BatchLookups::Authoritative(auth) => auth
            .lookups
            .iter()
            .map(|lookup| auth_answer(lookup, source.clone(), today))
            .collect(),
    }
}

struct Endpoint {
    address: String,
    name: Option<String>,
    transport: &'static str,
}

fn endpoint(config: &NameServerConfig) -> Endpoint {
    let (ip_addr, port, name, transport) = match config {
        NameServerConfig::Udp {
            ip_addr,
            port,
            name,
        } => (ip_addr, port, name, "udp"),
        NameServerConfig::Tcp {
            ip_addr,
            port,
            name,
        } => (ip_addr, port, name, "tcp"),
        NameServerConfig::Tls {
            ip_addr,
            port,
            name,
            ..
        } => (ip_addr, port, name, "tls"),
        NameServerConfig::Https {
            ip_addr,
            port,
            name,
            ..
        } => (ip_addr, port, name, "https"),
    };
    Endpoint {
        address: SocketAddr::new(*ip_addr, *port).to_string(),
        name: name.clone(),
        transport,
    }
}

fn resolved_answer(lookup: &Lookup, source: Option<String>, today: NaiveDate) -> DnsServerAnswer {
    let endpoint = endpoint(lookup.name_server());
    let mut answer = DnsServerAnswer {
        server: lookup.name_server().to_string(),
        address: endpoint.address,
        name: endpoint.name,
        transport: endpoint.transport.to_owned(),
        source,
        outcome: AnswerOutcome::Unmeasured,
        response_code: None,
        error: None,
        response_time_ms: None,
        records: Vec::new(),
    };
    answer.response_time_ms = lookup
        .result()
        .response_time()
        .map(|time| millis(time.as_secs(), time.subsec_nanos()));
    match lookup.result() {
        LookupResult::Response(response) => {
            answer.records = response
                .records()
                .iter()
                .map(|record| DnsRecordDetail {
                    name: record.name().to_string(),
                    record_type: record.record_type().to_string(),
                    ttl: record.ttl(),
                    data: rdata_text(record.data()),
                    soa_serial_age_days: match record.data() {
                        RData::SOA(soa) => soa_serial_age_days(soa.serial(), today),
                        _ => None,
                    },
                })
                .collect();
            answer.outcome = if answer.records.is_empty() {
                AnswerOutcome::NoRecords
            } else {
                AnswerOutcome::Answered
            };
        }
        LookupResult::NxDomain(nx) => {
            let code = nx.response_code().map(|code| code.to_string());
            if nx.is_definite() {
                answer.outcome = if code.as_deref() == Some("NXDOMAIN") {
                    AnswerOutcome::Nxdomain
                } else {
                    AnswerOutcome::NoRecords
                };
            } else {
                answer.error = Some(if nx.is_referral() {
                    "referral, no answer".to_owned()
                } else {
                    code.clone().unwrap_or_else(|| "no answer".to_owned())
                });
                answer.response_code = code;
            }
        }
        LookupResult::Error(error) => answer.error = Some(error.to_string()),
    }
    sort_records(&mut answer.records);
    answer
}

fn auth_endpoint(name_server: &str) -> Endpoint {
    let (transport, address) = match name_server.split_once(':') {
        Some((prefix @ ("udp" | "tcp" | "tls" | "https"), rest)) => (
            match prefix {
                "tcp" => "tcp",
                "tls" => "tls",
                "https" => "https",
                _ => "udp",
            },
            rest,
        ),
        _ => ("udp", name_server),
    };
    Endpoint {
        address: address.to_owned(),
        name: None,
        transport,
    }
}

fn auth_answer(lookup: &AuthLookup, source: Option<String>, today: NaiveDate) -> DnsServerAnswer {
    let endpoint = auth_endpoint(&lookup.name_server);
    let mut answer = DnsServerAnswer {
        server: lookup.name_server.clone(),
        address: endpoint.address,
        name: endpoint.name,
        transport: endpoint.transport.to_owned(),
        source,
        outcome: AnswerOutcome::Unmeasured,
        response_code: None,
        error: None,
        response_time_ms: None,
        records: Vec::new(),
    };
    match &lookup.result {
        AuthResult::Response {
            records,
            response_time,
            failure,
            failure_code,
        } => {
            let WireDuration { secs, nanos } = *response_time;
            answer.response_time_ms = Some(millis(secs, nanos));
            if let Some(failure) = failure {
                answer.response_code = failure_code.clone();
                answer.error = Some(failure.clone());
            } else {
                answer.records = records
                    .iter()
                    .map(|record| DnsRecordDetail {
                        name: record.name.clone(),
                        record_type: record.record_type.clone(),
                        ttl: record.ttl,
                        data: record.presentation.clone(),
                        soa_serial_age_days: (record.record_type == "SOA")
                            .then(|| {
                                let serial = record.presentation.split_whitespace().nth(2)?;
                                soa_serial_age_days(serial.parse().ok()?, today)
                            })
                            .flatten(),
                    })
                    .collect();
                answer.outcome = if answer.records.is_empty() {
                    AnswerOutcome::NoRecords
                } else {
                    AnswerOutcome::Answered
                };
            }
        }
        AuthResult::NxDomain { response_time } => {
            let WireDuration { secs, nanos } = *response_time;
            answer.response_time_ms = Some(millis(secs, nanos));
            answer.outcome = AnswerOutcome::Nxdomain;
        }
    }
    sort_records(&mut answer.records);
    answer
}

fn sort_records(records: &mut [DnsRecordDetail]) {
    records.sort_by(|a, b| (&a.record_type, &a.data).cmp(&(&b.record_type, &b.data)));
}
