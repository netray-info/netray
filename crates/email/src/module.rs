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

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    BUCKETED
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

/// The former V1 groups of the headline and the categories each covers.
const HEADLINE_GROUPS: &[(&str, &[&str])] = &[
    ("Auth", &["spf", "dkim", "dmarc"]),
    ("Infra", &["mx", "fcrdns", "dnsbl"]),
    ("Transport", &["mta_sts", "tls_rpt", "dane"]),
    ("Brand", &["bimi"]),
];

/// Beacon categories the email section does not score as email categories, with the reason.
pub const EXCLUDED: &[(&str, &str)] = &[
    ("dnssec", "scored in DNS section"),
    ("cross_validation", "routed by sub-check"),
];

/// Cross-validation sub-checks beacon emits that are ignored when the domain sends no mail.
const IGNORED_WHEN_SENDS_NO_MAIL: &[&str] = &["reject_no_dkim", "spf_mx_coverage"];

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

/// Maps beacon's events onto the ten email checks and the V1 headline and extras. A `skipped`
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
            CheckResult {
                id: id.clone(),
                status,
                findings,
                evidence: vec![],
            }
        })
        .collect();

    let headline = build_headline(&checks);

    SectionOutcome::Measured {
        checks,
        presentation: json!({
            "headline": headline,
            "grade": summary.grade,
        }),
    }
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

    let sends_no_mail = summary.categories.iter().any(|c| {
        c.name == "cross_validation" && c.sub_checks.iter().any(|s| s.name == BEACON_SENDS_NO_MAIL)
    });
    let routed: Vec<&SubCheck> = summary
        .categories
        .iter()
        .filter(|c| c.name == "cross_validation")
        .flat_map(|c| &c.sub_checks)
        .filter(|s| {
            route_cross_validation(&s.name) == Some(name)
                && !(sends_no_mail && IGNORED_WHEN_SENDS_NO_MAIL.contains(&s.name.as_str()))
        })
        .collect();

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

/// The V1 headline: each former group shows the worst status over its checks.
fn build_headline(checks: &[CheckResult]) -> String {
    HEADLINE_GROUPS
        .iter()
        .map(|(display, members)| {
            let worst = checks
                .iter()
                .filter(|c| {
                    members
                        .iter()
                        .any(|m| c.id.to_string() == format!("email.{m}"))
                })
                .map(|c| c.status)
                .max_by_key(|s| match s {
                    Status::Fail => 3,
                    Status::Warn => 2,
                    Status::Pass => 1,
                    _ => 0,
                });
            let symbol = match worst {
                Some(Status::Pass) => "OK",
                Some(Status::Warn) => "Warn",
                Some(Status::Fail) => "Fail",
                _ => "N/A",
            };
            format!("{display}: {symbol}")
        })
        .collect::<Vec<_>>()
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
