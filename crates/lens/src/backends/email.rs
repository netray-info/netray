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

    // Beacon's "Skipped" grade means its own internal timeout fired.
    if summary.grade.as_deref() == Some("Skipped") {
        return Err(SectionError::NotApplicable {
            reason: "beacon timeout".to_string(),
        });
    }

    let no_mx = detect_no_mx(&summary);
    let checks = map_buckets(&summary, no_mx);

    let bucket_na: HashMap<String, String> = if no_mx {
        [
            (
                "email_infrastructure".to_string(),
                "no MX records".to_string(),
            ),
            ("email_transport".to_string(), "no MX records".to_string()),
            (
                "email_brand_policy".to_string(),
                "no MX records".to_string(),
            ),
        ]
        .into_iter()
        .collect()
    } else {
        HashMap::new()
    };

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

    let categories: Vec<CategoryVerdict> = data
        .get("verdicts")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(name, verdict)| {
                    let event = category_events.get(name.as_str());
                    CategoryVerdict {
                        name: name.clone(),
                        verdict: verdict.as_str().unwrap_or("Skip").to_string(),
                        message: event
                            .and_then(|d| d.get("detail")?.as_str())
                            .map(|s| s.to_string()),
                        sub_checks: event.map(|d| parse_sub_checks(d)).unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

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

/// Detect whether the domain has no MX records: beacon's `mx` category carries the `no_mx`
/// sub-check. Other `mx` failures (`mx_cname`, `mx_no_addr`, ...) mean MX records exist.
pub fn detect_no_mx(summary: &BeaconSummary) -> bool {
    summary
        .categories
        .iter()
        .any(|c| c.name == "mx" && c.sub_checks.iter().any(|s| s.name == "no_mx"))
}

// ---------------------------------------------------------------------------
// Bucket aggregation
// ---------------------------------------------------------------------------

const BUCKET_AUTH: &[&str] = &["spf", "dkim", "dmarc"];
const BUCKET_INFRA: &[&str] = &["mx", "fcrdns", "dnsbl"];
const BUCKET_TRANSPORT: &[&str] = &["mta_sts", "tls_rpt", "dane"];
const BUCKET_BRAND: &[&str] = &["bimi"];

/// Map beacon's per-category verdicts into four scored CheckResults.
pub fn map_buckets(summary: &BeaconSummary, no_mx: bool) -> Vec<CheckResult> {
    let mut results = vec![aggregate_bucket(
        "email_authentication",
        BUCKET_AUTH,
        summary,
    )];

    if no_mx {
        let na_msg = "No MX records — email receiving not configured".to_string();
        results.push(CheckResult {
            name: "email_infrastructure".to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![na_msg.clone()],
        });
        results.push(CheckResult {
            name: "email_transport".to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![na_msg.clone()],
        });
        results.push(CheckResult {
            name: "email_brand_policy".to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![na_msg],
        });
    } else {
        results.push(aggregate_bucket(
            "email_infrastructure",
            BUCKET_INFRA,
            summary,
        ));
        results.push(aggregate_bucket(
            "email_transport",
            BUCKET_TRANSPORT,
            summary,
        ));
        results.push(aggregate_bucket(
            "email_brand_policy",
            BUCKET_BRAND,
            summary,
        ));
    }

    results
}

fn aggregate_bucket(name: &str, category_names: &[&str], summary: &BeaconSummary) -> CheckResult {
    let mut worst = CheckVerdict::Pass;
    let mut messages: Vec<String> = Vec::new();

    for &cat_name in category_names {
        let cat = summary.categories.iter().find(|c| c.name == cat_name);
        let (verdict, msgs) = match cat {
            None => (CheckVerdict::Skip, Vec::new()),
            Some(c) => (parse_beacon_verdict(&c.verdict), category_messages(c)),
        };

        if verdict_rank(&verdict) > verdict_rank(&worst) {
            worst = verdict.clone();
        }
        match verdict {
            CheckVerdict::Warn | CheckVerdict::Fail | CheckVerdict::NotFound => {
                messages.extend(msgs);
            }
            _ => {}
        }
    }

    // Cap messages at 5 per bucket.
    messages.truncate(5);

    CheckResult {
        name: name.to_string(),
        verdict: worst,
        messages,
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

/// Beacon's `Verdict` serde values; `info` is not scored and maps like `skip`.
fn beacon_verdict(s: &str) -> Option<CheckVerdict> {
    match s {
        "Pass" | "pass" => Some(CheckVerdict::Pass),
        "Warn" | "warn" => Some(CheckVerdict::Warn),
        "Fail" | "fail" => Some(CheckVerdict::Fail),
        "Skip" | "skip" | "Skipped" | "Info" | "info" => Some(CheckVerdict::Skip),
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
