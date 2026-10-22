// V2 history, Phase 5, requirement 18: the email module declares the address-keyed blocks
// `email.mx_addresses`, `email.fcrdns` and `email.dnsbl` volatile as whole blocks, so rotating
// addresses neither change the fingerprint nor show in the evidence diff, while `email.mx` and
// every check status stay compared.

use netray_email::{EmailModule, ModuleConfig};
use netray_engine::Module;
use netray_model::diff::{ChangeClass, diff};
use netray_model::evidence::{Block, BlockId, Shape, Value as Ev};
use netray_model::run::{Run, RunCheck, fingerprint};
use netray_model::{CheckId, Grade, Status};

async fn paths() -> Vec<&'static str> {
    let cfg: ModuleConfig = toml::from_str("[dkim]\nmax_user_selectors = 3").unwrap();
    let module = EmailModule::new(cfg, None).await.expect("module builds");
    module.volatile().iter().map(|p| p.0).collect()
}

fn text(s: &str) -> Ev {
    Ev::Text(s.into())
}

fn table(id: &str, key: &str, columns: &[&str], rows: Vec<Vec<Ev>>) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: id.into(),
        shape: Shape::Table {
            key: key.into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
        },
    }
}

fn mx(hosts: [&str; 2]) -> Block {
    table(
        "email.mx",
        "host",
        &["host", "preference"],
        vec![
            vec![text(hosts[0]), Ev::Int(10)],
            vec![text(hosts[1]), Ev::Int(20)],
        ],
    )
}

/// The three address-keyed blocks for two MX hosts answering `a1` and `a2`.
fn address_blocks(a1: &str, a2: &str) -> Vec<Block> {
    let zone = "bl.example.com";
    vec![
        table(
            "email.mx_addresses",
            "address",
            &["address", "host"],
            vec![
                vec![Ev::Addr(a1.parse().unwrap()), text("mx1.example.com")],
                vec![Ev::Addr(a2.parse().unwrap()), text("mx2.example.com")],
            ],
        ),
        table(
            "email.fcrdns",
            "address",
            &["address", "ptr", "confirmed"],
            vec![
                vec![
                    Ev::Addr(a1.parse().unwrap()),
                    text("mx1.example.com"),
                    Ev::Bool(true),
                ],
                vec![
                    Ev::Addr(a2.parse().unwrap()),
                    text("mx2.example.com"),
                    Ev::Bool(true),
                ],
            ],
        ),
        table(
            "email.dnsbl",
            "query",
            &["query", "zone", "target", "answered", "answer"],
            vec![
                vec![
                    text(&format!("{a1}@{zone}")),
                    text(zone),
                    text(a1),
                    Ev::Bool(true),
                    text("not listed"),
                ],
                vec![
                    text(&format!("{a2}@{zone}")),
                    text(zone),
                    text(a2),
                    Ev::Bool(true),
                    text("not listed"),
                ],
            ],
        ),
    ]
}

fn run(hosts: [&str; 2], addrs: [&str; 2], fcrdns: Status) -> Run {
    let mut blocks = vec![mx(hosts)];
    blocks.extend(address_blocks(addrs[0], addrs[1]));
    Run {
        profile_version: 1,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: vec![],
        checks: vec![
            RunCheck {
                id: CheckId::parse("email.mx").unwrap(),
                status: Status::Pass,
                findings: vec![],
                evidence: vec![],
            },
            RunCheck {
                id: CheckId::parse("email.fcrdns").unwrap(),
                status: fcrdns,
                findings: vec![],
                evidence: vec![],
            },
        ],
        blocks,
    }
}

const HOSTS: [&str; 2] = ["mx1.example.com", "mx2.example.com"];

#[tokio::test]
async fn volatile_list_covers_the_address_blocks_and_not_email_mx() {
    let p = paths().await;
    for want in ["email.mx_addresses/*", "email.fcrdns/*", "email.dnsbl/*"] {
        assert!(p.contains(&want), "{want} missing from {p:?}");
    }
    for path in &p {
        let block = path.split('/').next().unwrap();
        assert_ne!(block, "email.mx", "email.mx must stay compared: {path}");
    }
}

#[tokio::test]
async fn rotating_addresses_change_neither_fingerprint_nor_evidence_diff() {
    let p = paths().await;
    let a = run(HOSTS, ["192.0.2.10", "192.0.2.20"], Status::Pass);
    let b = run(HOSTS, ["198.51.100.10", "198.51.100.20"], Status::Pass);
    assert_eq!(fingerprint(&a, &p), fingerprint(&b, &p));
    let d = diff(&a, &b, &p, None);
    assert!(d.evidence.is_empty(), "evidence changes: {:?}", d.evidence);
}

#[tokio::test]
async fn different_mx_hosts_change_the_fingerprint() {
    let p = paths().await;
    let a = run(
        ["mx1.example.com", "mx2.example.com"],
        ["192.0.2.10", "192.0.2.20"],
        Status::Pass,
    );
    let b = run(
        ["mx3.example.com", "mx4.example.com"],
        ["192.0.2.10", "192.0.2.20"],
        Status::Pass,
    );
    assert_ne!(fingerprint(&a, &p), fingerprint(&b, &p));
}

#[tokio::test]
async fn fcrdns_status_transition_shows_in_the_diff() {
    let p = paths().await;
    let a = run(HOSTS, ["192.0.2.10", "192.0.2.20"], Status::Pass);
    let b = run(HOSTS, ["192.0.2.10", "192.0.2.20"], Status::Fail);
    let d = diff(&a, &b, &p, None);
    let c = d
        .checks
        .iter()
        .find(|c| c.id.to_string() == "email.fcrdns")
        .expect("email.fcrdns change reported");
    assert_eq!(c.from, Some(Status::Pass));
    assert_eq!(c.to, Some(Status::Fail));
    assert_eq!(c.class, ChangeClass::New);
}
