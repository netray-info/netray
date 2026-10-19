//! Contract tests for the result page's issue grouping (spec v2-result-page, requirement 9; C2).
use std::collections::BTreeMap;

use netray_engine::catalogue::catalogue;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_web::pages::result::views::{Grouping, IssueGroup, Links, issues};

const OWNERS: [(&str, &str); 6] = [
    ("dns_provider", "your DNS provider"),
    ("registrar", "your registrar"),
    ("certificate_provider", "your certificate provider"),
    ("web_server", "your web server configuration"),
    ("mail_provider", "your mail provider"),
    ("hosting", "your hosting provider"),
];
const SEVERITIES: [&str; 4] = ["critical", "high", "medium", "low"];
const PROTOCOLS: [(&str, &str); 5] = [
    ("dns", "DNS"),
    ("tls", "TLS"),
    ("http", "HTTP"),
    ("email", "email"),
    ("ip", "IP"),
];

fn word<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn pos(list: &[&str], key: &str) -> usize {
    list.iter().position(|k| *k == key).unwrap()
}

/// Every catalogue check, in catalogue order: pass, fail, warn cycling by index, so the last
/// check (the one IP check) is open and every protocol has a group.
fn status_at(i: usize) -> Status {
    match i % 3 {
        0 => Status::Pass,
        1 => Status::Fail,
        _ => Status::Warn,
    }
}

fn run() -> Run {
    let protocols = [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ];
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: protocols
            .into_iter()
            .map(|protocol| RunSection {
                protocol,
                outcome: SectionKind::Measured,
                reason: None,
            })
            .collect(),
        // Reversed, so the input order is not the catalogue's.
        checks: catalogue()
            .entries()
            .iter()
            .enumerate()
            .rev()
            .map(|(i, c)| RunCheck {
                id: c.id.clone(),
                status: status_at(i),
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks: vec![],
    }
}

fn grouped(grouping: Grouping) -> Vec<IssueGroup> {
    issues(&run(), catalogue(), Links::Live, grouping, &BTreeMap::new())
}

/// The expected keys and the rows' check ids, derived from the catalogue with explicit rank tables.
fn expected(grouping: Grouping) -> Vec<(String, Vec<String>)> {
    let owners: Vec<&str> = OWNERS.iter().map(|o| o.0).collect();
    let protocols: Vec<&str> = PROTOCOLS.iter().map(|p| p.0).collect();
    let mut open: Vec<(usize, String, String, String, bool)> = Vec::new();
    for (i, c) in catalogue().entries().iter().enumerate() {
        if status_at(i) == Status::Pass {
            continue;
        }
        open.push((
            i,
            word(&c.fix_owner),
            word(&c.severity),
            c.protocol.as_str().to_owned(),
            status_at(i) == Status::Fail,
        ));
    }
    let (keys, key_of): (
        Vec<&str>,
        fn(&(usize, String, String, String, bool)) -> &str,
    ) = match grouping {
        Grouping::Owner => (owners, |r| r.1.as_str()),
        Grouping::Severity => (SEVERITIES.to_vec(), |r| r.2.as_str()),
        Grouping::Protocol => (protocols, |r| r.3.as_str()),
    };
    let mut out = Vec::new();
    for key in keys {
        let mut rows: Vec<_> = open.iter().filter(|r| key_of(r) == key).collect();
        rows.sort_by_key(|r| (pos(&SEVERITIES, &r.2), !r.4, r.0));
        if !rows.is_empty() {
            out.push((
                key.to_owned(),
                rows.iter()
                    .map(|r| catalogue().entries()[r.0].id.to_string())
                    .collect(),
            ));
        }
    }
    out
}

#[test]
fn groups_run_in_the_fixed_order_with_the_fixed_labels() {
    let owner = grouped(Grouping::Owner);
    let labels: Vec<(&str, &str)> = OWNERS.to_vec();
    let got: Vec<(&str, &str)> = owner
        .iter()
        .map(|g| (g.key.as_str(), g.label.as_str()))
        .collect();
    let want: Vec<(&str, &str)> = labels
        .into_iter()
        .filter(|(k, _)| got.iter().any(|(g, _)| g == k))
        .collect();
    assert_eq!(got, want, "owner groups: fixed order and labels");
    assert!(got.len() >= 3, "the catalogue spans several owners");

    let sev = grouped(Grouping::Severity);
    let got: Vec<&str> = sev.iter().map(|g| g.label.as_str()).collect();
    let want: Vec<&str> = SEVERITIES
        .iter()
        .copied()
        .filter(|s| got.contains(s))
        .collect();
    assert_eq!(got, want, "severity groups run critical to low");
    assert!(got.len() >= 3);

    let proto = grouped(Grouping::Protocol);
    let got: Vec<(&str, &str)> = proto
        .iter()
        .map(|g| (g.key.as_str(), g.label.as_str()))
        .collect();
    let want: Vec<(&str, &str)> = PROTOCOLS
        .iter()
        .copied()
        .filter(|(k, _)| got.iter().any(|(g, _)| g == k))
        .collect();
    assert_eq!(got, want, "protocol groups: dns, tls, http, email, ip");
    assert_eq!(got.len(), 5);
}

#[test]
fn rows_inside_a_group_follow_severity_then_status_then_catalogue_order() {
    for grouping in [Grouping::Owner, Grouping::Severity, Grouping::Protocol] {
        let got: Vec<(String, Vec<String>)> = grouped(grouping)
            .iter()
            .map(|g| {
                (
                    g.key.clone(),
                    g.rows.iter().map(|r| r.view.code.to_string()).collect(),
                )
            })
            .collect();
        assert_eq!(got, expected(grouping), "{grouping:?}");
    }
}

#[test]
fn every_row_carries_the_catalogue_attributes_and_pass_checks_are_absent() {
    for grouping in [Grouping::Owner, Grouping::Severity, Grouping::Protocol] {
        let mut seen = 0;
        for row in grouped(grouping).iter().flat_map(|g| g.rows.iter()) {
            let (i, spec) = catalogue()
                .entries()
                .iter()
                .enumerate()
                .find(|(_, c)| c.id.to_string() == row.view.code.to_string())
                .unwrap();
            assert_eq!(row.owner, word(&spec.fix_owner));
            assert_eq!(row.severity, word(&spec.severity));
            assert_eq!(row.protocol, spec.protocol.as_str());
            assert_eq!(row.order, i);
            let status = if status_at(i) == Status::Fail {
                "fail"
            } else {
                "warn"
            };
            assert_eq!(row.status, status);
            seen += 1;
        }
        let open = (0..catalogue().entries().len())
            .filter(|i| status_at(*i) != Status::Pass)
            .count();
        assert_eq!(seen, open, "{grouping:?}: each open check exactly once");
    }
}
