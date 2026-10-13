// v2-unmeasured, requirements 15 and 16 (contract status table), criteria C2, C17, C18 for the
// IP section.
//
// One row per (check id, worst measured status). `ip.reputation` has no unmeasured row: its
// lists are in memory and have no error path, so the guard names it as the one exception.
// C17: a domain with only private addresses has no IP section ("no public addresses"); this
// protects today's behaviour.
//
// Cargo needs on netray-ip: `[[test]] name = "contract_status"`, `path = "tests/contract_status.rs"`,
// `required-features = ["testing"]` (it runs with `--features testing`).

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_ip::{IpModule, ModuleConfig};
use netray_model::{CheckResult, Status};

/// Catalogue ids this table deliberately has no unmeasured row for, with the reason.
const NO_UNMEASURED_ROW: &[(&str, &str)] = &[(
    "ip.reputation",
    "in-memory lists, no error path (crates/ip/src/module.rs:80-83)",
)];

const LISTED: &str = "8.8.8.8";

#[derive(Clone, Copy)]
enum Kind {
    /// The worst measured status: a listed address.
    Worst,
}

struct Row {
    id: &'static str,
    kind: Kind,
    status: Status,
}

fn rows() -> Vec<Row> {
    vec![Row {
        id: "ip.reputation",
        kind: Kind::Worst,
        status: Status::Fail,
    }]
}

fn ctx() -> RunContext {
    RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    }
}

fn catalogue_ids(prefix: &str) -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/catalogue/checks.toml");
    let doc: toml::Table = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["check"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .filter(|id| id.starts_with(prefix))
        .collect()
}

fn check<'a>(checks: &'a [CheckResult], id: &str) -> Option<&'a CheckResult> {
    checks.iter().find(|c| c.id.to_string() == id)
}

async fn module_with_listed_address() -> (IpModule, PathBuf) {
    let dir = std::env::temp_dir().join(format!("netray-ip-contract-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let list = dir.join("feodo.txt");
    std::fs::write(&list, format!("# feodo\n{LISTED}\n")).unwrap();
    let cfg: ModuleConfig = toml::from_str(&format!("feodo_botnet_ips = {:?}", list.display().to_string())).unwrap();
    (IpModule::new(cfg).await.expect("module builds"), dir)
}

#[tokio::test]
async fn contract_status_table() {
    let (module, dir) = module_with_listed_address().await;
    let facts = Facts {
        a: vec![LISTED.parse::<Ipv4Addr>().unwrap()],
        ..Facts::default()
    };
    let outcome = module.run(&ctx(), &facts).await;
    std::fs::remove_dir_all(&dir).ok();

    let SectionOutcome::Measured { checks, .. } = outcome else {
        panic!("expected Measured, got {outcome:?}");
    };
    let mut problems = Vec::new();
    for row in rows() {
        let Kind::Worst = row.kind;
        match check(&checks, row.id) {
            None => problems.push(format!("{}: no such check in the section", row.id)),
            Some(c) if c.status != row.status => problems.push(format!(
                "{}: expected {:?}, got {:?} {:?}",
                row.id, row.status, c.status, c.findings
            )),
            Some(_) => {}
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// C17: only private addresses, so no IP section.
#[tokio::test]
async fn only_private_addresses_is_not_applicable_no_public_addresses() {
    let (module, dir) = module_with_listed_address().await;
    let facts = Facts {
        a: vec!["10.0.0.1".parse::<Ipv4Addr>().unwrap()],
        aaaa: vec!["fd00::1".parse::<Ipv6Addr>().unwrap()],
        ..Facts::default()
    };
    let outcome = module.run(&ctx(), &facts).await;
    std::fs::remove_dir_all(&dir).ok();
    match outcome {
        SectionOutcome::NotApplicable { reason } => assert_eq!(reason, "no public addresses"),
        other => panic!("expected NotApplicable, got {other:?}"),
    }
}

/// Guard: the table and its exceptions cover exactly the catalogue's `ip.*` ids.
#[test]
fn contract_table_covers_catalogue() {
    let catalogue = catalogue_ids("ip.");
    let worst: BTreeSet<String> = rows().iter().map(|r| r.id.to_string()).collect();
    assert_eq!(worst, catalogue, "worst-status rows vs catalogue ip.* ids");
    let excepted: BTreeSet<String> = NO_UNMEASURED_ROW.iter().map(|(id, _)| id.to_string()).collect();
    assert_eq!(excepted, catalogue, "unmeasured exceptions vs catalogue ip.* ids");
    assert!(NO_UNMEASURED_ROW.iter().all(|(_, why)| !why.is_empty()));
}
