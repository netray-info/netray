use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;
use tracing::Instrument;

use crate::backends::{Backend, BackendContext, BackendExtra, BackendResult};
use crate::check::SectionError;
use crate::scoring::engine::{CheckResult, CheckVerdict};

// ---------------------------------------------------------------------------
// Backend struct
// ---------------------------------------------------------------------------

pub struct EmailBackend {
    pub email_url: String,
    pub public_url: String,
    pub timeout: Duration,
    pub client: reqwest::Client,
}

impl Backend for EmailBackend {
    fn section(&self) -> &'static str {
        "email"
    }

    fn run(
        &self,
        domain: &str,
        context: &BackendContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    > {
        let client = self.client.clone();
        let domain = domain.to_string();
        let email_url = self.email_url.clone();
        let public_url = self.public_url.clone();
        let timeout = self.timeout;
        let selectors = context.dkim_selectors.clone();
        let fwd = context.forward_headers.clone();

        Box::pin(async move {
            let url = format!("{}/inspect", email_url.trim_end_matches('/'));
            let span = tracing::info_span!("backend_call", service = "beacon", url = %url);
            check_email(
                &client,
                &url,
                &domain,
                &public_url,
                selectors,
                timeout,
                &fwd,
            )
            .instrument(span)
            .await
        })
    }
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

async fn check_email(
    client: &reqwest::Client,
    url: &str,
    domain: &str,
    public_url: &str,
    selectors: Option<Vec<String>>,
    timeout: Duration,
    fwd: &reqwest::header::HeaderMap,
) -> Result<BackendResult, SectionError> {
    let mut body = serde_json::json!({ "domain": domain });
    if let Some(ref sels) = selectors
        && !sels.is_empty()
    {
        body["dkim_selectors"] = serde_json::json!(sels);
    }

    let deadline = tokio::time::Instant::now() + timeout;
    let send_fut = client
        .post(url)
        .headers(fwd.clone())
        .header("Accept", "text/event-stream")
        .json(&body)
        .send();

    let resp = tokio::time::timeout_at(deadline, send_fut)
        .await
        .map_err(|_| {
            tracing::warn!(service = "beacon", url = %url, error = "timeout", "backend call failed");
            SectionError::Timeout
        })?
        .map_err(|e| {
            tracing::warn!(service = "beacon", url = %url, error = %e, "backend call failed");
            SectionError::BackendError(e.to_string())
        })?;

    if !resp.status().is_success() {
        tracing::warn!(service = "beacon", url = %url, status = %resp.status(), "backend call failed");
        return Err(SectionError::BackendError(format!(
            "beacon returned HTTP {}",
            resp.status()
        )));
    }

    // Drain until the "summary" event, with the same timeout budget.
    let events = tokio::time::timeout_at(deadline, super::sse::collect_until_type(resp, "summary"))
        .await
        .map_err(|_| {
            tracing::warn!(service = "beacon", url = %url, error = "stream timeout", "backend call failed");
            SectionError::Timeout
        })?
        .map_err(|e| {
            tracing::warn!(service = "beacon", url = %url, error = %e, "backend call failed");
            SectionError::BackendError(format!("failed to read beacon stream: {e}"))
        })?;

    let event_types: Vec<&str> = events
        .iter()
        .filter_map(|e| e.get("type").and_then(|v| v.as_str()))
        .collect();
    tracing::debug!(
        service = "beacon",
        url = %url,
        events = events.len(),
        ?event_types,
        "backend call succeeded"
    );

    let summary = parse_summary(&events)?;

    let no_mx_reason = no_mx_reason(&summary);
    let checks = map_buckets(&summary, no_mx_reason);

    let bucket_na: HashMap<String, String> = checks
        .iter()
        .filter(|c| matches!(c.verdict, CheckVerdict::Skip))
        .map(|c| {
            (
                c.name.clone(),
                no_mx_reason.unwrap_or(NOT_APPLICABLE).to_string(),
            )
        })
        .collect();

    let raw_headline = build_headline(&checks, &bucket_na);
    let detail_url = format!(
        "{}/?domain={}",
        if public_url.is_empty() {
            "https://email.netray.info"
        } else {
            public_url.trim_end_matches('/')
        },
        super::percent_encode(domain),
    );

    Ok(BackendResult {
        checks,
        extra: BackendExtra::Email {
            raw_headline,
            detail_url,
            grade: summary.grade.clone(),
            bucket_na,
        },
    })
}

// ---------------------------------------------------------------------------
// Beacon vocabulary (pinned against beacon by tests/contract_beacon.rs)
// ---------------------------------------------------------------------------

/// Beacon's wire value for a check that did not run: the summary grade after its own timeout
/// and the sub-check name of a category that did not complete.
pub const BEACON_SKIPPED: &str = "skipped";
/// Beacon's `mx` sub-check for a Null MX (RFC 7505) domain.
pub const BEACON_NULL_MX: &str = "null_mx";
/// Beacon's cross-validation sub-check for a domain that declares it sends no mail.
pub const BEACON_SENDS_NO_MAIL: &str = "sends_no_mail";

const NOT_APPLICABLE: &str = "not applicable";

const AUTH: &str = "email_authentication";
const INFRA: &str = "email_infrastructure";
const TRANSPORT: &str = "email_transport";
const BRAND: &str = "email_brand_policy";
const BUCKET_NAMES: [&str; 4] = [AUTH, INFRA, TRANSPORT, BRAND];

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

/// Beacon categories lens does not score as email categories, with the reason.
pub const EXCLUDED: &[(&str, &str)] = &[
    ("dnssec", "scored in DNS section"),
    ("cross_validation", "routed by sub-check"),
];

/// Cross-validation sub-checks beacon emits that lens ignores when the domain sends no mail.
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
    pub verdict: String,
    pub detail: String,
}

