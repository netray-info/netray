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

        Box::pin(async move {
            let url = format!("{}/inspect", email_url.trim_end_matches('/'));
            let span = tracing::info_span!("backend_call", service = "beacon", url = %url);
            check_email(&client, &url, &domain, &public_url, selectors, timeout)
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
) -> Result<BackendResult, SectionError> {
    let mut body = serde_json::json!({ "domain": domain });
    if let Some(ref sels) = selectors
        && !sels.is_empty()
    {
        body["dkim_selectors"] = serde_json::json!(sels);
    }

    let send_fut = client
        .post(url)
        .header("Accept", "text/event-stream")
        .json(&body)
        .send();

    let resp = tokio::time::timeout(timeout, send_fut)
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
    let events = tokio::time::timeout(timeout, super::sse::collect(resp, "summary"))
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
pub struct CategoryVerdict {
    pub name: String,
    pub verdict: String,
    pub message: Option<String>,
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
/// Beacon may omit `event:` type lines and send raw `data:` only. In that case all
/// events arrive with an empty type. We find the summary by looking for an explicit
/// `event: summary` first, then fall back to the event whose data contains a `"grade"` field.
pub fn parse_summary(events: &[Value]) -> Result<BeaconSummary, SectionError> {
    let data = events
        .iter()
        .find(|e| e.get("type").and_then(|v| v.as_str()) == Some("summary"))
        .or_else(|| {
            // Beacon sends no event type names — locate by data content.
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

    let mut categories = Vec::new();
    if let Some(cats) = data.get("categories") {
        if let Some(obj) = cats.as_object() {
            // Object form: {"spf": {"verdict": "Pass", ...}, ...}
            for (name, cat_data) in obj {
                let verdict = cat_data
                    .get("verdict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Skip")
                    .to_string();
                let message = cat_data
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                categories.push(CategoryVerdict {
                    name: name.clone(),
                    verdict,
                    message,
                });
            }
        } else if let Some(arr) = cats.as_array() {
            // Array form: [{"name": "spf", "verdict": "Pass", ...}, ...]
            for cat_data in arr {
                let name = match cat_data.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let verdict = cat_data
                    .get("verdict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Skip")
                    .to_string();
                let message = cat_data
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                categories.push(CategoryVerdict {
                    name,
                    verdict,
                    message,
                });
            }
        }
    }

    Ok(BeaconSummary { grade, categories })
}

/// Detect whether the domain has no MX records from the beacon summary.
pub fn detect_no_mx(summary: &BeaconSummary) -> bool {
    summary
        .categories
        .iter()
        .any(|c| c.name == "mx" && c.verdict.eq_ignore_ascii_case("Fail"))
}

// ---------------------------------------------------------------------------
// Bucket aggregation
// ---------------------------------------------------------------------------

const BUCKET_AUTH: &[&str] = &["spf", "dkim", "dmarc"];
const BUCKET_INFRA: &[&str] = &["mx", "fcrdns", "dnsbl"];
const BUCKET_TRANSPORT: &[&str] = &["mta_sts", "tlsrpt", "dane"];
const BUCKET_BRAND: &[&str] = &["bimi", "dmarc_policy"];

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
        let (verdict, msg) = match cat {
            None => (CheckVerdict::Skip, None),
            Some(c) => (parse_beacon_verdict(&c.verdict), c.message.clone()),
        };

        if verdict_rank(&verdict) > verdict_rank(&worst) {
            worst = verdict.clone();
        }
        match verdict {
            CheckVerdict::Warn | CheckVerdict::Fail | CheckVerdict::NotFound => {
                if let Some(m) = msg {
                    messages.push(m);
                }
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

fn parse_beacon_verdict(s: &str) -> CheckVerdict {
    match s {
        "Pass" | "pass" => CheckVerdict::Pass,
        "Warn" | "warn" => CheckVerdict::Warn,
        "Fail" | "fail" => CheckVerdict::Fail,
        "Skip" | "skip" | "Skipped" => CheckVerdict::Skip,
        _ => CheckVerdict::Skip,
    }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn load_fixture(name: &str) -> Value {
        let path = format!("{}/tests/email_fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let contents =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture not found: {path}"));
        serde_json::from_str(&contents).expect("fixture must be valid JSON")
    }

    fn summary_event(data: Value) -> Vec<Value> {
        vec![json!({ "type": "summary", "data": data })]
    }

    fn run_fixture(name: &str) -> Result<BackendResult, SectionError> {
        let data = load_fixture(name);
        let events = summary_event(data);
        let summary = parse_summary(&events)?;
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
        let detail_url = format!("https://email.netray.info/?domain=example.com");
        Ok(BackendResult {
            checks,
            extra: BackendExtra::Email {
                raw_headline,
                detail_url,
                grade: summary.grade,
                bucket_na,
            },
        })
    }

    #[test]
    fn mail_domain_all_pass() {
        let result = run_fixture("mail_domain.json").expect("fixture must succeed");
        let checks = &result.checks;
        assert_eq!(checks.len(), 4);
        for check in checks {
            assert_eq!(
                check.verdict,
                CheckVerdict::Pass,
                "expected pass for {}, got {:?}",
                check.name,
                check.verdict
            );
        }
        match &result.extra {
            BackendExtra::Email {
                bucket_na, grade, ..
            } => {
                assert!(bucket_na.is_empty(), "no MX N/A expected for mail domain");
                assert_eq!(grade.as_deref(), Some("A"));
            }
            _ => panic!("unexpected extra variant"),
        }
    }

    #[test]
    fn no_mx_marks_three_buckets_na() {
        let result = run_fixture("no_mx.json").expect("fixture must succeed");
        let checks = &result.checks;

        let auth = checks
            .iter()
            .find(|c| c.name == "email_authentication")
            .unwrap();
        assert_eq!(
            auth.verdict,
            CheckVerdict::Pass,
            "auth should pass in no-MX scenario"
        );

        for bucket in &[
            "email_infrastructure",
            "email_transport",
            "email_brand_policy",
        ] {
            let check = checks.iter().find(|c| c.name == *bucket).unwrap();
            assert_eq!(
                check.verdict,
                CheckVerdict::Skip,
                "{bucket} must be Skip when no MX"
            );
            assert!(
                check.messages.iter().any(|m| m.contains("No MX")),
                "{bucket} must have N/A message"
            );
        }

        match &result.extra {
            BackendExtra::Email { bucket_na, .. } => {
                assert_eq!(bucket_na.len(), 3, "exactly 3 N/A buckets expected");
                assert!(bucket_na.contains_key("email_infrastructure"));
                assert!(bucket_na.contains_key("email_transport"));
                assert!(bucket_na.contains_key("email_brand_policy"));
            }
            _ => panic!("unexpected extra variant"),
        }
    }

    #[test]
    fn beacon_timeout_returns_not_applicable() {
        let err = run_fixture("beacon_timeout.json").unwrap_err();
        assert!(
            matches!(err, SectionError::NotApplicable { ref reason } if reason == "beacon timeout"),
            "beacon Skipped grade must map to NotApplicable, got {:?}",
            err
        );
    }

    #[test]
    fn partial_fail_worst_verdict_aggregation() {
        let result = run_fixture("partial_fail.json").expect("fixture must succeed");
        let auth = result
            .checks
            .iter()
            .find(|c| c.name == "email_authentication")
            .expect("auth bucket must exist");
        assert_eq!(
            auth.verdict,
            CheckVerdict::Warn,
            "worst of Pass/Warn/Pass must be Warn"
        );
        assert!(!auth.messages.is_empty(), "warn bucket must carry messages");
    }
}
