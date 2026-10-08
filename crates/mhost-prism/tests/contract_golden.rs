// Golden test for the batch SSE stream lens consumes (`POST /api/check`).
//
// The golden `tests/fixtures/contracts/prism.sse` is written here, from prism's real
// `BatchEvent` and mhost's real lint functions, framed by axum's real `Sse` response.
// lens's `tests/contract_backends.rs` reads the same file.
//
// The `/api/check` handler needs live DNS, so the stream is assembled here from prism's real
// `LintEvent` and `CheckDoneEvent`; the lint results inside are mhost's `CheckResult` values.
//
// UPDATE_GOLDEN=1 cargo test -p prism --test contract_golden   writes the golden.

use std::convert::Infallible;
use std::path::PathBuf;

use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use http_body_util::BodyExt;
use mhost::lints::{CheckResult, check_caa, check_ns_count, check_spf};
use mhost::resolver::Lookups;
use prism::api::{BatchEvent, CheckDoneEvent, LintEvent};
use prism::record_format;

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "golden {} is missing; run with UPDATE_GOLDEN=1 to write it",
            path.display()
        )
    });
    assert!(
        committed == actual,
        "golden {} differs from what prism produces now; if the change is intended, run with UPDATE_GOLDEN=1 and commit the result",
        path.display()
    );
}

fn lookups(name: &str, record_type: &str, records: serde_json::Value) -> Lookups {
    serde_json::from_value(serde_json::json!({
        "lookups": [{
            "query": { "name": name, "record_type": record_type },
            "name_server": "udp:192.0.2.53:53",
            "result": { "Response": {
                "records": records,
                "response_time": { "secs": 0, "nanos": 12_000_000 },
                "valid_until": "2030-01-01T00:00:00Z"
            }}
        }]
    }))
    .expect("lookups fixture must deserialize into mhost Lookups")
}

fn record(name: &str, record_type: &str, data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "name": name, "type": record_type, "ttl": 300, "data": data })
}

fn batch_event(label: &str, lookups: &Lookups, completed: u32) -> Event {
    let batch = BatchEvent {
        request_id: "contract-golden".to_string(),
        record_type: label.to_string(),
        lookups: lookups.clone(),
        completed,
        total: 19,
        transport: None,
        source: None,
    };
    let mut v = serde_json::to_value(&batch).unwrap();
    record_format::enrich_lookups_json(&mut v, label);
    Event::default().event("batch").json_data(&v).unwrap()
}

fn lint_event(category: &'static str, results: Vec<CheckResult>) -> Event {
    let lint = LintEvent {
        request_id: "contract-golden".to_string(),
        category,
        results,
    };
    Event::default().event("lint").json_data(&lint).unwrap()
}

#[tokio::test]
async fn prism_check_stream_matches_golden() {
    let a = lookups(
        "example.com.",
        "A",
        serde_json::json!([record(
            "example.com.",
            "A",
            serde_json::json!({ "A": "192.0.2.10" })
        )]),
    );
    let ns = lookups(
        "example.com.",
        "NS",
        serde_json::json!([
            record(
                "example.com.",
                "NS",
                serde_json::json!({ "NS": "ns1.example.com." })
            ),
            record(
                "example.com.",
                "NS",
                serde_json::json!({ "NS": "ns2.example.com." })
            ),
        ]),
    );
    let txt = lookups(
        "example.com.",
        "TXT",
        serde_json::json!([record(
            "example.com.",
            "TXT",
            serde_json::json!({ "TXT": { "txt_data": [b"v=spf1 -all".to_vec()] } })
        )]),
    );
    let all = a.clone().merge(ns.clone()).merge(txt.clone());

    let lints = [
        ("caa", check_caa(&all)),
        ("ns", check_ns_count(&all)),
        ("spf", check_spf(&all)),
    ];
    let (mut passed, mut warnings, mut failed, mut not_found, mut total) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    for (_, results) in &lints {
        for r in results {
            total += 1;
            match r {
                CheckResult::Ok(_) => passed += 1,
                CheckResult::Warning(_) => warnings += 1,
                CheckResult::Failed(_) => failed += 1,
                CheckResult::NotFound() => not_found += 1,
            }
        }
    }

    let mut events = vec![
        batch_event("A", &a, 1),
        batch_event("NS", &ns, 2),
        batch_event("TXT", &txt, 3),
    ];
    for (category, results) in lints {
        events.push(lint_event(category, results));
    }
    let done = CheckDoneEvent {
        request_id: "contract-golden".to_string(),
        duration_ms: 1,
        total_checks: total,
        passed,
        warnings,
        failed,
        not_found,
        cache_key: None,
    };
    events.push(Event::default().event("done").json_data(&done).unwrap());

    let resp = Sse::new(futures::stream::iter(
        events.into_iter().map(Ok::<_, Infallible>),
    ))
    .into_response();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();

    assert!(
        body.contains("event: lint"),
        "stream must carry lint events"
    );
    assert_golden("prism.sse", &body);
}
