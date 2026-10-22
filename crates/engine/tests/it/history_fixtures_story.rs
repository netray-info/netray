//! The history fixtures tell the boards' story
//! (specs/features/v2-fixture-consistency/spec.md, Phase 3, requirement 9 and the constraints;
//! report.md C3, C8, C9): the shop runs' statuses, every fixture's grade, and the documentation
//! names and addresses only.
//!
//! Copied from crates/lens/tests/it/history_fixtures_story.rs (spec v2-cutover,
//! Phase 6); nothing dropped.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use netray_model::Grade;
use netray_model::run::Run;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct FixtureFile {
    domain: String,
    age_seconds: u64,
    run: Run,
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/acceptance/fixtures/history")
}

fn read(name: &str) -> String {
    let path = dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()))
}

fn load(name: &str) -> FixtureFile {
    serde_json::from_str(&read(name))
        .unwrap_or_else(|e| panic!("fixture {name} does not parse: {e}"))
}

fn all_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    names
}

fn status(run: &Run, id: &str) -> String {
    let check = run
        .checks
        .iter()
        .find(|c| c.id.to_string() == id)
        .unwrap_or_else(|| panic!("run holds no check {id}"));
    serde_json::to_value(check.status)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn shop_runs_tell_the_fixed_and_new_story() {
    // (file, age_seconds, [(check, status)])
    let table: [(&str, u64, [(&str, &str); 5]); 3] = [
        (
            "shop.example.com-1.json",
            1_209_600,
            [
                ("email.dkim", "fail"),
                ("email.dmarc", "fail"),
                ("email.mta_sts", "warn"),
                ("http.hsts", "pass"),
                ("tls.expiry_window", "warn"),
            ],
        ),
        (
            "shop.example.com-2.json",
            604_800,
            [
                ("email.dkim", "fail"),
                ("email.dmarc", "warn"),
                ("email.mta_sts", "warn"),
                ("http.hsts", "pass"),
                ("tls.expiry_window", "warn"),
            ],
        ),
        (
            "shop.example.com-3.json",
            0,
            [
                ("email.dkim", "pass"),
                ("email.dmarc", "warn"),
                ("email.mta_sts", "pass"),
                ("http.hsts", "fail"),
                ("tls.expiry_window", "pass"),
            ],
        ),
    ];
    for (file, age, expected) in table {
        let f = load(file);
        assert_eq!(f.domain, "shop.example.com", "{file}");
        assert_eq!(f.age_seconds, age, "{file}");
        for (id, want) in expected {
            assert_eq!(status(&f.run, id), want, "{file}: {id}");
        }
    }
}

#[tokio::test]
async fn fixture_grades_hold() {
    let table = [
        ("new.example.com.json", Grade::A),
        ("shop.example.com-1.json", Grade::A),
        ("shop.example.com-2.json", Grade::A),
        ("shop.example.com-3.json", Grade::A),
        ("checkout.eu-central.storefront.example.com.json", Grade::A),
        ("example.com.json", Grade::B),
        ("stale.example.com.json", Grade::B),
    ];
    for (file, grade) in table {
        assert_eq!(load(file).run.grade, grade, "{file}");
    }
}

fn documentation_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let o = a.octets();
            matches!(
                (o[0], o[1], o[2]),
                (192, 0, 2) | (198, 51, 100) | (203, 0, 113)
            )
        }
        IpAddr::V6(a) => {
            let s = a.segments();
            s[0] == 0x2001 && s[1] == 0x0db8
        }
    }
}

fn parse_address(text: &str) -> IpAddr {
    text.parse::<IpAddr>()
        .or_else(|_| text.parse::<SocketAddr>().map(|s| s.ip()))
        .unwrap_or_else(|_| panic!("{text:?} is not an address"))
}

/// Every `{"t": "addr", "v": ...}` value under `v`.
fn addr_values(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            if map.get("t").and_then(Value::as_str) == Some("addr")
                && let Some(s) = map.get("v").and_then(Value::as_str)
            {
                out.push(s.to_string());
            }
            map.values().for_each(|x| addr_values(x, out));
        }
        Value::Array(items) => items.iter().for_each(|x| addr_values(x, out)),
        _ => {}
    }
}

#[tokio::test]
async fn fixtures_use_documentation_names_and_addresses() {
    for name in all_names() {
        let value: Value = serde_json::from_str(&read(&name)).unwrap();
        let domain = value["domain"].as_str().unwrap();
        assert!(
            domain == "example.com" || domain.ends_with(".example.com"),
            "{name}: domain {domain} is outside example.com"
        );

        let mut addresses = Vec::new();
        for block in value["run"]["blocks"].as_array().unwrap() {
            addr_values(&block["shape"], &mut addresses);
            let id = block["id"].as_str().unwrap();
            if id == "dns.a" || id == "dns.aaaa" {
                for record in block["shape"]["data"].as_array().unwrap() {
                    let kind = record["type"]["v"].as_str().unwrap_or_default();
                    if kind == "A" || kind == "AAAA" {
                        addresses.push(record["data"]["v"].as_str().unwrap().to_string());
                    }
                }
            }
        }
        assert!(!addresses.is_empty(), "{name}: no address found");
        for a in addresses {
            assert!(
                documentation_address(parse_address(&a)),
                "{name}: address {a} is not a documentation address"
            );
        }
    }
}
