use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;
use tracing::Instrument;

use crate::backends::{Backend, BackendContext, BackendExtra, BackendResult};
use crate::check::SectionError;
use crate::error::AppError;
use crate::scoring::engine::{CheckResult, CheckVerdict};

// ---------------------------------------------------------------------------
// Public result type
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct DnsBackendResult {
    pub checks: Vec<CheckResult>,
    pub resolved_ips: Vec<IpAddr>,
    pub raw_headline: String,
    pub detail_url: String,
}

// ---------------------------------------------------------------------------
// Backend trait implementation
// ---------------------------------------------------------------------------

pub struct DnsBackend {
    pub dns_url: String,
    pub public_url: String,
    pub timeout: Duration,
    pub client: reqwest::Client,
    pub dns_servers: Vec<String>,
}

impl Backend for DnsBackend {
    fn section(&self) -> &'static str {
        "dns"
    }

    fn run(
        &self,
        domain: &str,
        _context: &BackendContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    > {
        let client = self.client.clone();
        let domain = domain.to_string();
        let dns_url = self.dns_url.clone();
        let public_url = self.public_url.clone();
        let timeout = self.timeout;
        let dns_servers = self.dns_servers.clone();
        Box::pin(async move {
            let mut result = check_dns(&client, &dns_url, &domain, &dns_servers, timeout)
                .await
                .map_err(|e| match e {
                    AppError::Timeout => SectionError::Timeout,
                    other => SectionError::BackendError(other.to_string()),
                })?;
            result.detail_url = format!(
                "{}/?q={}+%2Bcheck",
                public_url.trim_end_matches('/'),
                super::percent_encode(&domain),
            );
            Ok(BackendResult {
                checks: result.checks,
                extra: BackendExtra::Dns {
                    resolved_ips: result.resolved_ips,
                    raw_headline: result.raw_headline,
                    detail_url: result.detail_url,
                },
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn check_dns(
    client: &reqwest::Client,
    dns_url: &str,
    domain: &str,
    dns_servers: &[String],
    timeout: Duration,
) -> Result<DnsBackendResult, AppError> {
    let url = format!("{}/api/check", dns_url.trim_end_matches('/'),);
    let span = tracing::info_span!("backend_call", service = "prism", url = %url);
    check_dns_inner(client, &url, domain, dns_url, dns_servers, timeout)
        .instrument(span)
        .await
}

async fn check_dns_inner(
    client: &reqwest::Client,
    url: &str,
    domain: &str,
    dns_url: &str,
    dns_servers: &[String],
    timeout: Duration,
) -> Result<DnsBackendResult, AppError> {
    let mut body = serde_json::json!({ "domain": domain });
    if !dns_servers.is_empty() {
        body["servers"] = serde_json::json!(dns_servers);
    }

    // Connect and get headers — SSE stream starts immediately.
    let resp = tokio::time::timeout(
        timeout,
        client
            .post(url)
            .header("Accept", "text/event-stream")
            .json(&body)
            .send(),
    )
    .await
    .map_err(|_| {
        tracing::warn!(service = "prism", url = %url, error = "timeout", "backend call failed");
        AppError::Timeout
    })?
    .map_err(|e| {
        tracing::warn!(service = "prism", url = %url, error = %e, "backend call failed");
        AppError::BackendError {
            backend: "dns",
            message: e.to_string(),
        }
    })?;

    if !resp.status().is_success() {
        tracing::warn!(service = "prism", url = %url, status = %resp.status(), "backend call failed");
        return Err(AppError::BackendError {
            backend: "dns",
            message: format!("prism returned HTTP {}", resp.status()),
        });
    }

    // Collect SSE events until "done" or timeout.
    let events = tokio::time::timeout(timeout, collect_sse(resp))
        .await
        .map_err(|_| {
            tracing::warn!(service = "prism", url = %url, error = "stream timeout", "backend call failed");
            AppError::Timeout
        })?
        .map_err(|e| {
            tracing::warn!(service = "prism", url = %url, error = %e, "backend call failed");
            AppError::BackendError {
                backend: "dns",
                message: format!("failed to read prism stream: {e}"),
            }
        })?;

    tracing::debug!(service = "prism", url = %url, events = events.len(), "backend call succeeded");
    parse_events(events, domain, dns_url)
}

// ---------------------------------------------------------------------------
// SSE stream reader
// ---------------------------------------------------------------------------

/// Read an SSE stream from a reqwest response.
///
/// Returns events as `{"type": "...", "data": {...}}` values — the same shape
/// the former `stream=false` CollectedResponse used, so `parse_events` works
/// without changes.
async fn collect_sse(resp: reqwest::Response) -> Result<Vec<Value>, String> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut events: Vec<Value> = Vec::new();
    let mut cur_type = String::new();
    let mut cur_data = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        loop {
            match buf.find('\n') {
                None => break,
                Some(pos) => {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf = buf[pos + 1..].to_string();

                    if line.is_empty() {
                        // Blank line = dispatch current event.
                        if !cur_data.is_empty()
                            && let Ok(data) = serde_json::from_str::<Value>(&cur_data)
                        {
                            let done = cur_type == "done";
                            events.push(serde_json::json!({
                                "type": cur_type,
                                "data": data,
                            }));
                            if done {
                                return Ok(events);
                            }
                        }
                        cur_type.clear();
                        cur_data.clear();
                    } else if let Some(rest) = line.strip_prefix("event: ") {
                        cur_type = rest.to_string();
                    } else if let Some(rest) = line.strip_prefix("data: ") {
                        cur_data = rest.to_string();
                    }
                }
            }
        }
    }

    Ok(events)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_events(
    events: Vec<Value>,
    domain: &str,
    dns_url: &str,
) -> Result<DnsBackendResult, AppError> {
    let mut checks: Vec<CheckResult> = Vec::new();
    let mut resolved_ips: Vec<IpAddr> = Vec::new();
    let mut seen_ips: HashSet<IpAddr> = HashSet::new();

    for event in &events {
        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "batch" => {
                if let Some(data) = event.get("data") {
                    collect_ips_from_batch(data, &mut resolved_ips, &mut seen_ips);
                }
            }
            "lint" => {
                if let Some(data) = event.get("data")
                    && let Some(check) = parse_lint_event(data)
                {
                    checks.push(check);
                }
            }
            _ => {}
        }
    }

    // If DNSKEY algorithm check is NotFound (no DNSKEY records → DNSSEC not deployed),
    // dnskey_algorithm and dnssec_rollover are N/A — skip them rather than penalising.
    let dnssec_absent = checks
        .iter()
        .any(|c| c.name == "dnskey_algorithm" && c.verdict == CheckVerdict::NotFound);
    if dnssec_absent {
        for check in &mut checks {
            if check.name == "dnskey_algorithm" || check.name == "dnssec_rollover" {
                check.verdict = CheckVerdict::Skip;
                check.messages.clear();
            }
        }
    }

    let raw_headline = build_headline(&checks);
    let detail_url = format!(
        "{}/?q={}+%2Bcheck",
        dns_url.trim_end_matches('/'),
        super::percent_encode(domain),
    );

    Ok(DnsBackendResult {
        checks,
        resolved_ips,
        raw_headline,
        detail_url,
    })
}

/// Extract IPs from a batch event's data.
///
/// Batch event data shape (first resolver):
/// ```json
/// { "record_type": "A", "lookups": { "lookups": [ { "result": { "Response": { "records": [
///   { "name": "...", "type": "A", "data": {"A": "1.2.3.4"} }
/// ] } } } ] } }
/// ```
fn collect_ips_from_batch(
    data: &Value,
    resolved_ips: &mut Vec<IpAddr>,
    seen: &mut HashSet<IpAddr>,
) {
    let record_type = data
        .get("record_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if record_type != "A" && record_type != "AAAA" {
        return;
    }

    let lookups = data.pointer("/lookups/lookups").and_then(|v| v.as_array());

    let lookups = match lookups {
        Some(l) => l,
        None => return,
    };

    for lookup in lookups {
        let records = lookup
            .pointer("/result/Response/records")
            .and_then(|v| v.as_array());

        let records = match records {
            Some(r) => r,
            None => continue,
        };

        for record in records {
            // RData serializes as a tagged enum: {"A": "1.2.3.4"} or {"AAAA": "::1"}
            let ip_str = record
                .get("data")
                .and_then(|d| d.get(record_type))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Ok(ip) = ip_str.parse::<IpAddr>()
                && seen.insert(ip)
            {
                resolved_ips.push(ip);
            }
        }
    }
}

/// Parse a lint event and return a single CheckResult representing the worst verdict.
///
/// All non-passing messages (warn, fail, not-found) are collected and attached to
/// the result so the frontend can show the user why points were deducted.
///
/// Lint event data shape:
/// ```json
/// { "category": "spf", "results": [ {"Ok": "msg"} | {"Warning": "msg"} | {"Failed": "msg"} | {"NotFound": null} ] }
/// ```
fn parse_lint_event(data: &Value) -> Option<CheckResult> {
    let category = data.get("category").and_then(|v| v.as_str())?;
    let results = data.get("results").and_then(|v| v.as_array())?;

    let mut worst = CheckVerdict::Pass;
    let mut messages: Vec<String> = Vec::new();

    for result in results {
        let (verdict, msg) = classify_lint_result(result);
        if verdict_rank(&verdict) > verdict_rank(&worst) {
            worst = verdict.clone();
        }
        match verdict {
            CheckVerdict::Warn | CheckVerdict::Fail | CheckVerdict::NotFound => {
                messages.push(msg.unwrap_or_else(|| "Not found".to_string()));
            }
            _ => {}
        }
    }

    // If no results at all, treat as not found.
    if results.is_empty() {
        worst = CheckVerdict::NotFound;
        messages.push("Not found".to_string());
    }

    Some(CheckResult {
        name: category.to_string(),
        verdict: worst,
        messages,
    })
}

/// Rank verdicts so we can find the worst: higher rank = worse.
/// Order: Pass < NotFound < Warn < Fail.
fn verdict_rank(v: &CheckVerdict) -> u8 {
    match v {
        CheckVerdict::Pass | CheckVerdict::Skip => 0,
        CheckVerdict::NotFound => 1,
        CheckVerdict::Warn => 2,
        CheckVerdict::Fail => 3,
    }
}

/// Map a single lint result item to (CheckVerdict, optional message).
fn classify_lint_result(result: &Value) -> (CheckVerdict, Option<String>) {
    if let Some(obj) = result.as_object() {
        if obj.contains_key("Ok") {
            return (CheckVerdict::Pass, None);
        }
        if let Some(msg) = obj.get("Warning").and_then(|v| v.as_str()) {
            return (CheckVerdict::Warn, Some(msg.to_string()));
        }
        if let Some(msg) = obj.get("Failed").and_then(|v| v.as_str()) {
            return (CheckVerdict::Fail, Some(msg.to_string()));
        }
        if obj.contains_key("NotFound") {
            return (CheckVerdict::NotFound, None);
        }
    }
    (CheckVerdict::Pass, None)
}

/// Build a summary headline from the collected checks.
///
/// Shows the key email-security checks in order: SPF, DMARC, DNSSEC, MTA-STS.
fn build_headline(checks: &[CheckResult]) -> String {
    let keys = ["spf", "dmarc", "dnssec", "mta_sts"];
    let labels = ["SPF", "DMARC", "DNSSEC", "MTA-STS"];

    let parts: Vec<String> = keys
        .iter()
        .zip(labels.iter())
        .map(|(key, label)| {
            let symbol = match checks.iter().find(|c| c.name == *key) {
                Some(c) => match c.verdict {
                    CheckVerdict::Pass => "\u{2713}",
                    CheckVerdict::Warn => "~",
                    CheckVerdict::Fail => "\u{2717}",
                    CheckVerdict::NotFound | CheckVerdict::Skip => "\u{2013}",
                },
                None => "\u{2013}",
            };
            format!("{label} {symbol}")
        })
        .collect();

    parts.join("  ")
}
