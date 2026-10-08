//! Contract golden: the SSE bytes beacon puts on the wire, consumed by
//! `crates/lens/tests/contract_beacon.rs`.
//!
//! The events go through beacon's real encoding path (`From<SseEvent> for sse::Event`,
//! then axum's `Sse` response body), so framing is exactly what a client receives.
//! Only JSON key order is canonicalised (`verdicts` is a `HashMap`, whose order is random).
//!
//! `UPDATE_GOLDEN=1 cargo test -p beacon --test contract_golden` rewrites the file.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;

use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use beacon::quality::{
    Category, CheckResult, SseEvent, SubCheck, Verdict, compute_grade,
};
use http_body_util::BodyExt;
use serde_json::{Map, Value};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/contracts/beacon.sse")
}

fn sub(name: &str, verdict: Verdict, detail: &str) -> SubCheck {
    SubCheck {
        name: name.to_string(),
        verdict,
        detail: detail.to_string(),
    }
}

fn canonical(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<String> = m.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                let child = canonical(m[&k].clone());
                out.insert(k, child);
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(canonical).collect()),
        other => other,
    }
}

/// Re-encode each `data:` line with sorted keys; all other framing is left untouched.
fn canonicalise_wire(wire: &str) -> String {
    wire.split_inclusive('\n')
        .map(|line| match line.strip_prefix("data: ") {
            Some(rest) => {
                let v: Value = serde_json::from_str(rest.trim_end()).expect("data is JSON");
                format!("data: {}\n", serde_json::to_string(&canonical(v)).unwrap())
            }
            None => line.to_string(),
        })
        .collect()
}

async fn produce() -> String {
    // Every category run_all_checks reports, keyed like its verdicts_map (serde name of Category).
    let table = [
        (Category::Mx, Verdict::Pass, "mx_present"),
        (Category::Spf, Verdict::Pass, "spf_record"),
        (Category::Dkim, Verdict::Fail, "dkim_selector"),
        (Category::Dmarc, Verdict::Pass, "dmarc_policy"),
        (Category::MtaSts, Verdict::Warn, "mta_sts_policy"),
        (Category::TlsRpt, Verdict::Warn, "tls_rpt_record"),
        (Category::Dane, Verdict::Pass, "dane_tlsa"),
        (Category::Dnssec, Verdict::Pass, "dnssec_chain"),
        (Category::Bimi, Verdict::Pass, "bimi_record"),
        (Category::Fcrdns, Verdict::Pass, "fcrdns_match"),
        (Category::Dnsbl, Verdict::Pass, "dnsbl_clean"),
        (Category::CrossValidation, Verdict::Pass, "cross_checks"),
    ];
    let results: Vec<CheckResult> = table
        .iter()
        .map(|(cat, v, name)| {
            CheckResult::new(
                cat.clone(),
                vec![sub(name, *v, "example.com")],
                format!("{name} detail"),
            )
        })
        .collect();

    let verdict_list: Vec<Verdict> = results.iter().map(|r| r.verdict).collect();
    let verdicts: HashMap<String, Verdict> = results
        .iter()
        .map(|r| {
            let key = serde_json::to_value(&r.category)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            (key, r.verdict)
        })
        .collect();

    let mut events: Vec<SseEvent> = results.into_iter().map(SseEvent::Category).collect();
    events.push(SseEvent::Summary {
        grade: compute_grade(&verdict_list),
        verdicts,
        duration_ms: 1234,
    });

    let stream = futures::stream::iter(events.into_iter().map(|e| {
        let ev: Event = e.into();
        Ok::<_, Infallible>(ev)
    }));
    let body = Sse::new(stream).into_response().into_body();
    let bytes = body.collect().await.expect("body").to_bytes();
    canonicalise_wire(std::str::from_utf8(&bytes).expect("utf-8"))
}

#[tokio::test]
async fn sse_wire_format_matches_golden() {
    let produced = produce().await;
    let path = golden_path();

    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &produced).unwrap();
        return;
    }

    let golden = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "golden {} is missing; run `UPDATE_GOLDEN=1 cargo test -p beacon --test contract_golden`",
            path.display()
        )
    });
    assert_eq!(
        golden, produced,
        "beacon's SSE output differs from {}; if the change is intended, regenerate with \
         `UPDATE_GOLDEN=1 cargo test -p beacon --test contract_golden` and check lens still parses it",
        path.display()
    );
}
