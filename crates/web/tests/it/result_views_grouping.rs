//! Contract tests for the result page's issue grouping (spec v2-result-page, requirement 9; C2).
use std::collections::BTreeMap;

use netray_engine::catalogue::catalogue;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_web::pages::result::views::{Grouping, IssueGroup, Links, issues};

const OWNERS: [(&str, &str); 6] = [
    ("dns_provider", "Your DNS provider"),
    ("registrar", "Your registrar"),
    ("certificate_provider", "Your certificate provider"),
    ("web_server", "Your web server configuration"),
    ("mail_provider", "Your mail provider"),
    ("hosting", "Your hosting provider"),
];
const SEVERITIES: [&str; 4] = ["critical", "high", "medium", "low"];
const PROTOCOLS: [(&str, &str); 5] = [
    ("dns", "DNS"),
    ("tls", "TLS"),
    ("http", "HTTP"),
    ("email", "Email"),
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
    let key_of: fn(&(usize, String, String, String, bool)) -> &str = match grouping {
        Grouping::Owner => |r| r.1.as_str(),
        Grouping::Severity => |r| r.2.as_str(),
        Grouping::Protocol => |r| r.3.as_str(),
    };
    open.sort_by_key(|r| (!r.4, pos(&SEVERITIES, &r.2), r.0));
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for row in &open {
        let id = catalogue().entries()[row.0].id.to_string();
        match out.iter_mut().find(|(key, _)| key == key_of(row)) {
            Some((_, ids)) => ids.push(id),
            None => out.push((key_of(row).to_owned(), vec![id])),
        }
    }
    out
}

#[test]
fn groups_run_by_their_first_row_with_the_fixed_labels() {
    let order = |g: Grouping| -> Vec<String> { expected(g).into_iter().map(|(k, _)| k).collect() };

    let owner = grouped(Grouping::Owner);
    let got: Vec<(&str, &str)> = owner
        .iter()
        .map(|g| (g.key.as_str(), g.label.as_str()))
        .collect();
    let want: Vec<(&str, &str)> = order(Grouping::Owner)
        .iter()
        .map(|k| *OWNERS.iter().find(|(o, _)| o == k).unwrap())
        .collect();
    assert_eq!(got, want, "owner groups: first row order and labels");
    assert!(got.len() >= 3, "the catalogue spans several owners");

    let sev = grouped(Grouping::Severity);
    let got: Vec<&str> = sev.iter().map(|g| g.label.as_str()).collect();
    assert_eq!(
        got,
        order(Grouping::Severity),
        "severity groups by first row"
    );
    assert!(got.len() >= 3);

    let proto = grouped(Grouping::Protocol);
    let got: Vec<(&str, &str)> = proto
        .iter()
        .map(|g| (g.key.as_str(), g.label.as_str()))
        .collect();
    let want: Vec<(&str, &str)> = order(Grouping::Protocol)
        .iter()
        .map(|k| *PROTOCOLS.iter().find(|(p, _)| p == k).unwrap())
        .collect();
    assert_eq!(got, want, "protocol groups by first row");
    assert_eq!(got.len(), 5);
}

#[test]
fn rows_inside_a_group_follow_status_then_severity_then_catalogue_order() {
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
