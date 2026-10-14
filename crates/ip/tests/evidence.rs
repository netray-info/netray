// V2 evidence, Phase 5 (IP row): the IP register, the `ip.addresses` table the module measures,
// the `ip.reputation` reference to it and the volatile paths.
//
// Spec: specs/features/v2-evidence/spec.md, Phase 5.
//
// Interface assumed (does not exist yet):
// - `netray_ip::module::REGISTER: netray_model::evidence::Register` with `ip.addresses` as a
//   table keyed `address` (address Addr; asn, org, prefix, network_type, country, city Text;
//   spamhaus, c2, tor, vpn Bool). An absent text value is Text(""); asn is Text such as "AS15169".
// - `SectionOutcome::Measured { evidence, .. }` holds `ip.addresses`, one row per public address;
//   `ip.reputation` references it.
//
// Cargo needs on netray-ip: `[[test]] name = "evidence"`, `path = "tests/evidence.rs"`,
// `required-features = ["testing"]`, and `--test evidence` in the justfile `test-rust-offline`.

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use netray_engine::{Domain, EvidencePath, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_ip::module::REGISTER;
use netray_ip::{IpModule, ModuleConfig};
use netray_model::CheckResult;
use netray_model::evidence::{Block, Shape, Value, check_evidence_rule, validate};

const LISTED: &str = "8.8.8.8";

fn ctx() -> RunContext {
    RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    }
}

/// Runs the module over `LISTED` with `config_key` pointing at a list file holding `content`.
async fn run(config_key: &str, content: &str) -> (IpModule, Vec<CheckResult>, Vec<Block>) {
    // Tests run in parallel and several use the same list key; a per-call directory keeps one
    // test's cleanup from deleting another's list before its module loads it.
    static CALL: AtomicUsize = AtomicUsize::new(0);
    let call = CALL.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("netray-ip-evidence-{}-{call}-{config_key}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let list = dir.join("list.txt");
    std::fs::write(&list, content).unwrap();
    let cfg: ModuleConfig = toml::from_str(&format!("{config_key} = {:?}", list.display().to_string())).unwrap();
    let module = IpModule::new(cfg).await.expect("module builds");
    let facts = Facts {
        a: vec![LISTED.parse::<Ipv4Addr>().unwrap()],
        ..Facts::default()
    };
    let outcome = module.run(&ctx(), &facts).await;
    std::fs::remove_dir_all(&dir).ok();
    let SectionOutcome::Measured { checks, evidence, .. } = outcome else {
        panic!("expected Measured, got {outcome:?}");
    };
    (module, checks, evidence)
}

fn reputation_references_addresses(checks: &[CheckResult]) {
    let rep = checks
        .iter()
        .find(|c| c.id.to_string() == "ip.reputation")
        .expect("ip.reputation is in the section");
    assert!(
        rep.evidence.iter().any(|b| b.to_string() == "ip.addresses"),
        "ip.reputation references {:?}",
        rep.evidence
    );
}

/// C2: the section's blocks validate against the register and satisfy the evidence rule.
#[tokio::test]
async fn addresses_block_validates_and_reputation_references_it() {
    let (_, checks, evidence) = run("feodo_botnet_ips", &format!("{LISTED}\n")).await;
    validate("ip", &REGISTER, &evidence).unwrap_or_else(|e| panic!("evidence does not validate: {e:?}"));
    reputation_references_addresses(&checks);
    check_evidence_rule(&checks, &evidence).unwrap_or_else(|e| panic!("evidence rule broken: {e:?}"));
}

/// C8: a listed address has its flag column true, one row per public address. One row per
/// list-backed flag.
#[tokio::test]
async fn listed_address_row_carries_the_matching_flag() {
    let rows = [
        ("feodo_botnet_ips", "c2", format!("{LISTED}\n")),
        ("spamhaus_drop", "spamhaus", "8.8.8.0/24 ; SBL1\n".to_string()),
        ("tor_exit_nodes", "tor", format!("{LISTED}\n")),
    ];
    for (config_key, flag, content) in rows {
        let (_, checks, evidence) = run(config_key, &content).await;
        reputation_references_addresses(&checks);
        let block = evidence
            .iter()
            .find(|b| b.id.to_string() == "ip.addresses")
            .unwrap_or_else(|| panic!("{flag}: no ip.addresses block"));
        let Shape::Table { key, columns, rows } = &block.shape else {
            panic!("{flag}: ip.addresses is not a table");
        };
        assert_eq!(key, "address");
        assert_eq!(rows.len(), 1, "{flag}: one row per public address");
        let col = |name: &str| {
            columns
                .iter()
                .position(|c| c == name)
                .unwrap_or_else(|| panic!("{flag}: no column {name}"))
        };
        assert_eq!(rows[0][col("address")], Value::Addr(LISTED.parse().unwrap()));
        for other in ["spamhaus", "c2", "tor", "vpn"] {
            assert_eq!(
                rows[0][col(other)],
                Value::Bool(other == flag),
                "{config_key}: column {other}"
            );
        }
    }
}

/// C3: every volatile path names a registered block and key.
#[tokio::test]
async fn volatile_paths_name_registered_keys() {
    let (module, _, _) = run("feodo_botnet_ips", &format!("{LISTED}\n")).await;
    for path in module.volatile() {
        let (block, key, _) = EvidencePath::parse(path.0).expect(path.0);
        let name = format!("{}.{}", block.module(), block.name());
        assert!(
            REGISTER.blocks.iter().any(|(b, _)| *b == name),
            "{}: block {name} not registered",
            path.0
        );
        assert!(
            REGISTER.keys.iter().any(|(b, k, _)| *b == name && *k == key),
            "{}: key {key} not registered for {name}",
            path.0
        );
    }
}
