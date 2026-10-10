use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use netray_common::enrichment::{EnrichmentClient, EnrichmentMode};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinHandle;

use crate::checks::run_all_checks;
use crate::config::{Config, ModuleConfig};
use crate::dns::{DnsResolver, FetchResolver};
use crate::error::MailError;
use crate::input;
use crate::quality::types::{Category, CheckResult as CategoryResult, SseEvent, Verdict};
use crate::state::OutboundFetch;

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

const AUTH: &str = "email_authentication";
const INFRA: &str = "email_infrastructure";
const TRANSPORT: &str = "email_transport";
const BRAND: &str = "email_brand_policy";
const BUCKET_NAMES: [&str; 4] = [AUTH, INFRA, TRANSPORT, BRAND];

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    BUCKET_NAMES
        .iter()
        .map(|name| CheckId::parse(&format!("email.{name}")).expect("static check id"))
        .collect()
});

pub(crate) fn email_checks() -> &'static [CheckId] {
    CHECKS.as_slice()
}

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
/// Beacon's `mx` sub-check for a Null MX (RFC 7505) domain.
pub const BEACON_NULL_MX: &str = "null_mx";
/// Beacon's cross-validation sub-check for a domain that declares it sends no mail.
pub const BEACON_SENDS_NO_MAIL: &str = "sends_no_mail";

const NOT_APPLICABLE: &str = "not applicable";

/// Scored beacon categories and the bucket each feeds.
pub const BUCKETED: &[(&str, &str)] = &[
    ("spf", AUTH),
    ("dkim", AUTH),
    ("dmarc", AUTH),
    ("mx", INFRA),
    ("fcrdns", INFRA),
    ("dnsbl", INFRA),
    ("mta_sts", TRANSPORT),
    ("tls_rpt", TRANSPORT),
    ("dane", TRANSPORT),
    ("bimi", BRAND),
];

/// Beacon categories the email section does not score as email categories, with the reason.
pub const EXCLUDED: &[(&str, &str)] = &[
    ("dnssec", "scored in DNS section"),
    ("cross_validation", "routed by sub-check"),
];

/// Cross-validation sub-checks beacon emits that are ignored when the domain sends no mail.
const IGNORED_WHEN_SENDS_NO_MAIL: &[&str] = &["reject_no_dkim", "spf_mx_coverage"];