#[derive(Debug)]
pub struct CategoryVerdict {
    pub name: String,
    pub verdict: String,
    pub message: Option<String>,
    pub sub_checks: Vec<SubCheck>,
}

#[derive(Debug)]
pub struct BeaconSummary {
    pub grade: Option<String>,
    pub categories: Vec<CategoryVerdict>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Extract the summary event from drained SSE events.
///
/// Events carry their kind in the JSON `type` field. The summary is the `summary` event; as a
/// fallback, the event whose data contains a `"grade"` field. Verdicts come from its `verdicts`
/// map (category -> verdict); messages and sub-checks come from the `category` events.
pub fn parse_summary(events: &[Value]) -> Result<BeaconSummary, SectionError> {
    let data = events
        .iter()
        .find(|e| e.get("type").and_then(|v| v.as_str()) == Some("summary"))
        .or_else(|| {
            events
                .iter()
                .find(|e| e.get("data").and_then(|d| d.get("grade")).is_some())
        })
        .and_then(|e| e.get("data"))
        .ok_or_else(|| SectionError::BackendError("no summary event from beacon".to_string()))?;

    tracing::debug!(summary_data = ?data, "beacon summary data");

    let grade = data
        .get("grade")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Per-category events carry the detail and sub-checks; the summary carries the verdicts.
    if grade.as_deref() == Some(BEACON_SKIPPED) {
        return Err(SectionError::Timeout);
    }

    let category_events: HashMap<&str, &Value> = events
        .iter()
        .filter_map(|e| e.get("data"))
        .filter(|d| d.get("type").and_then(|v| v.as_str()) == Some("category"))
        .filter_map(|d| Some((d.get("category")?.as_str()?, d)))
        .collect();

    for d in category_events.values() {
        if let Some(v) = d.get("verdict").and_then(|v| v.as_str()) {
            check_known_verdict(v)?;
        }
    }

    let mut categories: Vec<CategoryVerdict> = Vec::new();
    if let Some(verdicts) = data.get("verdicts").and_then(|v| v.as_object()) {
        for (name, verdict) in verdicts {
            let known =
                BUCKETED.iter().any(|(c, _)| c == name) || EXCLUDED.iter().any(|(c, _)| c == name);
            if !known {
                return Err(SectionError::BackendError(
                    super::unknown_verdict("email", name).to_string(),
                ));
            }
            let event = category_events.get(name.as_str()).ok_or_else(|| {
                SectionError::BackendError(format!("beacon sent no category event for `{name}`"))
            })?;
            let sub_checks = parse_sub_checks(event);
            if sub_checks.iter().any(|s| s.name == BEACON_SKIPPED) {
                return Err(SectionError::BackendError(format!(
                    "beacon category `{name}` did not complete"
                )));
            }
            if name == "cross_validation"
                && let Some(unrouted) = sub_checks
                    .iter()
                    .find(|s| route_cross_validation(&s.name).is_none())
            {
                return Err(SectionError::BackendError(
                    super::unknown_verdict("email", &unrouted.name).to_string(),
                ));
            }
            categories.push(CategoryVerdict {
                name: name.clone(),
                verdict: verdict.as_str().unwrap_or("Skip").to_string(),
                message: event
                    .get("detail")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string()),
                sub_checks,
            });
        }
    }

    for cat in &categories {
        check_known_verdict(&cat.verdict)?;
        for sub in &cat.sub_checks {
            check_known_verdict(&sub.verdict)?;
        }
    }

    Ok(BeaconSummary { grade, categories })
}

