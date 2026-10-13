use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use netray_common::enrichment::{EnrichmentClient, EnrichmentMode};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{
    Block, BlockId, Fields, Register, Row, Shape, ShapeKind, Value as EvValue, ValueType,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinHandle;

use crate::checks::run_all_checks;
use crate::checks::util::parse_tags;
use crate::config::{Config, ModuleConfig};
use crate::dns::{DnsResolver, FetchResolver};
use crate::error::MailError;
use crate::input;
use crate::quality::types::{
    Category, CheckResult as CategoryResult, Grade, Measured, SseEvent, Verdict,
};
use crate::state::OutboundFetch;

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    BUCKETED
        .iter()
        .map(|name| CheckId::parse(&format!("email.{name}")).expect("static check id"))
        .collect()
});

pub(crate) fn email_checks() -> &'static [CheckId] {
    CHECKS.as_slice()
}

/// The email register: the MX records and their addresses, the policy records at the domain's
/// names with their tags and what checking them got, the MTA-STS policy file, DNSKEY presence,
/// the DKIM key records, the TLSA records of the MX hosts, the DNSBL queries, the FCrDNS
/// answers and the V1 grade.
pub const REGISTER: Register = Register {
    blocks: &[
        ("email.mx", ShapeKind::Table),
        ("email.mx_addresses", ShapeKind::Table),
        ("email.spf", ShapeKind::Fields),
        ("email.dmarc", ShapeKind::Fields),
        ("email.mta_sts", ShapeKind::Fields),
        ("email.mta_sts_policy", ShapeKind::Text),
        ("email.tls_rpt", ShapeKind::Fields),
        ("email.bimi", ShapeKind::Fields),
        ("email.dnssec", ShapeKind::Fields),
        ("email.dkim", ShapeKind::Table),
        ("email.dane", ShapeKind::Table),
        ("email.dnsbl", ShapeKind::Table),
        ("email.fcrdns", ShapeKind::Table),
        ("email.summary", ShapeKind::Fields),
    ],
    keys: &[
        ("email.mx", "host", ValueType::Text),
        ("email.mx", "preference", ValueType::Int),
        ("email.mx", "cname", ValueType::Text),
        ("email.mx_addresses", "address", ValueType::Addr),
        ("email.mx_addresses", "host", ValueType::Text),
        ("email.spf", "record", ValueType::Text),
        ("email.spf", "authorized_prefixes", ValueType::Text),
        ("email.spf", "lookup_count", ValueType::Int),
        ("email.spf", "void_lookups", ValueType::Int),
        ("email.dmarc", "record", ValueType::Text),
        ("email.dmarc", "p", ValueType::Text),
        ("email.dmarc", "sp", ValueType::Text),
        ("email.dmarc", "pct", ValueType::Text),
        ("email.dmarc", "rua", ValueType::Text),
        ("email.dmarc", "ruf", ValueType::Text),
        ("email.dmarc", "adkim", ValueType::Text),
        ("email.dmarc", "aspf", ValueType::Text),
        ("email.dmarc", "fo", ValueType::Text),
        ("email.dmarc", "ri", ValueType::Text),
        ("email.dmarc", "rua_authorized", ValueType::Bool),
        ("email.dmarc", "ruf_authorized", ValueType::Bool),
        ("email.mta_sts", "record", ValueType::Text),
        ("email.mta_sts", "id", ValueType::Text),
        ("email.mta_sts", "policy_status", ValueType::Int),
        ("email.mta_sts", "content_type", ValueType::Text),
        ("email.mta_sts", "fetch_error", ValueType::Text),
        ("email.tls_rpt", "record", ValueType::Text),
        ("email.tls_rpt", "rua", ValueType::Text),
        ("email.bimi", "record", ValueType::Text),
        ("email.bimi", "l", ValueType::Text),
        ("email.bimi", "a", ValueType::Text),
        ("email.bimi", "logo_fetch", ValueType::Text),
        ("email.dnssec", "dnskey_present", ValueType::Bool),
        ("email.dkim", "key", ValueType::Text),
        ("email.dkim", "selector", ValueType::Text),
        ("email.dkim", "state", ValueType::Text),
        ("email.dkim", "key_type", ValueType::Text),
        ("email.dkim", "bits", ValueType::Text),
        ("email.dane", "record", ValueType::Text),
        ("email.dane", "host", ValueType::Text),
        ("email.dane", "usage", ValueType::Int),
        ("email.dane", "selector", ValueType::Int),
        ("email.dane", "matching_type", ValueType::Int),
        ("email.dnsbl", "query", ValueType::Text),
        ("email.dnsbl", "zone", ValueType::Text),
        ("email.dnsbl", "target", ValueType::Text),
        ("email.dnsbl", "answered", ValueType::Bool),
        ("email.dnsbl", "answer", ValueType::Text),
        ("email.fcrdns", "address", ValueType::Addr),
        ("email.fcrdns", "ptr", ValueType::Text),
        ("email.fcrdns", "confirmed", ValueType::Bool),
        ("email.summary", "grade", ValueType::Text),
    ],
};