/// The bucket a beacon cross-validation sub-check is scored in; `None` for an unknown name.
pub fn route_cross_validation(name: &str) -> Option<&'static str> {
    match name {
        "null_mx_spf" | "reject_no_dkim" | "dmarc_rua_auth" | "dmarc_sp_gap"
        | "spf_mx_coverage" | BEACON_SENDS_NO_MAIL => Some(AUTH),
        "fcrdns_mismatch" => Some(INFRA),
        "bimi_dmarc_policy" => Some(BRAND),
        n if n.starts_with("mta_sts_") || n.starts_with("dane_") => Some(TRANSPORT),
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

/// Maps beacon's events onto the four email buckets and the V1 headline and extras. A `skipped`
/// summary is a [`SectionOutcome::TimedOut`]; a stream that cannot be scored is `Incomplete`.
pub fn translate(events: &[SseEvent]) -> SectionOutcome {
    let summary = match parse_summary(events) {
        Ok(summary) => summary,
        Err(Abort::TimedOut) => return SectionOutcome::TimedOut,
        Err(Abort::Incomplete(reason)) => return SectionOutcome::Incomplete { reason },
    };

    let no_mx_reason = no_mx_reason(&summary);
    let buckets = map_buckets(&summary, no_mx_reason);

    let bucket_na: HashMap<String, String> = buckets
        .iter()
        .filter(|b| b.status == Status::NotApplicable)
        .map(|b| {
            (
                b.name.clone(),
                no_mx_reason.unwrap_or(NOT_APPLICABLE).to_string(),
            )
        })
        .collect();

    let headline = build_headline(&buckets, &bucket_na);

    let checks = buckets
        .into_iter()
        .zip(email_checks())
        .map(|(bucket, id)| CheckResult {
            id: id.clone(),
            status: bucket.status,
            findings: bucket.messages,
            evidence: vec![],
        })
        .collect();

    SectionOutcome::Measured {
        checks,
        presentation: json!({
            "headline": headline,
            "grade": summary.grade,
            "bucket_na": bucket_na,
        }),
    }
}

/// One scored bucket before it becomes a [`CheckResult`].
struct Bucket {
    name: String,
    status: Status,
    messages: Vec<String>,
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
    if grade.as_deref() == Some(BEACON_SKIPPED) {
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
        let known =
            BUCKETED.iter().any(|(c, _)| c == name) || EXCLUDED.iter().any(|(c, _)| c == name);
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

const NULL_MX_REASON: &str = "null MX";

/// Why the receiving buckets do not apply: beacon's `mx` category carries the `no_mx` or the
/// `null_mx` sub-check. Other `mx` failures (`mx_cname`, `mx_no_addr`, ...) mean MX records exist.
fn no_mx_reason(summary: &BeaconSummary) -> Option<&'static str> {
    let mx = summary.categories.iter().find(|c| c.name == "mx")?;
    if mx.sub_checks.iter().any(|s| s.name == BEACON_NULL_MX) {
        Some(NULL_MX_REASON)
    } else if mx.sub_checks.iter().any(|s| s.name == "no_mx") {
        Some("no MX records")
    } else {
        None
    }
}

/// Whether the domain accepts no mail: no MX records or a Null MX.
pub fn detect_no_mx(summary: &BeaconSummary) -> bool {
    no_mx_reason(summary).is_some()
}

// ---------------------------------------------------------------------------
// Bucket aggregation
// ---------------------------------------------------------------------------

/// Map beacon's per-category verdicts into four scored buckets.
fn map_buckets(summary: &BeaconSummary, no_mx: Option<&str>) -> Vec<Bucket> {
    BUCKET_NAMES
        .iter()
        .map(|&name| {
            if let Some(reason) = no_mx.filter(|_| name != AUTH) {
                let na_msg = if reason == NULL_MX_REASON {
                    "Null MX (RFC 7505) — domain does not accept mail"
                } else {
                    "No MX records — email receiving not configured"
                }
                .to_string();
                return Bucket {
                    name: name.to_string(),
                    status: Status::NotApplicable,
                    messages: vec![na_msg],
                };
            }
            aggregate_bucket(name, summary)
        })
        .collect()
}

/// A bucket's verdict is the worst of its categories' and routed cross-validation verdicts;
/// Info and Skip are neutral, and a bucket with no Pass, Warn or Fail is not applicable.
fn aggregate_bucket(name: &str, summary: &BeaconSummary) -> Bucket {
    let mut worst: Option<Status> = None;
    let mut messages: Vec<String> = Vec::new();
    let mut take = |verdict: Status, msgs: Vec<String>| {
        if verdict == Status::NotApplicable {
            return;
        }
        if matches!(verdict, Status::Warn | Status::Fail) {
            messages.extend(msgs);
        }
        if worst
            .as_ref()
            .is_none_or(|w| verdict_rank(&verdict) > verdict_rank(w))
        {
            worst = Some(verdict);
        }
    };

    let sends_no_mail = summary.categories.iter().any(|c| {
        c.name == "cross_validation" && c.sub_checks.iter().any(|s| s.name == BEACON_SENDS_NO_MAIL)
    });

    for (cat_name, bucket) in BUCKETED {
        if *bucket != name {
            continue;
        }
        if let Some(c) = summary.categories.iter().find(|c| c.name == *cat_name) {
            take(c.verdict, category_messages(c));
        }
    }

    if let Some(cv) = summary
        .categories
        .iter()
        .find(|c| c.name == "cross_validation")
    {
        for sub in &cv.sub_checks {
            if route_cross_validation(&sub.name) != Some(name)
                || (sends_no_mail && IGNORED_WHEN_SENDS_NO_MAIL.contains(&sub.name.as_str()))
            {
                continue;
            }
            take(sub.verdict, vec![sub.detail.clone()]);
        }
    }

    // Cap messages at 5 per bucket.
    messages.truncate(5);

    match worst {
        Some(status) => Bucket {
            name: name.to_string(),
            status,
            messages,
        },
        None => Bucket {
            name: name.to_string(),
            status: Status::NotApplicable,
            messages: vec![NOT_APPLICABLE.to_string()],
        },
    }
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

fn build_headline(buckets: &[Bucket], bucket_na: &HashMap<String, String>) -> String {
    let label_of = |check_name: &str| -> String {
        let display = match check_name {
            "email_authentication" => "Auth",
            "email_infrastructure" => "Infra",
            "email_transport" => "Transport",
            "email_brand_policy" => "Brand",
            _ => check_name,
        };
        let bucket = buckets.iter().find(|b| b.name == check_name);
        let symbol = if bucket_na.contains_key(check_name) {
            "N/A".to_string()
        } else {
            match bucket.map(|b| &b.status) {
                Some(Status::Pass) => "OK".to_string(),
                Some(Status::Warn) => "Warn".to_string(),
                Some(Status::Fail) => "Fail".to_string(),
                _ => "N/A".to_string(),
            }
        };
        format!("{display}: {symbol}")
    };

    [
        label_of("email_authentication"),
        label_of("email_infrastructure"),
        label_of("email_transport"),
        label_of("email_brand_policy"),
    ]
    .join("  ")
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
            let known = BUCKETED.iter().any(|(cat, _)| *cat == name)
                || EXCLUDED.iter().any(|(cat, _)| *cat == name);
            if !known {
                problems.push(format!(
                    "category `{name}` is neither BUCKETED nor EXCLUDED"
                ));
            }
        }
        for name in crate::checks::cross_validation::CROSS_VALIDATION_CHECKS {
            if route_cross_validation(name).is_none() {
                problems.push(format!("cross-validation check `{name}` has no bucket"));
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
        assert_eq!(BEACON_SKIPPED, crate::checks::SKIPPED);
        assert_eq!(BEACON_NULL_MX, crate::checks::mx::NULL_MX);
        assert_eq!(
            BEACON_SENDS_NO_MAIL,
            crate::checks::cross_validation::SENDS_NO_MAIL
        );
    }
}
