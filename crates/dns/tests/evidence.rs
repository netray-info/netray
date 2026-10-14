// V2 evidence, Phase 2 (requirements 3a, 4, 5): the DNS register, the records blocks `translate`
// emits, the references of each measured check and the volatile `ttl` paths.
//
// Spec: specs/features/v2-evidence/spec.md, Phase 2.
//
// Interface assumed (does not exist yet):
// - `netray_dns::module::REGISTER: netray_model::evidence::Register`
// - `translate` returns `SectionOutcome::Measured { evidence, .. }` with one records block
//   `dns.<type>` per answered base-name batch, the six facts types (a, aaaa, mx, caa, ns, https)
//   included; a definite empty answer is a block with zero records, an errored batch has no
//   block. Where the facts carry the same id, the run keeps the facts block.
// - a judged check references its CHECK_INPUTS blocks as they exist in the section's output;
//   `dns.ttl` references every records block of the section.
// - `DnsModule::volatile()` lists `dns.<type>/ttl` for every records block of the register.
//
// No `testing` feature is needed: the seam is `translate` and `DnsModule::new`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use mhost::resolver::Lookups;
use netray_dns::api::{BatchEvent, CheckDoneEvent, LintEvent};
use netray_dns::module::REGISTER;
use netray_dns::{CheckEvent, DnsModule, ModuleConfig, translate};
use netray_engine::{EvidencePath, Module, SectionOutcome};
use netray_model::Status;
use netray_model::evidence::{Block, BlockId, Shape, check_evidence_rule, validate};
use serde_json::json;

const FACTS_BLOCKS: &[&str] = &[
    "dns.a",
    "dns.aaaa",
    "dns.mx",
    "dns.caa",
    "dns.ns",
    "dns.https",
];

/// The record types of a check's inputs, as `CHECK_INPUTS` in `module.rs` maps them.
const CHECK_INPUTS: &[(&str, &[&str])] = &[
    ("caa", &["CAA"]),
    ("cname_apex", &["CNAME", "SOA"]),
    ("dnssec", &["DNSKEY", "DS"]),
    ("dnskey_algorithm", &["DNSKEY"]),
    ("dnssec_rollover", &["DNSKEY", "DS"]),
    ("https_svcb", &["HTTPS", "SVCB"]),
    ("ns", &["NS"]),
    ("ns_lame", &["NS"]),
    ("ns_delegation", &["NS"]),
];

fn sse(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts")
            .join(name),
    )
    .unwrap()
}

fn parse_events(sse: &str) -> Vec<CheckEvent> {
    sse.split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .map(|block| {
            let field = |p: &str| {
                block
                    .lines()
                    .find_map(|l| l.strip_prefix(p))
                    .unwrap_or_else(|| panic!("no {p} line"))
                    .trim()
            };
            let data = field("data:");
            match field("event:") {
                "batch" => CheckEvent::Batch(serde_json::from_str::<BatchEvent>(data).unwrap()),
                "lint" => CheckEvent::Lint(serde_json::from_str::<LintEvent>(data).unwrap()),
                "done" => CheckEvent::Done(serde_json::from_str::<CheckDoneEvent>(data).unwrap()),
                other => panic!("unknown event {other}"),
            }
        })
        .collect()
}

