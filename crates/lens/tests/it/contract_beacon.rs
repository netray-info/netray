//! Contract: lens's email section must reflect the SSE stream beacon actually sends (golden:
//! `tests/fixtures/contracts/beacon.sse`, produced by `crates/email/tests/it/contract_golden.rs`).
//! The email section is a module of the engine registry: `netray_email::testing::golden_module`
//! answers with the translation of the golden, and lens runs it through its `ModuleSection`.

use std::sync::Arc;
use std::time::Duration;

use crate::common;

use common::golden;
use lens::check::SectionError;
use lens::modules::ModuleSection;
use lens::modules::{Backend, BackendContext, BackendExtra, BackendResult};
use lens::score::CheckVerdict;
use netray_engine::{Module, Registry};
use netray_model::Protocol;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Run lens's email section over an email module.
async fn run_raw(module: Box<dyn Module>) -> Result<BackendResult, SectionError> {
    let section = ModuleSection {
        registry: Arc::new(
            Registry::new()
                .with(module)
                .with_facts(common::facts_with_ips(&[])),
        ),
        protocol: Protocol::Email,
        timeout: TIMEOUT,
        public_url: String::new(),
    };
    let ctx = BackendContext {
        dkim_selectors: None,
        forward_headers: reqwest::header::HeaderMap::new(),
    };
    section.run("example.com", &ctx).await
}

async fn run_backend(module: Box<dyn Module>) -> Result<BackendResult, String> {
    run_raw(module).await.map_err(|e| format!("{e:?}"))
}

/// The email module answering the golden `name` as it is.
async fn run_golden(name: &str) -> Result<BackendResult, String> {
    run_backend(netray_email::testing::golden_module(&golden(name))).await
}

/// The email module answering a rewritten golden stream.
async fn run_sse(sse: &str) -> Result<BackendResult, SectionError> {
    run_raw(netray_email::testing::golden_module(sse)).await
}

/// The email IDs that need a receiving mail server: not applicable without MX or with Null MX.
const MX_DEPENDENT: [&str; 7] = [
    "mx", "mta_sts", "tls_rpt", "bimi", "dane", "dnsbl", "fcrdns",
];

fn assert_reflects_golden(res: &BackendResult) {
    // golden: dkim=fail, dmarc=warn, spf=pass
    let dkim = check(res, "dkim");
    assert!(
        matches!(dkim.verdict, CheckVerdict::Fail),
        "dkim must be Fail (golden dkim=fail), got {:?}",
        dkim.verdict
    );
    let mta_sts = check(res, "mta_sts");
    assert!(
        matches!(mta_sts.verdict, CheckVerdict::Warn),
        "mta_sts must be Warn (golden mta_sts=warn), got {:?}",
        mta_sts.verdict
    );
    match &res.extra {
        BackendExtra::Email { grade, .. } => assert_eq!(grade.as_deref(), Some("D")),
        _ => panic!("expected BackendExtra::Email"),
    }
    // The reason lives in beacon's sub_checks[].detail, not in the category status line.
    assert!(
        dkim.messages
            .iter()
            .any(|m| m.contains("RSA 512 bits < 1024")),
        "dkim messages must carry the DKIM sub-check detail, got {:?}",
        dkim.messages
    );
    assert!(
        mta_sts.messages.iter().any(|m| m.contains("mode: testing")),
        "mta_sts messages must carry the MTA-STS sub-check detail, got {:?}",
        mta_sts.messages
    );
}

fn check<'a>(res: &'a BackendResult, name: &str) -> &'a lens::score::CheckResult {
    res.checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name} check"))
}

#[tokio::test]
async fn lens_parses_beacon_golden_verdicts() {
    let res = run_golden("beacon.sse").await.expect("backend result");
    assert_reflects_golden(&res);
}

