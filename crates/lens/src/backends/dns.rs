use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;

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
// Public entry point
// ---------------------------------------------------------------------------

pub async fn check_dns(
    client: &reqwest::Client,
    dns_url: &str,
    domain: &str,
    timeout: Duration,
) -> Result<DnsBackendResult, AppError> {
    let url = format!(
        "{}/api/check",
        dns_url.trim_end_matches('/'),
    );

    let body = serde_json::json!({ "domain": domain });

    // Connect and get headers — SSE stream starts immediately.
    let resp = tokio::time::timeout(
        timeout,
        client
            .post(&url)
            .header("Accept", "text/event-stream")
            .json(&body)
            .send(),
    )
    .await
    .map_err(|_| AppError::Timeout)?
    .map_err(|e| AppError::BackendError {
        backend: "dns",
        message: e.to_string(),
    })?;

    if !resp.status().is_success() {
        return Err(AppError::BackendError {
            backend: "dns",
            message: format!("prism returned HTTP {}", resp.status()),
        });
    }

    // Collect SSE events until "done" or timeout.
    let events = tokio::time::timeout(timeout, collect_sse(resp))
        .await
        .map_err(|_| AppError::Timeout)?
        .map_err(|e| AppError::BackendError {
            backend: "dns",
            message: format!("failed to read prism stream: {e}"),
        })?;

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
                        if !cur_data.is_empty() {
                            if let Ok(data) = serde_json::from_str::<Value>(&cur_data) {
                                let done = cur_type == "done";
                                events.push(serde_json::json!({
                                    "type": cur_type,
                                    "data": data,
                                }));
                                if done {
                                    return Ok(events);
                                }
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
                if let Some(data) = event.get("data") {
                    if let Some(check) = parse_lint_event(data) {
                        checks.push(check);
                    }
                }
            }
            _ => {}
        }
    }

    let raw_headline = build_headline(&checks);
    let detail_url = format!(
        "{}/?q={}+check",
        dns_url.trim_end_matches('/'),
        urlencoding::encode(domain),
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

    let lookups = data
        .pointer("/lookups/lookups")
        .and_then(|v| v.as_array());

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
            if let Ok(ip) = ip_str.parse::<IpAddr>() {
                if seen.insert(ip) {
                    resolved_ips.push(ip);
                }
            }
        }
    }
}

/// Parse a lint event and return a single CheckResult representing the worst verdict.
///
/// Lint event data shape:
/// ```json
/// { "category": "spf", "results": [ {"Ok": "msg"} | {"Warning": "msg"} | {"Failed": "msg"} | {"NotFound": null} ] }
/// ```
fn parse_lint_event(data: &Value) -> Option<CheckResult> {
    let category = data.get("category").and_then(|v| v.as_str())?;
    let results = data.get("results").and_then(|v| v.as_array())?;

    let mut worst = CheckVerdict::Pass;

    for result in results {
        let (verdict, _msg) = classify_lint_result(result);
        if verdict_rank(&verdict) > verdict_rank(&worst) {
            worst = verdict;
        }
    }

    // If no results, treat as not found.
    if results.is_empty() {
        worst = CheckVerdict::NotFound;
    }

    Some(CheckResult {
        name: category.to_string(),
        verdict: worst,
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

// ---------------------------------------------------------------------------
// URL encoding helper
// ---------------------------------------------------------------------------

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char);
                }
                _ => {
                    out.push('%');
                    out.push_str(&format!("{b:02X}"));
                }
            }
        }
        out
    }
}
