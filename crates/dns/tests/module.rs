// V2 Phase 5, requirement 16: the DNS module, its config and the pure translation moved from
// lens's `backends/dns.rs`. C4 (translate equals the DNS section of lens's full-output golden),
// C5 (facts_from_lookups on the golden's lookups) and the module surface.
//
// Cargo needs on netray-dns (not edited here):
// - `[[test]] name = "module"`, `path = "tests/module.rs"`, `required-features = ["testing"]`
// - a `testing = []` feature
// - dev-dependencies `netray-engine` and `toml`
//
// Deserialize needs: prism's check events are Serialize-only today. This test deserializes the
// golden frames into `netray_dns::api::{BatchEvent, LintEvent, CheckDoneEvent}`, so all three
// need `Deserialize` (LintEvent.category: `&'static str` becomes `String`), and the translate
// input type `netray_dns::CheckEvent` is an enum with the variants `Batch(BatchEvent)`,
// `Lint(LintEvent)` and `Done(CheckDoneEvent)`, exported at the crate root. The `error` frame
// is not in the goldens.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use mhost::resolver::Lookups;
use netray_dns::api::{BatchEvent, CheckDoneEvent, LintEvent};
use netray_dns::{CheckEvent, DnsModule, ModuleConfig, facts_from_lookups, testing, translate};
use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_model::{Protocol, Status};
use serde_json::{Value, json};

fn contracts_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn contracts(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(contracts_path(name)).unwrap()).unwrap()
}

fn sse(name: &str) -> String {
    std::fs::read_to_string(contracts_path(name)).unwrap()
}

/// SSE frames are `event: <name>` + `data: <json>`; the event name selects the payload type.
fn parse_events(sse: &str) -> Vec<CheckEvent> {
    sse.split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .map(|block| {
            let name = block
                .lines()
                .find_map(|l| l.strip_prefix("event:"))
                .expect("event line")
                .trim();
            let data = block
                .lines()
                .find_map(|l| l.strip_prefix("data:"))
                .expect("data line")
                .trim();
            match name {
                "batch" => CheckEvent::Batch(serde_json::from_str::<BatchEvent>(data).unwrap()),
                "lint" => CheckEvent::Lint(serde_json::from_str::<LintEvent>(data).unwrap()),
                "done" => CheckEvent::Done(serde_json::from_str::<CheckDoneEvent>(data).unwrap()),
                other => panic!("unknown event {other}"),
            }
        })
        .collect()
}

fn batch_lookups(sse: &str) -> Lookups {
    parse_events(sse)
        .into_iter()
        .filter_map(|e| match e {
            CheckEvent::Batch(b) => Some(b.lookups),
            _ => None,
        })
        .fold(Lookups::empty(), |acc, l| acc.merge(l))
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

/// (prism golden, lens full-output golden, the A/AAAA addresses the golden's batches hold)
const FIXTURES: &[(&str, &str, &[&str])] = &[
    ("prism.sse", "lens-full-healthy.json", &["192.0.2.10"]),
    (
        "prism-no-address.sse",
        "lens-full-no-address-records.json",
        &[],
    ),
];

#[test]
fn translate_equals_lens_dns_section_for_every_fixture() {
    for (sse_name, lens_name, ips) in FIXTURES {
        let outcome = translate(&parse_events(&sse(sse_name)));
        let lens = contracts(lens_name);
        let dns = lens["dns"].as_object().expect("lens dns section");

        let SectionOutcome::Measured {
            checks,
            presentation,
        } = outcome
        else {
            panic!("{sse_name}: expected Measured, got {outcome:?}");
        };

        let lens_checks = dns["checks"].as_array().unwrap();
        assert_eq!(checks.len(), lens_checks.len(), "{sse_name}: check count");
        for lc in lens_checks {
            let name = lc["name"].as_str().unwrap();
            let id = format!("dns.{name}");
            let got = checks
                .iter()
                .find(|c| c.id.to_string() == id)
                .unwrap_or_else(|| panic!("{sse_name}: no CheckResult {id}"));
            assert_eq!(
                got.status,
                status_of(lc["verdict"].as_str().unwrap()),
                "{sse_name}: {id} status"
            );
            let want: Vec<String> = lc
                .get("messages")
                .and_then(Value::as_array)
                .map(|m| m.iter().map(|s| s.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            assert_eq!(got.findings, want, "{sse_name}: {id} findings");
        }
        // The email categories (spf here) are dropped, not rendered as dns checks.
        assert!(
            checks.iter().all(|c| c.id.to_string() != "dns.spf"),
            "{sse_name}: spf belongs to the email module"
        );

        // Translation-derived keys of lens's dns object. Not asserted: `detail_url` (lens's
        // public URL) and `status` (lens's scoring). `resolved_ips` is not in the lens
        // golden (the IP section reads it) but must be present, from the batch A/AAAA records.
        assert_eq!(presentation["headline"], dns["headline"], "{sse_name}");
        assert_eq!(
            presentation["resolved_ips"],
            json!(ips),
            "{sse_name}: presentation.resolved_ips"
        );
    }
}

#[test]
fn facts_from_lookups_holds_the_golden_records() {
    let facts = facts_from_lookups(&batch_lookups(&sse("prism.sse")));
    assert_eq!(
        facts.a,
        vec!["192.0.2.10".parse::<std::net::Ipv4Addr>().unwrap()]
    );
    assert!(facts.aaaa.is_empty());
    assert!(facts.mx.is_empty());
    assert!(facts.caa.is_empty());
    let mut ns: Vec<String> = facts
        .ns
        .iter()
        .map(|n| n.trim_end_matches('.').to_string())
        .collect();
    ns.sort();
    assert_eq!(ns, vec!["ns1.example.com", "ns2.example.com"]);

    let none = facts_from_lookups(&batch_lookups(&sse("prism-no-address.sse")));
    assert_eq!(none, Facts::default());
}

#[test]
fn module_config_refuses_unknown_keys() {
    let bad: toml::Table = toml::from_str("bogus = 1").unwrap();
    assert!(
        bad.try_into::<ModuleConfig>().is_err(),
        "unknown key refused"
    );

    let bad_nested: toml::Table = toml::from_str("[dns]\nbogus = 1").unwrap();
    assert!(bad_nested.try_into::<ModuleConfig>().is_err());

    let good: toml::Table = toml::from_str(
        "servers = [\"cloudflare\"]\n[dns]\nallow_system_resolvers = false\n[limits]\nmax_servers = 2",
    )
    .unwrap();
    assert!(good.try_into::<ModuleConfig>().is_ok());
}

#[tokio::test]
async fn dns_module_declares_dns_checks() {
    let cfg: ModuleConfig = toml::from_str("servers = [\"cloudflare\"]").unwrap();
    let module = DnsModule::new(cfg).await.expect("module builds");
    assert_eq!(module.protocol(), Protocol::Dns);
    assert!(!module.checks().is_empty());
    for id in module.checks() {
        assert!(id.to_string().starts_with("dns."), "{id}");
        assert_eq!(id.protocol(), Protocol::Dns);
    }
}

#[tokio::test]
async fn golden_module_run_equals_translate() {
    let golden = sse("prism.sse");
    let want = translate(&parse_events(&golden));

    let module = testing::golden_module(&golden);
    assert_eq!(module.protocol(), Protocol::Dns);
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let got = module.run(&ctx, &Facts::default()).await;
    assert_eq!(got, want);
}