#[tokio::test]
async fn lens_marks_mx_dependent_checks_na_for_beacon_no_mx() {
    let res = run_golden("beacon-no-mx.sse")
        .await
        .expect("backend result");
    for name in MX_DEPENDENT {
        let c = check(&res, name);
        assert!(
            matches!(c.verdict, CheckVerdict::Skip),
            "{name} must be N/A (Skip) for no_mx, got {:?}",
            c.verdict
        );
        assert_eq!(c.messages, ["no MX"], "{name} messages");
    }
}

#[tokio::test]
async fn lens_does_not_treat_mx_cname_failure_as_no_mx() {
    let res = run_golden("beacon-mx-cname.sse")
        .await
        .expect("backend result");
    // An mx_cname failure is not "no MX": no check is N/A for that reason. Checks whose
    // categories are Info (here mta_sts, tls_rpt, bimi, dane) are not applicable on their own.
    assert!(
        res.checks
            .iter()
            .all(|c| !c.messages.iter().any(|m| m == "no MX" || m == "Null MX")),
        "mx_cname is not 'no MX'; got {:?}",
        res.checks
    );
    let mx = check(&res, "mx");
    assert!(
        matches!(mx.verdict, CheckVerdict::Fail),
        "mx must reflect the mx_cname failure, got {:?}",
        mx.verdict
    );
    assert!(
        mx.messages.iter().any(|m| m.contains("points to a CNAME")),
        "mx messages must carry the mx_cname detail, got {:?}",
        mx.messages
    );
    let dnsbl = check(&res, "dnsbl");
    assert!(
        matches!(dnsbl.verdict, CheckVerdict::Fail),
        "dnsbl must reflect the listing, got {:?}",
        dnsbl.verdict
    );
    assert!(
        dnsbl
            .messages
            .iter()
            .any(|m| m.contains("listed in zen.spamhaus.org")),
        "dnsbl messages must carry the DNSBL listing, got {:?}",
        dnsbl.messages
    );
}

// The section sign of the mx_cname detail survives the module intact (the chunk-split half of
// this test has no in-process counterpart: there is no stream to split).
#[tokio::test]
async fn lens_keeps_non_ascii_detail_intact() {
    let res = run_golden("beacon-mx-cname.sse")
        .await
        .expect("backend result");
    let mx = check(&res, "mx");
    assert!(
        mx.messages
            .iter()
            .any(|m| m.contains("points to a CNAME (RFC 5321 \u{a7}5.1)")),
        "mx messages must carry the exact mx_cname detail, got {:?}",
        mx.messages
    );
    assert!(
        !mx.messages.iter().any(|m| m.contains('\u{FFFD}')),
        "no U+FFFD allowed in messages, got {:?}",
        mx.messages
    );
}

// ---------------------------------------------------------------------------
// specs/features/email-scoring, Phase 2 (requirements 7-10)
// ---------------------------------------------------------------------------

/// Rewrite the golden's `data:` events and re-emit the stream with its framing untouched:
/// the stream is split on the blank-line separators, only the JSON of each block changes,
/// and the final blank line stays. `f` may edit or remove events.
fn variant(name: &str, f: impl FnOnce(&mut Vec<serde_json::Value>)) -> String {
    let text = golden(name);
    assert!(
        text.ends_with("\n\n"),
        "{name}: golden must end with a blank line"
    );
    let mut blocks: Vec<&str> = text.split("\n\n").collect();
    assert_eq!(blocks.pop(), Some(""), "{name}: trailing separator");
    let mut events: Vec<serde_json::Value> = blocks
        .iter()
        .map(|b| {
            let json = b
                .strip_prefix("data: ")
                .unwrap_or_else(|| panic!("{name}: block without data: prefix: {b}"));
            serde_json::from_str(json).unwrap()
        })
        .collect();
    f(&mut events);
    let mut out = String::new();
    for e in &events {
        out.push_str("data: ");
        out.push_str(&serde_json::to_string(e).unwrap());
        out.push_str("\n\n");
    }
    out
}

fn category_mut<'a>(events: &'a mut [serde_json::Value], name: &str) -> &'a mut serde_json::Value {
    events
        .iter_mut()
        .find(|e| e["category"] == name)
        .unwrap_or_else(|| panic!("no category event {name}"))
}

