//! The result page's open issues (V2 spec v2-result-page, requirements 7 and 8; criteria C1, C8, C9):
//! every warn and fail check once, grouped by owner, with the catalogue's texts.
use netray_engine::catalogue::catalogue;
use netray_model::evidence::{BlockId, EVIDENCE_SCHEMA_VERSION};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::components::Tone;
use netray_web::pages::result::views::{Grouping, Links, issues};
use std::collections::BTreeMap;

const ENTRY: &str = "Ab3dE5fG";

fn check(id: &str, status: Status, findings: &[&str], evidence: &[&str]) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: findings.iter().map(|s| s.to_string()).collect(),
        evidence: evidence
            .iter()
            .map(|b| BlockId::parse(b).unwrap())
            .collect(),
    }
}

fn section(protocol: Protocol) -> RunSection {
    RunSection {
        protocol,
        outcome: SectionKind::Measured,
        reason: None,
    }
}

/// Two warns and one fail on three owners, plus checks of every other status.
fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::C,
        complete: true,
        sections: vec![
            section(Protocol::Dns),
            section(Protocol::Tls),
            section(Protocol::Http),
        ],
        checks: vec![
            check("dns.dnssec", Status::Warn, &[], &[]),
            check(
                "http.hsts",
                Status::Warn,
                &["max-age is 3600 on example.com", "no includeSubDomains"],
                &["http.headers@192.0.2.1:443"],
            ),
            check(
                "tls.hostname_match",
                Status::Fail,
                &["netray.info is not covered"],
                &[],
            ),
            check("tls.chain_trusted", Status::Pass, &["chain verifies"], &[]),
            check("dns.dnskey_algorithm", Status::NotApplicable, &[], &[]),
            check("dns.dnssec_rollover", Status::NotTested, &[], &[]),
            check("tls.not_expired", Status::Unmeasured, &[], &[]),
        ],
        blocks: vec![],
    }
}

fn build() -> Vec<netray_web::pages::result::views::IssueGroup> {
    issues(
        &run(),
        catalogue(),
        Links::Entry(ENTRY),
        Grouping::Owner,
        &BTreeMap::new(),
    )
}

#[test]
fn issues_by_owner_hold_each_warn_and_fail_once_with_catalogue_texts() {
    let groups = build();

    let keys: Vec<&str> = groups.iter().map(|g| g.key.as_str()).collect();
    assert_eq!(keys, ["dns_provider", "certificate_provider", "web_server"]);
    let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "your DNS provider",
            "your certificate provider",
            "your web server configuration"
        ]
    );

    let rows: Vec<_> = groups.iter().flat_map(|g| g.rows.iter()).collect();
    let codes: Vec<&str> = rows.iter().map(|r| r.view.code.as_str()).collect();
    assert_eq!(codes, ["dns.dnssec", "tls.hostname_match", "http.hsts"]);

    // dns.dnssec: warn without findings -> catalogue finding
    let dnssec = rows[0];
    assert!(matches!(dnssec.view.tone, Tone::Warn));
    assert_eq!(dnssec.view.title, "DNSSEC signed");
    assert_eq!(
        dnssec.view.body,
        "The zone is not DNSSEC-signed, or its DNSSEC records are inconsistent, use weak algorithms or carry expired signatures."
    );
    assert_eq!(
        dnssec.view.fix.as_deref(),
        Some(
            "Enable DNSSEC signing at the DNS provider, have its DS record published at the registrar and keep the signatures current."
        )
    );
    assert_eq!(dnssec.view.guide_href.as_deref(), Some("/guide/dnssec"));
    assert_eq!(
        dnssec.view.evidence_href.as_deref(),
        Some("/r/Ab3dE5fG/dns")
    );
    assert_eq!(
        (dnssec.owner, dnssec.severity, dnssec.status),
        ("dns_provider", "high", "warn")
    );

    // tls.hostname_match: the fail, stored findings as body
    let host = rows[1];
    assert!(matches!(host.view.tone, Tone::Fail));
    assert_eq!(host.view.title, "Hostname match");
    assert_eq!(host.view.body, "netray.info is not covered");
    assert_eq!(
        host.view.fix.as_deref(),
        Some("Reissue the certificate with every hostname served on this server in its SAN list.")
    );
    assert_eq!(
        host.view.guide_href.as_deref(),
        Some("/guide/certificate-chain")
    );
    assert_eq!(host.view.evidence_href.as_deref(), Some("/r/Ab3dE5fG/tls"));
    assert_eq!(
        (host.owner, host.severity, host.status),
        ("certificate_provider", "critical", "fail")
    );

    // http.hsts: findings joined, evidence href points at the first reference
    let hsts = rows[2];
    assert!(matches!(hsts.view.tone, Tone::Warn));
    assert_eq!(hsts.view.title, "HSTS");
    assert_eq!(
        hsts.view.body,
        "max-age is 3600 on example.com no includeSubDomains"
    );
    assert_eq!(
        hsts.view.fix.as_deref(),
        Some("Send Strict-Transport-Security with max-age=31536000 on all HTTPS responses.")
    );
    assert_eq!(hsts.view.guide_href.as_deref(), Some("/guide/hsts"));
    assert_eq!(
        hsts.view.evidence_href.as_deref(),
        Some("/r/Ab3dE5fG/http#ev-http.headers@192.0.2.1:443")
    );
}

#[test]
fn issues_exclude_every_other_status() {
    let groups = build();
    let codes: Vec<&str> = groups
        .iter()
        .flat_map(|g| g.rows.iter())
        .map(|r| r.view.code.as_str())
        .collect();
    for absent in [
        "tls.chain_trusted",
        "dns.dnskey_algorithm",
        "dns.dnssec_rollover",
        "tls.not_expired",
    ] {
        assert!(
            !codes.contains(&absent),
            "{absent} must not be an issue: {codes:?}"
        );
    }
    assert_eq!(codes.len(), 3);
}

#[test]
fn row_body_is_the_findings_or_else_the_catalogue_finding() {
    let groups = build();
    let body = |code: &str| {
        groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .find(|r| r.view.code == code)
            .unwrap()
            .view
            .body
            .clone()
    };
    assert_eq!(
        body("http.hsts"),
        "max-age is 3600 on example.com no includeSubDomains"
    );
    assert_eq!(
        body("dns.dnssec"),
        catalogue()
            .get(&CheckId::parse("dns.dnssec").unwrap())
            .unwrap()
            .finding
    );
}