fn parse_sub_checks(category_event: &Value) -> Vec<SubCheck> {
    category_event
        .get("sub_checks")
        .and_then(|v| v.as_array())
        .map(|subs| {
            subs.iter()
                .filter_map(|s| {
                    Some(SubCheck {
                        name: s.get("name")?.as_str()?.to_string(),
                        verdict: s.get("verdict")?.as_str()?.to_string(),
                        detail: s.get("detail")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
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

/// Map beacon's per-category verdicts into four scored CheckResults.
pub fn map_buckets(summary: &BeaconSummary, no_mx: Option<&str>) -> Vec<CheckResult> {
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
                return CheckResult {
                    name: name.to_string(),
                    verdict: CheckVerdict::Skip,
                    messages: vec![na_msg],
                };
            }
            aggregate_bucket(name, summary)
        })
        .collect()
}

/// A bucket's verdict is the worst of its categories' and routed cross-validation verdicts;
/// Info and Skip are neutral, and a bucket with no Pass, Warn or Fail is not applicable.
fn aggregate_bucket(name: &str, summary: &BeaconSummary) -> CheckResult {
    let mut worst: Option<CheckVerdict> = None;
    let mut messages: Vec<String> = Vec::new();
    let mut take = |verdict: CheckVerdict, msgs: Vec<String>| {
        if matches!(verdict, CheckVerdict::Skip) {
            return;
        }
        if matches!(verdict, CheckVerdict::Warn | CheckVerdict::Fail) {
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
            take(parse_beacon_verdict(&c.verdict), category_messages(c));
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
            take(parse_beacon_verdict(&sub.verdict), vec![sub.detail.clone()]);
        }
    }

    // Cap messages at 5 per bucket.
    messages.truncate(5);

    match worst {
        Some(verdict) => CheckResult {
            name: name.to_string(),
            verdict,
            messages,
        },
        None => CheckResult {
            name: name.to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![NOT_APPLICABLE.to_string()],
        },
    }
}

/// Reasons for a category: the details of its warn/fail sub-checks, else the category detail.
fn category_messages(cat: &CategoryVerdict) -> Vec<String> {
    let reasons: Vec<String> = cat
        .sub_checks
        .iter()
        .filter(|s| {
            matches!(
                parse_beacon_verdict(&s.verdict),
                CheckVerdict::Warn | CheckVerdict::Fail
            )
        })
        .map(|s| s.detail.clone())
        .collect();
    if reasons.is_empty() {
        cat.message.iter().cloned().collect()
    } else {
        reasons
    }
}

/// Beacon's `Verdict` serde values; `info` is neutral and maps explicitly to `Skip`.
fn beacon_verdict(s: &str) -> Option<CheckVerdict> {
    match s {
        "Pass" | "pass" => Some(CheckVerdict::Pass),
        "Warn" | "warn" => Some(CheckVerdict::Warn),
        "Fail" | "fail" => Some(CheckVerdict::Fail),
        "Info" | "info" => Some(CheckVerdict::Skip),
        "Skip" | "skip" | "Skipped" | "skipped" => Some(CheckVerdict::Skip),
        _ => None,
    }
}

/// Map a verdict `parse_summary` has already checked against `beacon_verdict`.
fn parse_beacon_verdict(s: &str) -> CheckVerdict {
    beacon_verdict(s).unwrap_or(CheckVerdict::Skip)
}

fn check_known_verdict(s: &str) -> Result<(), SectionError> {
    if beacon_verdict(s).is_none() {
        return Err(SectionError::BackendError(
            super::unknown_verdict("email", s).to_string(),
        ));
    }
    Ok(())
}

fn verdict_rank(v: &CheckVerdict) -> u8 {
    match v {
        CheckVerdict::Pass | CheckVerdict::Skip => 0,
        CheckVerdict::NotFound => 1,
        CheckVerdict::Warn => 2,
        CheckVerdict::Fail => 3,
    }
}

fn build_headline(checks: &[CheckResult], bucket_na: &HashMap<String, String>) -> String {
    let label_of = |check_name: &str| -> String {
        let display = match check_name {
            "email_authentication" => "Auth",
            "email_infrastructure" => "Infra",
            "email_transport" => "Transport",
            "email_brand_policy" => "Brand",
            _ => check_name,
        };
        let verdict = checks.iter().find(|c| c.name == check_name);
        let symbol = if bucket_na.contains_key(check_name) {
            "N/A".to_string()
        } else {
            match verdict.map(|c| &c.verdict) {
                Some(CheckVerdict::Pass) => "OK".to_string(),
                Some(CheckVerdict::Warn) => "Warn".to_string(),
                Some(CheckVerdict::Fail) => "Fail".to_string(),
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
