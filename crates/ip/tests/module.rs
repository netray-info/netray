// V2 Phase 3, requirement 10: the IP module, its config and the pure translation moved from
// lens's `backends/ip.rs`. Needs `[[test]] name = "module"` with `required-features =
// ["testing"]` on this test target, a `testing = []` feature on netray-ip, and `netray-engine`,
// `netray-model` and `toml` as dev-dependencies of netray-ip. Runs offline: no GeoIP data, the
// reputation lists are temp files.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_ip::backend::Ifconfig;
use netray_ip::{IpModule, ModuleConfig, testing, translate};
use netray_model::{Protocol, Status};
use serde_json::Value;

const IFCONFIG_JSON: &str = include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");

fn contracts(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn measured(outcome: SectionOutcome) -> (Vec<netray_model::CheckResult>, Value) {
    match outcome {
        SectionOutcome::Measured { checks, presentation } => (checks, presentation),
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn status_of(verdict: &str) -> Status {
    match verdict {
        "pass" => Status::Pass,
        "warn" => Status::Warn,
        "fail" => Status::Fail,
        "skip" => Status::NotApplicable,
        other => panic!("unknown lens verdict {other}"),
    }
}

fn ctx() -> RunContext {
    RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    }
}

fn v4(s: &str) -> Ipv4Addr {
    s.parse().unwrap()
}

fn v6(s: &str) -> Ipv6Addr {
    s.parse().unwrap()
}

#[test]
fn translate_equals_lens_ip_section() {
    let ifc: Ifconfig = serde_json::from_str(IFCONFIG_JSON).unwrap();
    let ip: IpAddr = "1.1.1.1".parse().unwrap();
    let (checks, presentation) = measured(translate(&[(ip, Ok(ifc))], 1));

    let lens = contracts("lens-full-healthy.json");
    let section = lens["ip"].as_object().expect("lens ip section");
    let lens_checks = section["checks"].as_array().unwrap();

    assert_eq!(checks.len(), lens_checks.len(), "check count");
    for lc in lens_checks {
        let id = format!("ip.{}", lc["name"].as_str().unwrap());
        let got = checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("no CheckResult {id}"));
        assert_eq!(got.status, status_of(lc["verdict"].as_str().unwrap()), "{id}");
        let want: Vec<String> = lc
            .get("messages")
            .and_then(Value::as_array)
            .map(|m| m.iter().map(|s| s.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        assert_eq!(got.findings, want, "{id} findings");
    }

    // Not asserted: `detail_url` (lens-side) and `status` (lens's scoring).
    assert_eq!(presentation["headline"], section["headline"], "headline");
    assert_eq!(presentation["addresses"], section["addresses"], "addresses");
}

#[test]
fn translate_without_public_address_is_not_applicable() {
    assert!(matches!(translate(&[], 0), SectionOutcome::NotApplicable { .. }));
}

#[test]
fn translate_failed_lookup_is_incomplete() {
    let ip: IpAddr = "1.1.1.1".parse().unwrap();
    assert!(matches!(
        translate(&[(ip, Err("boom".into()))], 1),
        SectionOutcome::Incomplete { .. }
    ));
}

#[tokio::test]
async fn golden_module_run_equals_translate() {
    let ifc: Ifconfig = serde_json::from_str(IFCONFIG_JSON).unwrap();
    let ip = v4("1.1.1.1");
    let want = translate(&[(IpAddr::V4(ip), Ok(ifc))], 1);

    let module = testing::golden_module(IFCONFIG_JSON);
    assert_eq!(module.protocol(), Protocol::Ip);
    let facts = Facts {
        a: vec![ip],
        ..Facts::default()
    };
    assert_eq!(module.run(&ctx(), &facts).await, want);
}

#[tokio::test]
async fn run_samples_four_ipv4_and_four_ipv6_public_sorted() {
    let facts = Facts {
        // five public (shuffled) plus one private
        a: vec![
            v4("8.8.8.5"),
            v4("10.0.0.1"),
            v4("8.8.8.2"),
            v4("8.8.8.4"),
            v4("8.8.8.1"),
            v4("8.8.8.3"),
        ],
        // four public (shuffled) plus one private
        aaaa: vec![
            v6("2606:4700:4700::1114"),
            v6("fd00::1"),
            v6("2606:4700:4700::1111"),
            v6("2606:4700:4700::1113"),
            v6("2606:4700:4700::1112"),
        ],
        ..Facts::default()
    };
    let module = testing::golden_module(IFCONFIG_JSON);
    let (_, presentation) = measured(module.run(&ctx(), &facts).await);

    let got: Vec<String> = presentation["addresses"]
        .as_array()
        .expect("addresses list")
        .iter()
        .map(|a| a["ip"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        got,
        [
            "8.8.8.1",
            "8.8.8.2",
            "8.8.8.3",
            "8.8.8.4",
            "2606:4700:4700::1111",
            "2606:4700:4700::1112",
            "2606:4700:4700::1113",
            "2606:4700:4700::1114",
        ]
    );
}

#[tokio::test]
async fn run_without_public_address_is_not_applicable() {
    let facts = Facts {
        a: vec![v4("10.0.0.1")],
        aaaa: vec![v6("fd00::1")],
        ..Facts::default()
    };
    let module = testing::golden_module(IFCONFIG_JSON);
    assert!(matches!(
        module.run(&ctx(), &facts).await,
        SectionOutcome::NotApplicable { .. }
    ));
}

#[test]
fn module_config_refuses_unknown_key() {
    let bad: toml::Table = toml::from_str("bogus = 1").unwrap();
    assert!(bad.try_into::<ModuleConfig>().is_err());

    let good: toml::Table = toml::from_str("").unwrap();
    assert!(good.try_into::<ModuleConfig>().is_ok());
}

#[tokio::test]
async fn missing_city_db_refuses_but_missing_feodo_list_starts() {
    let cfg: ModuleConfig = toml::from_str(r#"geoip_city_db = "/nonexistent.mmdb""#).unwrap();
    assert!(IpModule::new(cfg).await.is_err(), "city db is required");

    let cfg: ModuleConfig = toml::from_str(r#"feodo_botnet_ips = "/nonexistent.txt""#).unwrap();
    assert!(IpModule::new(cfg).await.is_ok(), "feodo list only warns");
}

#[tokio::test]
async fn ip_module_declares_ip_checks() {
    let module = IpModule::new(toml::from_str("").unwrap()).await.expect("module builds");
    assert_eq!(module.protocol(), Protocol::Ip);
    assert!(!module.checks().is_empty());
    for id in module.checks() {
        assert!(id.to_string().starts_with("ip."), "{id}");
        assert_eq!(id.protocol(), Protocol::Ip);
    }
}

#[tokio::test]
async fn reload_picks_up_replaced_feodo_list() {
    let dir = std::env::temp_dir().join(format!("netray-ip-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let list = dir.join("feodo.txt");
    std::fs::write(&list, "# feodo\n8.8.4.4\n").unwrap();

    let cfg: ModuleConfig = toml::from_str(&format!("feodo_botnet_ips = {:?}", list.display().to_string())).unwrap();
    let module = IpModule::new(cfg).await.expect("module builds");
    let facts = Facts {
        a: vec![v4("8.8.8.8")],
        ..Facts::default()
    };

    let flagged = |outcome: SectionOutcome| {
        let (checks, _) = measured(outcome);
        let rep = checks
            .iter()
            .find(|c| c.id.to_string() == "ip.reputation")
            .expect("reputation check");
        rep.status == Status::Fail && rep.findings.iter().any(|f| f.contains("8.8.8.8"))
    };

    assert!(!flagged(module.run(&ctx(), &facts).await), "not listed yet");

    std::fs::write(&list, "# feodo\n8.8.4.4\n8.8.8.8\n").unwrap();
    module.reload().await;

    assert!(flagged(module.run(&ctx(), &facts).await), "listed after reload");
    std::fs::remove_dir_all(&dir).ok();
}