fn measured(events: &[CheckEvent]) -> (Vec<netray_model::CheckResult>, Vec<Block>) {
    match translate(events) {
        SectionOutcome::Measured {
            checks, evidence, ..
        } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn ids<'a>(it: impl IntoIterator<Item = &'a netray_model::evidence::BlockId>) -> BTreeSet<String> {
    it.into_iter().map(ToString::to_string).collect()
}

#[test]
fn fixtures_validate_and_checks_reference_their_inputs() {
    for name in ["prism.sse", "prism-no-address.sse"] {
        let (checks, evidence) = measured(&parse_events(&sse(name)));
        validate("dns", &REGISTER, &evidence)
            .unwrap_or_else(|e| panic!("{name}: evidence does not validate: {e:?}"));

        let module_blocks = ids(evidence.iter().map(|b| &b.id));
        let allowed: BTreeSet<String> = module_blocks
            .iter()
            .cloned()
            .chain(FACTS_BLOCKS.iter().map(|s| s.to_string()))
            .collect();

        for check in &checks {
            let refs = ids(&check.evidence);
            assert!(
                refs.is_subset(&allowed),
                "{name}: {} references {refs:?}, outside {allowed:?}",
                check.id
            );
            let short = check.id.to_string().replace("dns.", "");
            if matches!(check.status, Status::Pass | Status::Warn | Status::Fail)
                && let Some((_, types)) = CHECK_INPUTS.iter().find(|(c, _)| *c == short)
            {
                let want: BTreeSet<String> = types
                    .iter()
                    .map(|t| format!("dns.{}", t.to_lowercase()))
                    .collect();
                assert_eq!(refs, want, "{name}: {} references its inputs", check.id);
            }
            if short == "ttl" {
                assert!(
                    module_blocks.is_subset(&refs),
                    "{name}: dns.ttl must reference every records block {module_blocks:?}, has {refs:?}"
                );
            }
        }
    }
}

#[test]
fn own_caa_batch_backs_dns_caa_when_the_facts_lack_it() {
    // prism.sse carries the check run's own CAA batch.
    let (checks, evidence) = measured(&parse_events(&sse("prism.sse")));
    validate("dns", &REGISTER, &evidence).expect("validates");

    let block = evidence
        .iter()
        .find(|b| b.id.to_string() == "dns.caa")
        .unwrap_or_else(|| panic!("translate emits dns.caa: {evidence:?}"));
    match &block.shape {
        Shape::Records(records) => assert_eq!(records.len(), 1, "{records:?}"),
        other => panic!("dns.caa must be records, got {other:?}"),
    }
    let caa = checks
        .iter()
        .find(|c| c.id.to_string() == "dns.caa")
        .expect("dns.caa");
    assert_eq!(ids(&caa.evidence), BTreeSet::from(["dns.caa".to_string()]));

    // The run's blocks as `RunReport::blocks` keeps them: facts first, one block per id.
    let facts: Vec<Block> = FACTS_BLOCKS
        .iter()
        .filter(|id| **id != "dns.caa")
        .map(|id| Block {
            id: BlockId::parse(id).unwrap(),
            title: id.to_string(),
            shape: Shape::Records(vec![]),
        })
        .collect();
    let mut blocks = facts;
    for b in evidence {
        if !blocks.iter().any(|f| f.id == b.id) {
            blocks.push(b);
        }
    }
    assert_eq!(check_evidence_rule(&checks, &blocks), Ok(()));
}

const DOMAIN: &str = "example.com.";

/// A DNSKEY batch in which each name server answered with `rcode` (a code-carrying NxDomain).
fn dnskey_batch(rcode: &str) -> CheckEvent {
    let items: Vec<_> = ["udp:1.1.1.1:53", "udp:8.8.8.8:53"]
        .iter()
        .map(|ns| {
            json!({
                "query": { "name": DOMAIN, "record_type": "DNSKEY" },
                "name_server": ns,
                "result": { "NxDomain": {
                    "response_time": { "secs": 0, "nanos": 12_000_000 },
                    "response_code": rcode
                }}
            })
        })
        .collect();
    let lookups: Lookups = serde_json::from_value(json!({ "lookups": items })).unwrap();
    CheckEvent::Batch(BatchEvent {
        request_id: "r".to_owned(),
        record_type: "DNSKEY".to_owned(),
        lookups,
        completed: 1,
        total: 1,
        transport: None,
        source: None,
    })
}

fn lint_not_found(category: &str) -> CheckEvent {
    CheckEvent::Lint(
        serde_json::from_value::<LintEvent>(json!({
            "request_id": "r", "category": category, "results": [{ "NotFound": [] }]
        }))
        .unwrap(),
    )
}

#[test]
fn errored_dnskey_batch_has_no_block_and_definite_empty_has_an_empty_block() {
    // SERVFAIL on every resolver: no answer, so no block and the check is unmeasured.
    let (checks, evidence) =
        measured(&[dnskey_batch("SERVFAIL"), lint_not_found("dnskey_algorithm")]);
    validate("dns", &REGISTER, &evidence).expect("validates");
    assert!(
        !evidence.iter().any(|b| b.id.to_string() == "dns.dnskey"),
        "an errored batch must not produce a dns.dnskey block: {evidence:?}"
    );
    let check = checks
        .iter()
        .find(|c| c.id.to_string() == "dns.dnskey_algorithm")
        .expect("dns.dnskey_algorithm");
    assert_eq!(check.status, Status::Unmeasured);

    // NOERROR without records: a definite empty answer, a block with zero records.
    let (_, evidence) = measured(&[dnskey_batch("NOERROR"), lint_not_found("dnskey_algorithm")]);
    validate("dns", &REGISTER, &evidence).expect("validates");
    let block = evidence
        .iter()
        .find(|b| b.id.to_string() == "dns.dnskey")
        .unwrap_or_else(|| panic!("definite empty answer needs a dns.dnskey block: {evidence:?}"));
    match &block.shape {
        Shape::Records(records) => assert!(records.is_empty(), "{records:?}"),
        other => panic!("dns.dnskey must be records, got {other:?}"),
    }
}

#[tokio::test]
async fn volatile_ttl_paths_are_registered() {
    let cfg: ModuleConfig = toml::from_str("servers = [\"cloudflare\"]").unwrap();
    let module = DnsModule::new(cfg).await.expect("module builds");
    let volatile: &[EvidencePath] = module.volatile();

    let want: BTreeSet<String> = [
        "cname", "dnskey", "ds", "nsec", "nsec3", "rrsig", "soa", "svcb", "txt", "a", "aaaa", "mx",
        "caa", "ns", "https",
    ]
    .iter()
    .map(|t| format!("dns.{t}/ttl"))
    .collect();
    let got: BTreeSet<String> = volatile.iter().map(|p| p.0.to_string()).collect();
    assert_eq!(
        got, want,
        "volatile lists dns.<type>/ttl for every records block"
    );

    for path in volatile {
        let (block, key, row) = EvidencePath::parse(path.0).expect(path.0);
        assert_eq!(key, "ttl", "{}", path.0);
        assert!(row.is_none(), "{}", path.0);
        let name = format!("{}.{}", block.module(), block.name());
        assert!(
            REGISTER.blocks.iter().any(|(b, _)| *b == name),
            "{}: block {name} not registered",
            path.0
        );
        assert!(
            REGISTER
                .keys
                .iter()
                .any(|(b, k, _)| *b == name && *k == "ttl"),
            "{}: key ttl not registered for {name}",
            path.0
        );
    }
}
