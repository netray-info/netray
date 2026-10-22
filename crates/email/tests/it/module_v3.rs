// V2 Phase 3, requirement 3(c), criterion C4: the email module reports beacon's ten categories
// as the ten `email.*` catalogue IDs, routes cross-validation sub-checks to the check that owns
// the topic, and marks the MX-dependent checks not applicable for a domain without mail.

use std::collections::BTreeSet;
use std::path::PathBuf;

use netray_email::quality::types::SseEvent;
use netray_email::{EmailModule, ModuleConfig, translate};
use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use serde_json::{Value, json};

const IDS: [&str; 10] = [
    "email.mx",
    "email.spf",
    "email.dkim",
    "email.dmarc",
    "email.mta_sts",
    "email.tls_rpt",
    "email.bimi",
    "email.dane",
    "email.dnsbl",
    "email.fcrdns",
];

const MX_DEPENDENT: [&str; 7] = [
    "email.mx",
    "email.mta_sts",
    "email.tls_rpt",
    "email.bimi",
    "email.dane",
    "email.dnsbl",
    "email.fcrdns",
];

fn sse(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn data_lines(sse: &str) -> Vec<Value> {
    sse.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| serde_json::from_str(d.trim()).unwrap())
        .collect()
}

fn events(lines: &[Value]) -> Vec<SseEvent> {
    lines
        .iter()
        .map(|v| serde_json::from_value(v.clone()).expect("SseEvent"))
        .collect()
}

fn checks_of(lines: &[Value]) -> Vec<CheckResult> {
    match translate(&events(lines)) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn fixture_checks(name: &str) -> Vec<CheckResult> {
    checks_of(&data_lines(&sse(name)))
}

fn get<'a>(checks: &'a [CheckResult], id: &str) -> &'a CheckResult {
    checks
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("no CheckResult {id}"))
}

fn ids(checks: &[CheckResult]) -> Vec<String> {
    checks.iter().map(|c| c.id.to_string()).collect()
}

/// Replaces the cross-validation category's sub-checks (and its verdict in the summary).
fn with_cross_validation(mut lines: Vec<Value>, subs: &[(&str, &str, &str)]) -> Vec<Value> {
    let sub_checks: Vec<Value> = subs
        .iter()
        .map(|(name, verdict, detail)| json!({"name": name, "verdict": verdict, "detail": detail}))
        .collect();
    for line in &mut lines {
        match line["type"].as_str() {
            Some("category") if line["category"] == "cross_validation" => {
                line["sub_checks"] = Value::Array(sub_checks.clone());
                line["verdict"] = json!("warn");
            }
            Some("summary") => line["verdicts"]["cross_validation"] = json!("warn"),
            _ => {}
        }
    }
    lines
}

fn assert_ten_once(checks: &[CheckResult], what: &str) {
    let got = ids(checks);
    let unique: BTreeSet<&String> = got.iter().collect();
    assert_eq!(got.len(), 10, "{what}: {got:?}");
    assert_eq!(unique.len(), 10, "{what}: an ID twice: {got:?}");
    let want: BTreeSet<String> = IDS.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        unique.into_iter().cloned().collect::<BTreeSet<_>>(),
        want,
        "{what}"
    );
}

#[test]
fn golden_yields_the_ten_email_catalogue_ids_once_each() {
    for name in ["beacon.sse", "beacon-null-mx.sse", "beacon-no-mx.sse"] {
        assert_ten_once(&fixture_checks(name), name);
    }
}

#[tokio::test]
async fn module_checks_are_the_ten_email_catalogue_ids() {
    let cfg: ModuleConfig = toml::from_str("[dkim]\nmax_user_selectors = 3").unwrap();
    let module = EmailModule::new(cfg, None).await.expect("module builds");
    use netray_engine::Module;
    let got: Vec<String> = module.checks().iter().map(|c| c.to_string()).collect();
    let unique: BTreeSet<&String> = got.iter().collect();
    assert_eq!(got.len(), 10, "{got:?}");
    assert_eq!(unique.len(), 10, "{got:?}");
    let want: BTreeSet<String> = IDS.iter().map(|s| s.to_string()).collect();
    assert_eq!(unique.into_iter().cloned().collect::<BTreeSet<_>>(), want);
}

#[test]
fn one_id_per_beacon_category_carries_that_categorys_verdict() {
    let checks = fixture_checks("beacon.sse");
    for (id, status) in [
        ("email.mx", Status::Pass),
        ("email.spf", Status::Pass),
        ("email.dkim", Status::Fail),
        ("email.dmarc", Status::Warn),
        ("email.mta_sts", Status::Warn),
        ("email.tls_rpt", Status::Pass),
    ] {
        assert_eq!(get(&checks, id).status, status, "{id}");
    }
}