fn summary_mut(events: &mut [serde_json::Value]) -> &mut serde_json::Value {
    events
        .iter_mut()
        .find(|e| e["type"] == "summary")
        .expect("summary event")
}

fn assert_na(res: &BackendResult, name: &str) {
    let c = check(res, name);
    assert!(
        matches!(c.verdict, CheckVerdict::Skip),
        "{name} must be N/A (Skip), got {:?}",
        c.verdict
    );
    assert!(
        !c.messages.is_empty(),
        "{name} must carry its N/A reason as message"
    );
}

// C2 / C6 parser part: beacon's own timeout summary is a Timeout, not N/A.
#[tokio::test]
async fn beacon_timeout_golden_is_section_timeout() {
    let err = run_sse(&golden("beacon-timeout.sse"))
        .await
        .expect_err("a skipped beacon run has no result");
    assert!(matches!(err, SectionError::Timeout), "got {err:?}");
}

// C7: a Null MX domain does not accept mail; spf and dmarc still score and pass.
#[tokio::test]
async fn beacon_null_mx_golden_marks_mx_dependent_checks_na_and_auth_passes() {
    let res = run_golden("beacon-null-mx.sse").await.expect("result");
    for name in MX_DEPENDENT {
        assert_na(&res, name);
        // The domain publishes `MX 0 .`: the message must say Null MX, not "no MX".
        let msgs = &check(&res, name).messages;
        assert!(
            msgs.iter().any(|m| m.contains("Null MX")) && msgs.iter().all(|m| m != "no MX"),
            "{name}: Null MX message expected, got {msgs:?}"
        );
    }
    for name in ["spf", "dmarc"] {
        let c = check(&res, name);
        assert!(
            matches!(c.verdict, CheckVerdict::Pass),
            "{name} must Pass for a Null MX domain, got {:?} {:?}",
            c.verdict,
            c.messages
        );
    }
}

// Requirement 9: Null MX makes the MX-dependent checks N/A for that reason, even when a
// receiving-side check fails (a stale MTA-STS record whose policy fetch fails).
#[tokio::test]
async fn null_mx_keeps_transport_na_when_a_receiving_check_fails() {
    let bytes = variant("beacon-null-mx.sse", |ev| {
        let sts = category_mut(ev, "mta_sts");
        sts["sub_checks"] = serde_json::json!([
            {"detail": "policy host not reachable", "name": "https_fetch_failed", "verdict": "fail"}
        ]);
        sts["verdict"] = "fail".into();
        summary_mut(ev)["verdicts"]["mta_sts"] = "fail".into();
    });
    let res = run_sse(&bytes)
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("result");
    assert_na(&res, "mta_sts");
    assert_eq!(
        check(&res, "mta_sts").messages,
        ["Null MX"],
        "mta_sts is N/A because of Null MX, not because of its failing policy fetch"
    );
}

// Requirement 10: with sends_no_mail, spf_mx_coverage does not reach spf.
#[tokio::test]
async fn sends_no_mail_excludes_spf_mx_coverage() {
    let bytes = variant("beacon-null-mx.sse", |ev| {
        let cv = category_mut(ev, "cross_validation");
        let subs = cv["sub_checks"].as_array_mut().expect("sub_checks");
        subs.push(serde_json::json!(
            {"detail": "MX host IPs are not covered by SPF (only relevant if these MX hosts also send outbound mail)", "name": "spf_mx_coverage", "verdict": "warn"}
        ));
    });
    let res = run_sse(&bytes)
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("result");
    let spf = check(&res, "spf");
    assert!(
        matches!(spf.verdict, CheckVerdict::Pass),
        "spf_mx_coverage must not count for a domain that sends no mail, got {:?} {:?}",
        spf.verdict,
        spf.messages
    );
}

// C8: no MX records.
#[tokio::test]
async fn beacon_no_mx_golden_marks_mx_dependent_checks_na() {
    let res = run_golden("beacon-no-mx.sse").await.expect("result");
    for name in MX_DEPENDENT {
        assert_na(&res, name);
    }
}