/// The email inspection as an engine module: beacon's checks run in-process.
pub struct EmailModule {
    config: Arc<Config>,
    dns_resolver: Arc<DnsResolver>,
    dnsbl_resolver: Arc<DnsResolver>,
    fetch: OutboundFetch,
    enrichment_client: Option<Arc<EnrichmentClient>>,
    inspect_semaphore: Arc<Semaphore>,
}

impl EmailModule {
    pub async fn new(module: ModuleConfig) -> Result<Self, MailError> {
        let max_concurrent = module.inspections.max_concurrent;
        let config = Config::from_module(module);

        let dns_resolver = DnsResolver::new(&config.dns.resolvers, config.dns.timeout_ms)
            .await
            .map_err(|e| MailError::DnsError(e.to_string()))?;
        let dns_resolver = Arc::new(dns_resolver);

        let dnsbl_resolver = DnsResolver::new(&config.dnsbl.resolvers, config.dnsbl.timeout_ms)
            .await
            .map_err(|e| MailError::DnsError(e.to_string()))?;

        let enrichment_client = if config.backends.ip_url.is_empty() {
            None
        } else {
            Some(Arc::new(EnrichmentClient::new(
                &config.backends.ip_url,
                Duration::from_millis(config.backends.timeout_ms),
                "beacon",
                None,
                EnrichmentMode::Backend { cache_ttl_secs: 0 },
            )))
        };

        let fetch = OutboundFetch::new(
            config.http.timeout_ms,
            Arc::new(FetchResolver(dns_resolver.clone())),
        );

        Ok(Self {
            config: Arc::new(config),
            dns_resolver,
            dnsbl_resolver: Arc::new(dnsbl_resolver),
            fetch,
            enrichment_client,
            inspect_semaphore: Arc::new(Semaphore::new(max_concurrent)),
        })
    }
}