#[test]
fn cross_validation_findings_sit_on_the_check_that_owns_the_topic() {
    let lines = with_cross_validation(
        data_lines(&sse("beacon.sse")),
        &[
            ("dane_without_tls_rpt", "warn", "XV-DANE-NO-TLSRPT"),
            ("mta_sts_without_tls_rpt", "warn", "XV-MTASTS-NO-TLSRPT"),
            ("dane_without_dnssec", "warn", "XV-DANE-NO-DNSSEC"),
        ],
    );
    let checks = checks_of(&lines);
    assert_ten_once(&checks, "variant");

    let tls_rpt = get(&checks, "email.tls_rpt");
    assert!(
        tls_rpt.findings.iter().any(|f| f == "XV-DANE-NO-TLSRPT"),
        "{:?}",
        tls_rpt.findings
    );
    assert!(
        tls_rpt.findings.iter().any(|f| f == "XV-MTASTS-NO-TLSRPT"),
        "{:?}",
        tls_rpt.findings
    );
    assert_ne!(
        tls_rpt.status,
        Status::Pass,
        "a warn finding lifts tls_rpt off pass"
    );

    let dane = get(&checks, "email.dane");
    assert!(
        dane.findings.iter().any(|f| f == "XV-DANE-NO-DNSSEC"),
        "{:?}",
        dane.findings
    );

    for c in &checks {
        let id = c.id.to_string();
        for (needle, owner) in [
            ("XV-DANE-NO-TLSRPT", "email.tls_rpt"),
            ("XV-MTASTS-NO-TLSRPT", "email.tls_rpt"),
            ("XV-DANE-NO-DNSSEC", "email.dane"),
        ] {
            if id != owner {
                assert!(
                    !c.findings.iter().any(|f| f == needle),
                    "{needle} leaked to {id}"
                );
            }
        }
    }
}

#[test]
fn no_mail_domains_make_the_seven_mx_dependent_ids_not_applicable() {
    for (name, reason) in [
        ("beacon-null-mx.sse", "Null MX"),
        ("beacon-no-mx.sse", "no MX"),
    ] {
        let checks = fixture_checks(name);
        assert_ten_once(&checks, name);
        for id in MX_DEPENDENT {
            let c = get(&checks, id);
            assert_eq!(c.status, Status::NotApplicable, "{name}: {id}");
            assert_eq!(c.findings, vec![reason.to_string()], "{name}: {id}");
        }
        for id in ["email.spf", "email.dmarc"] {
            let c = get(&checks, id);
            assert!(
                matches!(c.status, Status::Pass | Status::Warn | Status::Fail),
                "{name}: {id} is measured, got {:?}",
                c.status
            );
        }
    }
    let nomx = fixture_checks("beacon-no-mx.sse");
    assert_eq!(get(&nomx, "email.spf").status, Status::Fail);
    assert_eq!(get(&nomx, "email.dmarc").status, Status::Fail);
}

#[test]
fn dkim_info_is_not_applicable_with_beacons_message() {
    let checks = fixture_checks("beacon-no-mx.sse");
    let dkim = get(&checks, "email.dkim");
    assert_eq!(dkim.status, Status::NotApplicable);
    assert_eq!(dkim.findings, vec!["No DKIM keys found".to_string()]);
}

#[test]
fn sends_no_mail_stays_on_spf_and_suppresses_its_two_siblings() {
    let lines = with_cross_validation(
        data_lines(&sse("beacon-null-mx.sse")),
        &[
            ("sends_no_mail", "info", "XV-SENDS-NO-MAIL"),
            ("reject_no_dkim", "warn", "XV-REJECT-NO-DKIM"),
            ("spf_mx_coverage", "warn", "XV-SPF-MX-COVERAGE"),
        ],
    );
    let checks = checks_of(&lines);
    assert_ten_once(&checks, "sends_no_mail variant");

    let spf = get(&checks, "email.spf");
    assert!(
        spf.findings.iter().any(|f| f == "XV-SENDS-NO-MAIL"),
        "{:?}",
        spf.findings
    );

    for c in &checks {
        for hidden in ["XV-REJECT-NO-DKIM", "XV-SPF-MX-COVERAGE"] {
            assert!(
                !c.findings.iter().any(|f| f == hidden),
                "{hidden} must not appear, found on {}",
                c.id
            );
        }
    }
}