// C9 parser part: a BIMI-less domain has nothing to score for BIMI.
#[tokio::test]
async fn beacon_golden_bimi_is_not_applicable() {
    let res = run_golden("beacon.sse").await.expect("result");
    let bimi = check(&res, "bimi");
    assert!(
        matches!(bimi.verdict, CheckVerdict::Skip),
        "bimi must be Skip without BIMI, got {:?}",
        bimi.verdict
    );
    assert_eq!(
        bimi.messages,
        ["No BIMI record"],
        "beacon's info message is the reason"
    );
}

// C11: a category that did not complete makes the section Errored.
#[tokio::test]
async fn beacon_partial_golden_errors_the_section() {
    let out = run_sse(&golden("beacon-partial.sse")).await;
    assert!(out.is_err(), "a skipped category must Error the section");
    assert!(
        !matches!(
            out,
            Err(SectionError::Timeout) | Err(SectionError::NotApplicable { .. })
        ),
        "got {out:?}"
    );
}

// C13: a sender without DKIM but with p=reject gets the cross-validation warning on dkim.
#[tokio::test]
async fn beacon_sending_no_dkim_golden_warns_authentication() {
    let res = run_golden("beacon-sending-no-dkim.sse")
        .await
        .expect("result");
    let dkim = check(&res, "dkim");
    assert!(
        matches!(dkim.verdict, CheckVerdict::Warn),
        "dkim must Warn, got {:?}",
        dkim.verdict
    );
    assert!(
        dkim.messages
            .iter()
            .any(|m| m.contains("DMARC p=reject and SPF -all but no DKIM keys found")),
        "dkim messages must carry the reject_no_dkim detail, got {:?}",
        dkim.messages
    );
}

// C10: a BIMI/DMARC cross-validation warning is routed to bimi.
#[tokio::test]
async fn bimi_dmarc_policy_cross_check_warns_bimi() {
    let bytes = variant("beacon.sse", |ev| {
        let cv = category_mut(ev, "cross_validation");
        cv["sub_checks"] = serde_json::json!([
            {"detail": "BIMI requires DMARC p=quarantine or reject", "name": "bimi_dmarc_policy", "verdict": "warn"}
        ]);
        cv["verdict"] = "warn".into();
        summary_mut(ev)["verdicts"]["cross_validation"] = "warn".into();
    });
    let res = run_sse(&bytes)
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("result");
    let bimi = check(&res, "bimi");
    assert!(
        matches!(bimi.verdict, CheckVerdict::Warn),
        "bimi must Warn, got {:?} {:?}",
        bimi.verdict,
        bimi.messages
    );
}

// C12: a summary verdict without its category event is a broken stream.
#[tokio::test]
async fn missing_category_event_errors_the_section() {
    let bytes = variant("beacon.sse", |ev| {
        ev.retain(|e| e["category"] != "dnsbl");
    });
    let out = run_sse(&bytes).await;
    assert!(
        out.is_err(),
        "a missing category event must Error, got {out:?}"
    );
}

// C15: an unknown cross-validation sub-check is a contract break: the section errors. (The
// `lens_unknown_verdict_total{section="email"}` count is not asserted: the translation now lives
// in `netray_email`, outside lens's metrics, as for HTTP.)
#[tokio::test]
async fn unknown_cross_validation_sub_check_errors_the_section() {
    let bytes = variant("beacon.sse", |ev| {
        let cv = category_mut(ev, "cross_validation");
        cv["sub_checks"] =
            serde_json::json!([{"detail": "new", "name": "brand_new_rule", "verdict": "warn"}]);
        cv["verdict"] = "warn".into();
        summary_mut(ev)["verdicts"]["cross_validation"] = "warn".into();
    });
    let out = run_sse(&bytes).await;
    assert!(
        out.is_err(),
        "an unrouted sub-check must Error, got {out:?}"
    );
}
