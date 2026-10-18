//! The collected DNS jobs as evidence blocks, for the inspector page. Every block id is in the
//! `dns` module; a job with nothing to show still yields its one block, empty.

use std::collections::{BTreeMap, BTreeSet};

use mhost::RecordType;
use mhost::resolver::Lookup;
use mhost::resolver::lookup::LookupResult;
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};

use super::events::{
    AuthResult, BatchLookups, DnsAuthCompareResult, DnsBatch, DnsCompareResult, DnsDnssecResult,
    DnsQueryResult, DnsTraceResult,
};
use crate::dns_trace::ServerOutcome;
use crate::module::{block_id, rdata_text, records_block};

fn text(s: impl Into<String>) -> Value {
    Value::Text(s.into())
}

fn count(n: usize) -> Value {
    Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
}

fn millis(ms: f64) -> Value {
    Value::Duration(ms.max(0.0).round() as u64)
}

fn block(name: &str, title: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(&format!("dns.{name}")).expect("static block id"),
        title: title.to_owned(),
        shape,
    }
}

fn node(fields: Vec<(&str, Value)>, children: Vec<Node>) -> Node {
    Node {
        fields: fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        children,
    }
}

/// The records of `lookups` as text, each once, in order.
fn records_text<'a>(lookups: impl IntoIterator<Item = &'a Lookup>) -> String {
    lookups
        .into_iter()
        .flat_map(|lookup| lookup.records())
        .map(|record| rdata_text(record.data()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("; ")
}

/// Why `lookup` is no answer, or `None` when it is one: a response or a definite negative.
fn lookup_error(lookup: &Lookup) -> Option<String> {
    match lookup.result() {
        LookupResult::Response(_) => None,
        LookupResult::NxDomain(nx) if nx.is_definite() => None,
        LookupResult::NxDomain(nx) if nx.is_referral() => Some("referral, no answer".to_owned()),
        LookupResult::NxDomain(nx) => Some(match nx.response_code() {
            Some(code) => code.to_string(),
            None => "no answer".to_owned(),
        }),
        LookupResult::Error(error) => Some(error.to_string()),
    }
}

/// The `records` and `outcome` cells of `lookups`: the answered lookups' records with
/// `answered` or `no records`, else the first error as both (Rule M).
fn outcome_cells<'a>(lookups: impl IntoIterator<Item = &'a Lookup>) -> (String, String) {
    let mut answered = Vec::new();
    let mut error = None;
    for lookup in lookups {
        match lookup_error(lookup) {
            None => answered.push(lookup),
            Some(message) => {
                error.get_or_insert(message);
            }
        }
    }
    if answered.is_empty() {
        let message = error.unwrap_or_else(|| "no answer".to_owned());
        return (message.clone(), message);
    }
    let records = records_text(answered);
    let outcome = if records.is_empty() {
        "no records"
    } else {
        "answered"
    };
    (records, outcome.to_owned())
}

/// The `records` and `outcome` cells of a compare batch.
fn batch_cells(batch: &DnsBatch) -> (String, String) {
    match &batch.lookups {
        BatchLookups::Resolved(lookups) => outcome_cells(lookups.iter()),
        BatchLookups::Authoritative(_) => outcome_cells([]),
    }
}

/// A `fields` block `dns.<type>` for a record type no lookup answered (Rule M): its outcome
/// "not measured" and the first failed lookup's reason.
fn not_measured_block(record_type: RecordType, lookups: &[&Lookup]) -> Block {
    let reason = lookups
        .iter()
        .find_map(|lookup| lookup_error(lookup))
        .unwrap_or_else(|| "no answer".to_owned());
    let title = record_type.to_string();
    Block {
        id: block_id(&title),
        title,
        shape: Shape::Fields(vec![
            ("outcome".to_owned(), text("not measured")),
            ("reason".to_owned(), text(reason)),
        ]),
    }
}

/// One block per record type of a query job, `dns.<type lowercase>`, in id order: the
/// `records` of an answered type, a "not measured" `fields` block with the reason of a type no
/// lookup answered (Rule M). A type whose name is no block id is left out.
pub fn query_blocks(result: &DnsQueryResult) -> Vec<Block> {
    let mut by_type = BTreeMap::new();
    for batch in &result.batches {
        let BatchLookups::Resolved(lookups) = &batch.lookups else {
            continue;
        };
        for lookup in lookups.iter() {
            let record_type = lookup.query().record_type();
            let id = format!("dns.{}", record_type.to_string().to_lowercase());
            if BlockId::parse(&id).is_err() {
                continue;
            }
            by_type
                .entry(id)
                .or_insert_with(|| (record_type, Vec::new()))
                .1
                .push(lookup);
        }
    }
    by_type
        .into_values()
        .map(|(record_type, lookups)| {
            records_block(record_type, lookups.iter().copied())
                .unwrap_or_else(|| not_measured_block(record_type, &lookups))
        })
        .collect()
}

/// The one `table` block `dns.compare` of a compare job: a row per transport and record type,
/// keyed `<transport> <type>`.
pub fn compare_blocks(result: &DnsCompareResult) -> Vec<Block> {
    let mut rows: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    for batch in &result.batches {
        let key = (
            batch.transport.clone().unwrap_or_default(),
            batch.record_type.clone(),
        );
        rows.insert(key, batch_cells(batch));
    }
    vec![block(
        "compare",
        "Transport comparison",
        Shape::Table {
            key: "lookup".to_owned(),
            columns: ["lookup", "transport", "type", "records", "outcome"]
                .map(str::to_owned)
                .to_vec(),
            rows: rows
                .into_iter()
                .map(|((transport, record_type), (records, outcome))| {
                    vec![
                        text(format!("{transport} {record_type}")),
                        text(transport),
                        text(record_type),
                        text(records),
                        text(outcome),
                    ]
                })
                .collect(),
        },
    )]
}

fn authcompare_row(
    source: &str,
    server: &str,
    record_type: &str,
    (records, outcome): (String, String),
) -> Vec<Value> {
    vec![
        text(format!("{source} {server} {record_type}")),
        text(source),
        text(server),
        text(record_type),
        text(records),
        text(outcome),
    ]
}

/// The one `table` block `dns.authcompare` of an authcompare job: a row per source, server and
/// record type, keyed `<source> <server> <type>`.
pub fn authcompare_blocks(result: &DnsAuthCompareResult) -> Vec<Block> {
    let mut rows = Vec::new();
    for batch in &result.batches {
        let source = batch.source.clone().unwrap_or_default();
        match &batch.lookups {
            BatchLookups::Resolved(lookups) => {
                for lookup in lookups.iter() {
                    rows.push(authcompare_row(
                        &source,
                        &lookup.name_server().to_string(),
                        &batch.record_type,
                        outcome_cells([lookup]),
                    ));
                }
            }
            BatchLookups::Authoritative(auth) => {
                for lookup in &auth.lookups {
                    if let AuthResult::Response {
                        failure: Some(failure),
                        ..
                    } = &lookup.result
                    {
                        rows.push(authcompare_row(
                            &source,
                            &lookup.name_server,
                            &lookup.query.record_type,
                            (failure.clone(), failure.clone()),
                        ));
                        continue;
                    }
                    let records = match &lookup.result {
                        AuthResult::Response { records, .. } => records
                            .iter()
                            .map(|r| r.presentation.clone())
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join("; "),
                        AuthResult::NxDomain { .. } => String::new(),
                    };
                    let outcome = if records.is_empty() {
                        "no records"
                    } else {
                        "answered"
                    };
                    rows.push(authcompare_row(
                        &source,
                        &lookup.name_server,
                        &lookup.query.record_type,
                        (records, outcome.to_owned()),
                    ));
                }
            }
        }
    }
    vec![block(
        "authcompare",
        "Authoritative comparison",
        Shape::Table {
            key: "lookup".to_owned(),
            columns: ["lookup", "source", "server", "type", "records", "outcome"]
                .map(str::to_owned)
                .to_vec(),
            rows,
        },
    )]
}

/// The one `tree` block `dns.trace` of a trace job: a node per hop, its children the servers
/// queried with address and latency.
pub fn trace_blocks(result: &DnsTraceResult) -> Vec<Block> {
    let mut hops: Vec<_> = result.hops.iter().map(|event| &event.hop).collect();
    hops.sort_by_key(|hop| hop.level);
    let children = hops
        .into_iter()
        .map(|hop| {
            let servers = hop
                .server_results
                .iter()
                .map(|server| {
                    let outcome = match &server.outcome {
                        ServerOutcome::Referral => "referral",
                        ServerOutcome::Answer => "answer",
                        ServerOutcome::Error { .. } => "error",
                    };
                    let mut fields = vec![("address", Value::Addr(server.server_ip))];
                    if !matches!(server.outcome, ServerOutcome::Error { .. }) {
                        fields.push(("latency", millis(server.latency_ms)));
                    }
                    fields.push(("outcome", text(outcome)));
                    node(fields, Vec::new())
                })
                .collect();
            node(
                vec![
                    ("level", count(hop.level)),
                    ("zone", text(&hop.zone)),
                    ("servers_queried", count(hop.servers_queried)),
                ],
                servers,
            )
        })
        .collect::<Vec<_>>();
    vec![block(
        "trace",
        "Delegation trace",
        Shape::Tree(node(vec![("hops", count(children.len()))], children)),
    )]
}

/// The one `tree` block `dns.dnssec_chain` of a dnssec job: a node per zone level, its children
/// the findings.
pub fn dnssec_blocks(result: &DnsDnssecResult) -> Vec<Block> {
    let mut levels: Vec<_> = result.levels.iter().map(|event| &event.level).collect();
    levels.sort_by_key(|level| level.level);
    let children = levels
        .into_iter()
        .map(|level| {
            let findings = level
                .findings
                .iter()
                .map(|finding| {
                    node(
                        vec![
                            ("severity", text(&finding.severity)),
                            ("message", text(&finding.message)),
                        ],
                        Vec::new(),
                    )
                })
                .collect();
            let mut fields = vec![("level", count(level.level)), ("zone", text(&level.zone))];
            if level.servers_queried > 0 {
                fields.push(("latency", millis(level.latency_ms)));
            }
            node(fields, findings)
        })
        .collect::<Vec<_>>();
    vec![block(
        "dnssec_chain",
        "DNSSEC chain",
        Shape::Tree(node(vec![("levels", count(children.len()))], children)),
    )]
}
