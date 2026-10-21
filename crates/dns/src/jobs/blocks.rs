//! The collected DNS jobs as evidence blocks, for the inspector page. Every block id is in the
//! `dns` module; a job with nothing to show still yields its one block, empty.

use std::collections::{BTreeMap, BTreeSet};

use mhost::RecordType;
use mhost::resolver::Lookup;
use mhost::resolver::lookup::LookupResult;
use netray_model::evidence::{Block, BlockId, Node, Shape, Value};
use serde::Serialize;

use super::events::{
    AuthResult, BatchLookups, DnsAuthCompareResult, DnsBatch, DnsCompareResult, DnsDnssecResult,
    DnsQueryResult, DnsTraceResult,
};
use super::summary::{DnsJobSummary, DnsServerAnswer, summarise};
use crate::dns_trace::ServerOutcome;
use crate::module::{block_id, rdata_text, records_block};
use crate::record_format::rdata_texts;

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

/// A text field, left out when empty.
fn push_text(fields: &mut Vec<(String, Value)>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
        fields.push((key.to_owned(), text(value)));
    }
}

/// The wire name of a unit-variant enum, as serde writes it.
fn wire_name(value: &impl Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

/// The `(human, explain)` texts by `(type, rdata text)`.
type RecordTexts = BTreeMap<(String, String), (Option<String>, Option<String>)>;

/// The `(human, explain)` texts of the resolved records, by `(type, rdata text)`.
fn record_texts(batches: &[DnsBatch]) -> RecordTexts {
    let mut texts = BTreeMap::new();
    for batch in batches {
        let BatchLookups::Resolved(lookups) = &batch.lookups else {
            continue;
        };
        for record in lookups.iter().flat_map(|lookup| lookup.records()) {
            let pair = rdata_texts(record.data());
            if pair.0.is_some() || pair.1.is_some() {
                texts.insert(
                    (record.record_type().to_string(), rdata_text(record.data())),
                    pair,
                );
            }
        }
    }
    texts
}

fn answer_node(answer: &DnsServerAnswer, texts: &RecordTexts) -> Node {
    let mut fields = Vec::new();
    push_text(&mut fields, "server", Some(&answer.server));
    push_text(&mut fields, "address", Some(&answer.address));
    push_text(&mut fields, "name", answer.name.as_deref());
    push_text(&mut fields, "transport", Some(&answer.transport));
    push_text(&mut fields, "source", answer.source.as_deref());
    push_text(&mut fields, "outcome", Some(&wire_name(&answer.outcome)));
    push_text(
        &mut fields,
        "response_code",
        answer.response_code.as_deref(),
    );
    push_text(&mut fields, "error", answer.error.as_deref());
    if let Some(ms) = answer.response_time_ms {
        fields.push(("response_time".to_owned(), millis(ms)));
    }
    let records = answer
        .records
        .iter()
        .map(|record| {
            let mut fields = Vec::new();
            push_text(&mut fields, "name", Some(&record.name));
            push_text(&mut fields, "type", Some(&record.record_type));
            fields.push(("ttl".to_owned(), Value::Int(i64::from(record.ttl))));
            push_text(&mut fields, "data", Some(&record.data));
            if let Some(age) = record.soa_serial_age_days {
                fields.push(("soa_serial_age_days".to_owned(), Value::Int(age)));
            }
            let (human, explain) = texts
                .get(&(record.record_type.clone(), record.data.clone()))
                .cloned()
                .unwrap_or_default();
            push_text(&mut fields, "human", human.as_deref());
            push_text(&mut fields, "explain", explain.as_deref());
            Node {
                fields,
                children: Vec::new(),
            }
        })
        .collect();
    Node {
        fields,
        children: records,
    }
}

/// The `dns.answers` and `dns.servers` trees of a batch job. `done` is the job's done facts,
/// `None` while it is unfinished.
fn summary_blocks(
    mode: &str,
    batches: &[DnsBatch],
    summary: Option<&DnsJobSummary>,
    done: Option<Vec<(String, Value)>>,
) -> [Block; 2] {
    let summary = summary.cloned().unwrap_or_else(|| summarise(batches));
    let texts = record_texts(batches);
    let mut fields = vec![
        ("mode".to_owned(), text(mode)),
        ("finished".to_owned(), Value::Bool(done.is_some())),
        (
            "record_types".to_owned(),
            Value::Int(i64::from(summary.record_types)),
        ),
        ("lookups".to_owned(), Value::Int(i64::from(summary.lookups))),
        ("servers".to_owned(), count(summary.servers.len())),
        ("agree".to_owned(), Value::Int(i64::from(summary.agree))),
        ("diverge".to_owned(), Value::Int(i64::from(summary.diverge))),
        ("single".to_owned(), Value::Int(i64::from(summary.single))),
        (
            "unmeasured".to_owned(),
            Value::Int(i64::from(summary.unmeasured)),
        ),
    ];
    fields.extend(done.into_iter().flatten());
    let types = summary
        .types
        .iter()
        .map(|comparison| {
            let mut fields = Vec::new();
            push_text(&mut fields, "record_type", Some(&comparison.record_type));
            push_text(
                &mut fields,
                "agreement",
                Some(&wire_name(&comparison.agreement)),
            );
            Node {
                fields,
                children: comparison
                    .answers
                    .iter()
                    .map(|answer| answer_node(answer, &texts))
                    .collect(),
            }
        })
        .collect();
    let servers = summary
        .servers
        .iter()
        .map(|server| {
            let mut fields = Vec::new();
            push_text(&mut fields, "server", Some(&server.server));
            push_text(&mut fields, "name", server.name.as_deref());
            push_text(&mut fields, "transport", Some(&server.transport));
            push_text(&mut fields, "source", server.source.as_deref());
            fields.push(("lookups".to_owned(), Value::Int(i64::from(server.lookups))));
            fields.push((
                "measured".to_owned(),
                Value::Int(i64::from(server.measured)),
            ));
            fields.push((
                "unmeasured".to_owned(),
                Value::Int(i64::from(server.unmeasured)),
            ));
            if let Some(ms) = server.median_response_time_ms {
                fields.push(("median_response_time".to_owned(), millis(ms)));
            }
            Node {
                fields,
                children: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    [
        block(
            "answers",
            "Answers",
            Shape::Tree(Node {
                fields,
                children: types,
            }),
        ),
        block(
            "servers",
            "Servers",
            Shape::Tree(node(vec![("servers", count(servers.len()))], servers)),
        ),
    ]
}

/// The done facts every batch job shares.
fn done_facts(duration_ms: u64, total_queries: u32) -> Vec<(String, Value)> {
    vec![
        ("duration".to_owned(), Value::Duration(duration_ms)),
        (
            "total_queries".to_owned(),
            Value::Int(i64::from(total_queries)),
        ),
    ]
}

fn page(summary_blocks: [Block; 2], own: Vec<Block>) -> Vec<Block> {
    summary_blocks.into_iter().chain(own).collect()
}

/// The page blocks of a query job: `dns.answers` and `dns.servers`, then `query_blocks`. A job
/// with no record-type block has none.
pub fn query_page_blocks(result: &DnsQueryResult) -> Vec<Block> {
    let own = query_blocks(result);
    if own.is_empty() {
        return own;
    }
    let done = result.done.as_ref().map(|done| {
        let mut facts = done_facts(done.duration_ms, done.total_queries);
        let mut texts = Vec::new();
        push_text(&mut texts, "transport", Some(&done.transport));
        facts.extend(texts);
        for provider in &done.degraded_providers {
            push_text(&mut facts, "degraded_provider", Some(provider));
        }
        facts.push(("dnssec".to_owned(), Value::Bool(done.dnssec)));
        facts
    });
    page(
        summary_blocks("query", &result.batches, result.summary.as_ref(), done),
        own,
    )
}

/// The page blocks of a compare job: `dns.answers` and `dns.servers`, then `compare_blocks`.
pub fn compare_page_blocks(result: &DnsCompareResult) -> Vec<Block> {
    let done = result.done.as_ref().map(|done| {
        let mut facts = done_facts(done.duration_ms, done.total_queries);
        for transport in &done.transports {
            push_text(&mut facts, "transport", Some(transport));
        }
        facts
    });
    page(
        summary_blocks("compare", &result.batches, result.summary.as_ref(), done),
        compare_blocks(result),
    )
}

/// The page blocks of an authcompare job: `dns.answers` and `dns.servers`, then
/// `authcompare_blocks`.
pub fn authcompare_page_blocks(result: &DnsAuthCompareResult) -> Vec<Block> {
    let done = result.done.as_ref().map(|done| {
        let mut facts = done_facts(done.duration_ms, done.total_queries);
        for server in &done.auth_servers {
            push_text(&mut facts, "auth_server", Some(server));
        }
        facts
    });
    page(
        summary_blocks(
            "authcompare",
            &result.batches,
            result.summary.as_ref(),
            done,
        ),
        authcompare_blocks(result),
    )
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
                    let mut fields = Vec::new();
                    if let Some(name) = server.server_name.as_deref().filter(|n| !n.is_empty()) {
                        fields.push(("server_name", text(name)));
                    }
                    fields.push(("address", Value::Addr(server.server_ip)));
                    if !matches!(server.outcome, ServerOutcome::Error { .. }) {
                        fields.push(("latency", millis(server.latency_ms)));
                    }
                    fields.push(("outcome", text(outcome)));
                    if let ServerOutcome::Error { message } = &server.outcome
                        && !message.is_empty()
                    {
                        fields.push(("error", text(message)));
                    }
                    fields.extend(
                        server
                            .referral_ns
                            .iter()
                            .filter(|ns| !ns.is_empty())
                            .map(|ns| ("referral", text(ns))),
                    );
                    fields.extend(server.answer_records.iter().map(|record| {
                        (
                            "answer",
                            text(format!("{} {}", record.record_type, record.rdata)),
                        )
                    }));
                    node(fields, Vec::new())
                })
                .collect();
            let mut fields = vec![
                ("level", count(hop.level)),
                ("zone", text(&hop.zone)),
                ("servers_queried", count(hop.servers_queried)),
            ];
            if !hop.referral_groups.is_empty() {
                fields.push(("referral_groups", count(hop.referral_groups.len())));
            }
            node(fields, servers)
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
            for (key, records) in [
                ("dnskey", &level.dnskey_records),
                ("ds", &level.ds_records),
                ("rrsig", &level.rrsig_records),
            ] {
                fields.extend(
                    records
                        .iter()
                        .filter(|r| !r.rdata.is_empty())
                        .map(|r| (key, text(&r.rdata))),
                );
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