/// Aborts the inspection task when the run future is dropped.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Module for EmailModule {
    fn protocol(&self) -> Protocol {
        Protocol::Email
    }

    fn checks(&self) -> &'static [CheckId] {
        email_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            let selectors = ctx.options.dkim_selectors.clone().unwrap_or_default();
            let domain = match input::parse_domain(ctx.domain.as_str()).and_then(|domain| {
                input::validate_selectors(&selectors, self.config.dkim.max_user_selectors)?;
                Ok(domain)
            }) {
                Ok(domain) => domain,
                Err(e) => {
                    return SectionOutcome::Incomplete {
                        reason: e.to_string(),
                    };
                }
            };

            let Ok(_permit) = self.inspect_semaphore.clone().try_acquire_owned() else {
                return SectionOutcome::Incomplete {
                    reason: "busy".to_string(),
                };
            };

            let (tx, mut rx) = mpsc::channel::<SseEvent>(32);
            let _task = AbortOnDrop(tokio::spawn(run_all_checks(
                domain,
                selectors,
                self.config.clone(),
                self.dns_resolver.clone(),
                self.dnsbl_resolver.clone(),
                self.fetch.clone(),
                self.enrichment_client.clone(),
                tx,
            )));

            let collect = async {
                let mut events = Vec::new();
                while let Some(event) = rx.recv().await {
                    let done = matches!(event, SseEvent::Summary { .. });
                    events.push(event);
                    if done {
                        break;
                    }
                }
                events
            };
            match tokio::time::timeout_at(ctx.deadline.into(), collect).await {
                Ok(events) => translate(&events),
                Err(_) => SectionOutcome::TimedOut,
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Beacon vocabulary (pinned against beacon's own constants by the tests below)
// ---------------------------------------------------------------------------

/// Beacon's wire value for a check that did not run: the summary grade after its own timeout
/// and the sub-check name of a category that did not complete.
pub const BEACON_SKIPPED: &str = "skipped";
/// Beacon's sub-check of a category whose lookup got no answer; its detail names the cause.
pub const BEACON_UNMEASURED: &str = "unmeasured";
/// Beacon's Info sub-check of a nested lookup that got no answer; its detail names the cause.
pub const BEACON_LOOKUP_FAILED: &str = "lookup_failed";
/// Beacon's `mx` sub-check for a Null MX (RFC 7505) domain.
pub const BEACON_NULL_MX: &str = "null_mx";
/// Beacon's cross-validation sub-check for a domain that declares it sends no mail.
pub const BEACON_SENDS_NO_MAIL: &str = "sends_no_mail";

const NOT_APPLICABLE: &str = "not applicable";

/// Scored beacon categories in catalogue order; each is scored as the `email.<category>` check.
pub const BUCKETED: &[&str] = &[
    "mx", "spf", "dkim", "dmarc", "mta_sts", "tls_rpt", "bimi", "dane", "dnsbl", "fcrdns",
];

/// Checks that do not apply to a domain without MX records or with a Null MX.
const MX_DEPENDENT: &[&str] = &[
    "mx", "mta_sts", "tls_rpt", "bimi", "dane", "dnsbl", "fcrdns",
];

/// Beacon categories the email section does not score as email categories, with the reason.
pub const EXCLUDED: &[(&str, &str)] = &[
    ("dnssec", "scored in DNS section"),
    ("cross_validation", "routed by sub-check"),
];

/// Cross-validation sub-checks beacon emits that are ignored when the domain sends no mail.
const IGNORED_WHEN_SENDS_NO_MAIL: &[&str] = &["reject_no_dkim", "spf_mx_coverage"];

/// The blocks a cross-validation sub-check judged from, which the check it is routed to
/// references besides its own; a block of another module is kept even when this section does
/// not hold it.
const CROSS_VALIDATION_INPUTS: &[(&str, &[&str])] = &[
    ("reject_no_dkim", &["email.dmarc", "email.spf"]),
    ("spf_mx_coverage", &["email.mx_addresses"]),
    ("bimi_dmarc_policy", &["email.dmarc"]),
    ("mta_sts_without_tls_rpt", &["email.tls_rpt"]),
    ("dane_without_tls_rpt", &["email.dane"]),
    ("dane_without_dnssec", &["email.dnssec", "dns.dnskey"]),
    ("null_mx_spf", &["email.mx"]),
    (BEACON_SENDS_NO_MAIL, &["email.mx"]),
];

/// The category a beacon cross-validation sub-check is scored in; `None` for an unknown name.
pub fn route_cross_validation(name: &str) -> Option<&'static str> {
    match name {
        "spf_mx_coverage" | "null_mx_spf" | BEACON_SENDS_NO_MAIL => Some("spf"),
        "reject_no_dkim" => Some("dkim"),
        "dmarc_rua_auth" | "dmarc_sp_gap" => Some("dmarc"),
        "mta_sts_id_mismatch" | "mta_sts_mx_coverage" => Some("mta_sts"),
        "mta_sts_without_tls_rpt" | "dane_without_tls_rpt" => Some("tls_rpt"),
        "bimi_dmarc_policy" => Some("bimi"),
        "dane_without_dnssec" => Some("dane"),
        "fcrdns_mismatch" => Some("fcrdns"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Beacon summary types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SubCheck {
    pub name: String,
    pub verdict: Status,
    pub detail: String,
}

#[derive(Debug)]
pub struct CategoryVerdict {
    pub name: String,
    pub verdict: Status,
    pub message: Option<String>,
    pub sub_checks: Vec<SubCheck>,
}

#[derive(Debug)]
pub struct BeaconSummary {
    pub grade: Option<String>,
    pub categories: Vec<CategoryVerdict>,
}

/// Why a stream has no summary to score.
enum Abort {
    TimedOut,
    Incomplete(String),
}

// ---------------------------------------------------------------------------
// Translation
// ---------------------------------------------------------------------------

/// Maps beacon's events onto the ten email checks and beacon's blocks. A `skipped`
/// summary whose every verdict is Skip (beacon's own timeout) is a
/// [`SectionOutcome::TimedOut`]; a stream that cannot be scored is `Incomplete`.
///
/// Precedence per check: no MX or a Null MX makes the MX-dependent checks not applicable; else
/// an unmeasured `mx` makes them unmeasured with its cause; else [`fold_category`].
pub fn translate(events: &[SseEvent]) -> SectionOutcome {
    let summary = match parse_summary(events) {
        Ok(summary) => summary,
        Err(Abort::TimedOut) => return SectionOutcome::TimedOut,
        Err(Abort::Incomplete(reason)) => return SectionOutcome::Incomplete { reason },
    };

    let evidence: Vec<Block> = events
        .iter()
        .find_map(|e| match e {
            SseEvent::Evidence { blocks } => Some(blocks.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let no_mx_reason = no_mx_reason(&summary);
    let mx_unmeasured = unmeasured_findings(&summary, "mx");
    let checks: Vec<CheckResult> = BUCKETED
        .iter()
        .zip(email_checks())
        .map(|(&name, id)| {
            let mx_dependent = MX_DEPENDENT.contains(&name);
            let (status, findings) = match (no_mx_reason, &mx_unmeasured) {
                (Some(reason), _) if mx_dependent => {
                    (Status::NotApplicable, vec![reason.to_string()])
                }
                (_, Some(cause)) if mx_dependent => (Status::Unmeasured, cause.clone()),
                _ => fold_category(name, &summary),
            };
            let mut refs = Vec::new();
            if matches!(status, Status::Pass | Status::Warn | Status::Fail) {
                refs = references(name, &summary);
                refs.retain(|id| id.module() != "email" || evidence.iter().any(|b| b.id == *id));
            }
            CheckResult {
                id: id.clone(),
                status,
                findings,
                evidence: refs,
            }
        })
        .collect();

    SectionOutcome::Measured { checks, evidence }
}

/// The blocks a judged check references: its own, the MTA-STS policy file for `mta_sts`, the
/// MX addresses for `mx`, the MX records for the MX-dependent checks and the inputs of the
/// cross-validation sub-checks routed to it. Absent email blocks are dropped by the caller.
fn references(name: &str, summary: &BeaconSummary) -> Vec<BlockId> {
    let mut refs = vec![format!("email.{name}")];
    match name {
        "mx" => refs.push("email.mx_addresses".into()),
        "mta_sts" => refs.push("email.mta_sts_policy".into()),
        _ => {}
    }
    if name != "mx" && MX_DEPENDENT.contains(&name) {
        refs.push("email.mx".into());
    }
    for sub in routed(name, summary) {
        let inputs = CROSS_VALIDATION_INPUTS
            .iter()
            .filter(|(n, _)| *n == sub.name)
            .flat_map(|(_, blocks)| blocks.iter());
        for input in inputs {
            if !refs.iter().any(|r| r == input) {
                refs.push(input.to_string());
            }
        }
    }
    refs.iter().map(|r| block_id(r)).collect()
}

/// The cross-validation sub-checks routed to check `name`, without those a domain that sends
/// no mail ignores.
fn routed<'a>(name: &str, summary: &'a BeaconSummary) -> Vec<&'a SubCheck> {
    let sends_no_mail = summary.categories.iter().any(|c| {
        c.name == "cross_validation" && c.sub_checks.iter().any(|s| s.name == BEACON_SENDS_NO_MAIL)
    });
    summary
        .categories
        .iter()
        .filter(|c| c.name == "cross_validation")
        .flat_map(|c| &c.sub_checks)
        .filter(|s| {
            route_cross_validation(&s.name) == Some(name)
                && !(sends_no_mail && IGNORED_WHEN_SENDS_NO_MAIL.contains(&s.name.as_str()))
        })
        .collect()
}

fn block_id(id: &str) -> BlockId {
    BlockId::parse(id).expect("static block id")
}

fn block(id: &str, title: &str, shape: Shape) -> Block {
    Block {
        id: block_id(id),
        title: title.into(),
        shape,
    }
}

fn table(id: &str, title: &str, columns: &[&str], rows: Vec<Row>) -> Block {
    block(
        id,
        title,
        Shape::Table {
            key: columns[0].into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
        },
    )
}

/// A `record` per policy record and, for a single record, its tags among `tags`.
fn record_fields(records: &[String], tags: &[&str]) -> Fields {
    let mut fields: Fields = records
        .iter()
        .map(|r| ("record".into(), EvValue::Text(r.clone())))
        .collect();
    if let [record] = records {
        let parsed = parse_tags(record);
        fields.extend(tags.iter().filter_map(|t| {
            parsed
                .get(*t)
                .map(|v| (t.to_string(), EvValue::Text(v.clone())))
        }));
    }
    fields
}

/// `fields` with `(key, value)` appended when there is a value.
fn with(mut fields: Fields, extra: &[(&str, Option<EvValue>)]) -> Fields {
    fields.extend(
        extra
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.clone()?))),
    );
    fields
}

/// Answer codes in canonical order, joined with `, `.
fn codes(answer: &[std::net::Ipv4Addr]) -> String {
    let mut codes: Vec<String> = answer.iter().map(|ip| ip.to_string()).collect();
    codes.sort();
    codes.dedup();
    codes.join(", ")
}

/// The blocks of what beacon measured, beacon's `evidence` message. A check whose lookup got
/// no answer gives no block; a lookup that answered with nothing gives an empty one.
pub fn evidence_blocks(m: &Measured, grade: Grade) -> Vec<Block> {
    let mut blocks = Vec::new();
    if let Some(hosts) = &m.mx {
        blocks.push(mx_block(hosts));
        blocks.push(mx_addresses_block(hosts));
    }
    if let Some(spf) = &m.spf {
        let mut fields = record_fields(&spf.records, &[]);
        fields.extend(
            spf.authorized_prefixes
                .iter()
                .map(|p| ("authorized_prefixes".into(), EvValue::Text(p.to_string()))),
        );
        let fields = with(
            fields,
            &[
                (
                    "lookup_count",
                    spf.lookup_count.map(|n| EvValue::Int(i64::from(n))),
                ),
                (
                    "void_lookups",
                    spf.void_lookups.map(|n| EvValue::Int(i64::from(n))),
                ),
            ],
        );
        blocks.push(block("email.spf", "SPF", Shape::Fields(fields)));
    }
    if let Some(dmarc) = &m.dmarc {
        let tags = ["p", "sp", "pct", "rua", "ruf", "adkim", "aspf", "fo", "ri"];
        let fields = with(
            record_fields(&dmarc.records, &tags),
            &[
                ("rua_authorized", dmarc.rua_authorized.map(EvValue::Bool)),
                ("ruf_authorized", dmarc.ruf_authorized.map(EvValue::Bool)),
            ],
        );
        blocks.push(block("email.dmarc", "DMARC", Shape::Fields(fields)));
    }
    if let Some(sts) = &m.mta_sts {
        let fields = with(
            record_fields(&sts.records, &["id"]),
            &[
                (
                    "policy_status",
                    sts.policy_status.map(|s| EvValue::Int(i64::from(s))),
                ),
                ("content_type", sts.content_type.clone().map(EvValue::Text)),
                ("fetch_error", sts.fetch_error.clone().map(EvValue::Text)),
            ],
        );
        blocks.push(block("email.mta_sts", "MTA-STS", Shape::Fields(fields)));
        if let Some(policy) = &sts.policy {
            blocks.push(block(
                "email.mta_sts_policy",
                "MTA-STS policy",
                Shape::Text(policy.clone()),
            ));
        }
    }
    if let Some(records) = &m.tls_rpt {
        blocks.push(block(
            "email.tls_rpt",
            "TLS-RPT",
            Shape::Fields(record_fields(records, &["rua"])),
        ));
    }
    if let Some(bimi) = &m.bimi {
        let fields = with(
            record_fields(&bimi.records, &["l", "a"]),
            &[("logo_fetch", bimi.logo_fetch.clone().map(EvValue::Text))],
        );
        blocks.push(block("email.bimi", "BIMI", Shape::Fields(fields)));
    }
    if let Some(present) = m.dnssec {
        blocks.push(block(
            "email.dnssec",
            "DNSSEC",
            Shape::Fields(vec![("dnskey_present".into(), EvValue::Bool(present))]),
        ));
    }
    if let Some(keys) = &m.dkim {
        blocks.push(dkim_block(keys));
    }
    if let Some(records) = &m.dane {
        blocks.push(dane_block(records));
    }
    if let Some(answers) = &m.dnsbl {
        let rows = answers
            .iter()
            .map(|a| {
                vec![
                    EvValue::Text(format!("{}@{}", a.target, a.zone)),
                    EvValue::Text(a.zone.clone()),
                    EvValue::Text(a.target.clone()),
                    EvValue::Bool(a.answered),
                    EvValue::Text(codes(&a.answer)),
                ]
            })
            .collect();
        blocks.push(table(
            "email.dnsbl",
            "DNSBL",
            &["query", "zone", "target", "answered", "answer"],
            rows,
        ));
    }
    if let Some(answers) = &m.fcrdns {
        let rows = answers
            .iter()
            .map(|a| {
                vec![
                    EvValue::Addr(a.address),
                    EvValue::Text(a.ptr.join(", ")),
                    EvValue::Bool(a.confirmed),
                ]
            })
            .collect();
        blocks.push(table(
            "email.fcrdns",
            "FCrDNS",
            &["address", "ptr", "confirmed"],
            rows,
        ));
    }
    blocks.push(block(
        "email.summary",
        "Summary",
        Shape::Fields(vec![("grade".into(), EvValue::Text(grade_wire(&grade)))]),
    ));
    blocks
}

/// One row per MX host, the first record of a repeated host; the `cname` column only when every
/// host's CNAME lookup got an answer.
fn mx_block(hosts: &[crate::quality::MxHost]) -> Block {
    let with_cname = hosts.iter().all(|h| h.cname.is_some());
    let mut seen = std::collections::HashSet::new();
    let rows = hosts
        .iter()
        .filter(|h| seen.insert(h.host.as_str()))
        .map(|h| {
            let mut row = vec![
                EvValue::Text(h.host.clone()),
                EvValue::Int(i64::from(h.preference)),
            ];
            if with_cname {
                row.push(EvValue::Text(h.cname.clone().unwrap_or_default()));
            }
            row
        })
        .collect();
    let columns: &[&str] = if with_cname {
        &["host", "preference", "cname"]
    } else {
        &["host", "preference"]
    };
    table("email.mx", "MX", columns, rows)
}

/// One row per MX address with the hosts that resolve to it, joined with `, `.
fn mx_addresses_block(hosts: &[crate::quality::MxHost]) -> Block {
    let mut addresses: Vec<(std::net::IpAddr, Vec<&str>)> = Vec::new();
    for h in hosts {
        for ip in &h.addresses {
            match addresses.iter_mut().find(|(a, _)| a == ip) {
                Some((_, names)) if !names.contains(&h.host.as_str()) => names.push(&h.host),
                Some(_) => {}
                None => addresses.push((*ip, vec![&h.host])),
            }
        }
    }
    table(
        "email.mx_addresses",
        "MX addresses",
        &["address", "host"],
        addresses
            .into_iter()
            .map(|(ip, names)| vec![EvValue::Addr(ip), EvValue::Text(names.join(", "))])
            .collect(),
    )
}

/// One row per key record, keyed `<selector>#<n>` with `n` counting the selector's records from
/// 1; `bits` is empty when the size was not read.
fn dkim_block(keys: &[crate::quality::DkimKey]) -> Block {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let rows = keys
        .iter()
        .map(|k| {
            let n = counts.entry(k.selector.as_str()).or_insert(0);
            *n += 1;
            vec![
                EvValue::Text(format!("{}#{n}", k.selector)),
                EvValue::Text(k.selector.clone()),
                EvValue::Text(k.state.clone()),
                EvValue::Text(k.key_type.clone()),
                EvValue::Text(k.bits.map(|b| b.to_string()).unwrap_or_default()),
            ]
        })
        .collect();
    table(
        "email.dkim",
        "DKIM keys",
        &["key", "selector", "state", "key_type", "bits"],
        rows,
    )
}

/// One row per TLSA record, keyed `_25._tcp.<host> <usage> <selector> <matching type>` with the
/// suffix `#<n>` on a repeat.
fn dane_block(records: &[crate::quality::DaneRecord]) -> Block {
    let mut keys = std::collections::HashSet::new();
    let rows = records
        .iter()
        .map(|r| {
            let base = format!(
                "_25._tcp.{} {} {} {}",
                r.host, r.usage, r.selector, r.matching_type
            );
            let mut key = base.clone();
            let mut n = 1;
            while !keys.insert(key.clone()) {
                n += 1;
                key = format!("{base}#{n}");
            }
            vec![
                EvValue::Text(key),
                EvValue::Text(r.host.clone()),
                EvValue::Int(i64::from(r.usage)),
                EvValue::Int(i64::from(r.selector)),
                EvValue::Int(i64::from(r.matching_type)),
            ]
        })
        .collect();
    table(
        "email.dane",
        "DANE TLSA",
        &["record", "host", "usage", "selector", "matching_type"],
        rows,
    )
}

fn category_name(category: &Category) -> String {
    serde_json::to_value(category)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("category serialises as a string")
}

fn grade_wire(grade: &crate::quality::types::Grade) -> String {
    serde_json::to_value(grade)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("grade serialises as a string")
}

/// Extract the summary from beacon's events.
///
/// Verdicts come from the summary's `verdicts` map (category -> verdict); messages and
/// sub-checks come from the category events.
fn parse_summary(events: &[SseEvent]) -> Result<BeaconSummary, Abort> {
    let (grade, verdicts) = events
        .iter()
        .find_map(|e| match e {
            SseEvent::Summary {
                grade, verdicts, ..
            } => Some((grade, verdicts)),
            _ => None,
        })
        .ok_or_else(|| Abort::Incomplete("no summary event from beacon".to_string()))?;

    let grade = Some(grade_wire(grade));

    // Per-category events carry the detail and sub-checks; the summary carries the verdicts.
    // Beacon's own timeout skips every category; a `skipped` grade over measured categories
    // only means some category is unmeasured.
    if grade.as_deref() == Some(BEACON_SKIPPED) && verdicts.values().all(|v| *v == Verdict::Skip) {
        return Err(Abort::TimedOut);
    }

    let category_events: HashMap<String, &CategoryResult> = events
        .iter()
        .filter_map(|e| match e {
            SseEvent::Category(c) => Some((category_name(&c.category), c)),
            _ => None,
        })
        .collect();

    let mut categories: Vec<CategoryVerdict> = Vec::new();
    let verdicts: BTreeMap<&String, &Verdict> = verdicts.iter().collect();
    for (name, verdict) in verdicts {
        let known = BUCKETED.contains(&name.as_str()) || EXCLUDED.iter().any(|(c, _)| c == name);
        if !known {
            return Err(Abort::Incomplete(format!("unknown verdict `{name}`")));
        }
        let event = category_events.get(name.as_str()).ok_or_else(|| {
            Abort::Incomplete(format!("beacon sent no category event for `{name}`"))
        })?;
        let sub_checks = parse_sub_checks(event);
        if sub_checks.iter().any(|s| s.name == BEACON_SKIPPED) {
            return Err(Abort::Incomplete(format!(
                "beacon category `{name}` did not complete"
            )));
        }
        if name == "cross_validation"
            && let Some(unrouted) = sub_checks
                .iter()
                .find(|s| route_cross_validation(&s.name).is_none())
        {
            return Err(Abort::Incomplete(format!(
                "unknown verdict `{}`",
                unrouted.name
            )));
        }
        categories.push(CategoryVerdict {
            name: name.clone(),
            verdict: beacon_verdict(verdict),
            message: Some(event.detail.clone()),
            sub_checks,
        });
    }

    Ok(BeaconSummary { grade, categories })
}

fn parse_sub_checks(category_event: &CategoryResult) -> Vec<SubCheck> {
    category_event
        .sub_checks
        .iter()
        .map(|s| SubCheck {
            name: s.name.clone(),
            verdict: beacon_verdict(&s.verdict),
            detail: s.detail.clone(),
        })
        .collect()
}

/// Why the MX-dependent checks do not apply: beacon's `mx` category carries the `no_mx` or the
/// `null_mx` sub-check. Other `mx` failures (`mx_cname`, `mx_no_addr`, ...) mean MX records exist.
fn no_mx_reason(summary: &BeaconSummary) -> Option<&'static str> {
    let mx = summary.categories.iter().find(|c| c.name == "mx")?;
    if mx.sub_checks.iter().any(|s| s.name == BEACON_NULL_MX) {
        Some("Null MX")
    } else if mx.sub_checks.iter().any(|s| s.name == "no_mx") {
        Some("no MX")
    } else {
        None
    }
}

/// Whether the domain accepts no mail: no MX records or a Null MX.
pub fn detect_no_mx(summary: &BeaconSummary) -> bool {
    no_mx_reason(summary).is_some()
}

// ---------------------------------------------------------------------------
// Category fold
// ---------------------------------------------------------------------------

/// The details of the `unmeasured` sub-check of category `name`, when it carries one.
fn unmeasured_findings(summary: &BeaconSummary, name: &str) -> Option<Vec<String>> {
    let category = summary.categories.iter().find(|c| c.name == name)?;
    let details: Vec<String> = category
        .sub_checks
        .iter()
        .filter(|s| s.name == BEACON_UNMEASURED)
        .map(|s| s.detail.clone())
        .collect();
    (!details.is_empty()).then_some(details)
}

/// A category carrying the `unmeasured` sub-check is unmeasured with its cause, whatever the
/// cross-validation routes to it. Otherwise a check's status is the worst Warn or Fail over its
/// category and the cross-validation sub-checks routed to it, else the category's Pass; `mx`
/// with records and no Warn or Fail is a Pass. On a Pass, the `lookup_failed` details stay as
/// findings, and `mx` and `dnsbl` keep all their Info details. A category that is only Info or Skip is not applicable with its detail
/// as the finding.
fn fold_category(name: &str, summary: &BeaconSummary) -> (Status, Vec<String>) {
    if let Some(cause) = unmeasured_findings(summary, name) {
        return (Status::Unmeasured, cause);
    }
    let category = summary.categories.iter().find(|c| c.name == name);
    let routed = routed(name, summary);

    let worst = category
        .map(|c| c.verdict)
        .into_iter()
        .chain(routed.iter().map(|s| s.verdict))
        .filter(|v| matches!(v, Status::Warn | Status::Fail))
        .max_by_key(verdict_rank);

    let category_pass = category.is_some_and(|c| c.verdict == Status::Pass || c.name == "mx");
    let Some(status) = worst.or(category_pass.then_some(Status::Pass)) else {
        let detail = category
            .and_then(|c| c.message.clone())
            .unwrap_or_else(|| NOT_APPLICABLE.to_string());
        return (Status::NotApplicable, vec![detail]);
    };

    let mut findings: Vec<String> = category
        .filter(|c| matches!(c.verdict, Status::Warn | Status::Fail))
        .map(category_messages)
        .unwrap_or_default();
    findings.extend(
        routed
            .iter()
            .filter(|s| {
                matches!(s.verdict, Status::Warn | Status::Fail) || s.name == BEACON_SENDS_NO_MAIL
            })
            .map(|s| s.detail.clone()),
    );
    if status == Status::Pass {
        let keeps_info = matches!(name, "mx" | "dnsbl");
        findings.extend(
            category
                .into_iter()
                .flat_map(|c| &c.sub_checks)
                .filter(|s| {
                    s.name == BEACON_LOOKUP_FAILED
                        || (keeps_info && s.verdict == Status::NotApplicable)
                })
                .map(|s| s.detail.clone()),
        );
    }
    (status, findings)
}

/// Reasons for a category: the details of its warn/fail sub-checks, else the category detail.
fn category_messages(cat: &CategoryVerdict) -> Vec<String> {
    let reasons: Vec<String> = cat
        .sub_checks
        .iter()
        .filter(|s| matches!(s.verdict, Status::Warn | Status::Fail))
        .map(|s| s.detail.clone())
        .collect();
    if reasons.is_empty() {
        cat.message.iter().cloned().collect()
    } else {
        reasons
    }
}

/// Beacon's `Verdict` as a status; `Info` is neutral and maps explicitly to not applicable.
fn beacon_verdict(v: &Verdict) -> Status {
    match v {
        Verdict::Pass => Status::Pass,
        Verdict::Warn => Status::Warn,
        Verdict::Fail => Status::Fail,
        Verdict::Info | Verdict::Skip => Status::NotApplicable,
    }
}

fn verdict_rank(v: &Status) -> u8 {
    match v {
        Status::Fail => 3,
        Status::Warn => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The routing tables cover everything beacon can emit, and the sentinels are beacon's.
    #[test]
    fn routing_covers_beacon_vocabulary() {
        let mut problems = Vec::new();
        for c in Category::ALL {
            let name = category_name(&c);
            let known =
                BUCKETED.contains(&name.as_str()) || EXCLUDED.iter().any(|(cat, _)| *cat == name);
            if !known {
                problems.push(format!(
                    "category `{name}` is neither BUCKETED nor EXCLUDED"
                ));
            }
        }
        for name in crate::checks::cross_validation::CROSS_VALIDATION_CHECKS {
            match route_cross_validation(name) {
                Some(category) if BUCKETED.contains(&category) => {}
                _ => problems.push(format!(
                    "cross-validation check `{name}` routes to no scored category"
                )),
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
        assert_eq!(BEACON_SKIPPED, crate::checks::SKIPPED);
        assert_eq!(BEACON_UNMEASURED, crate::checks::UNMEASURED);
        assert_eq!(BEACON_LOOKUP_FAILED, crate::checks::LOOKUP_FAILED);
        assert_eq!(BEACON_NULL_MX, crate::checks::mx::NULL_MX);
        assert_eq!(
            BEACON_SENDS_NO_MAIL,
            crate::checks::cross_validation::SENDS_NO_MAIL
        );
    }
}
