// V2 history, Phase 4, requirement 17: dns's volatile list covers the RRSIG data text (validity
// window and signature change on every re-sign), so two runs differing only there fingerprint equal.

use netray_dns::{DnsModule, ModuleConfig};
use netray_engine::Module;
use netray_model::Grade;
use netray_model::evidence::{Block, BlockId, Record, Shape, Value as EvValue};
use netray_model::run::{Run, fingerprint};

fn records(id: &str, rtype: &str, data: &str) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: id.into(),
        shape: Shape::Records(vec![Record {
            owner: EvValue::Text("example.com.".into()),
            rtype: EvValue::Text(rtype.into()),
            ttl: EvValue::Int(300),
            data: EvValue::Text(data.into()),
        }]),
    }
}

fn run(a_data: &str, rrsig_data: &str) -> Run {
    Run {
        profile_version: 1,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: vec![],
        checks: vec![],
        blocks: vec![
            records("dns.a", "A", a_data),
            records("dns.rrsig", "RRSIG", rrsig_data),
        ],
    }
}

async fn paths() -> Vec<&'static str> {
    let cfg: ModuleConfig = toml::from_str("servers = [\"cloudflare\"]").unwrap();
    let module = DnsModule::new(cfg).await.expect("module builds");
    module.volatile().iter().map(|p| p.0).collect()
}

#[tokio::test]
async fn rrsig_data_does_not_change_the_fingerprint() {
    let p = paths().await;
    let a = run(
        "192.0.2.1",
        "A 13 2 300 20261101000000 20261001000000 12345 example.com. c2lnQQ==",
    );
    let b = run(
        "192.0.2.1",
        "A 13 2 300 20261108000000 20261008000000 12345 example.com. c2lnQg==",
    );
    assert_eq!(fingerprint(&a, &p), fingerprint(&b, &p));
}

#[tokio::test]
async fn a_record_data_change_changes_the_fingerprint() {
    let p = paths().await;
    let a = run("192.0.2.1", "sig");
    let b = run("192.0.2.2", "sig");
    assert_ne!(fingerprint(&a, &p), fingerprint(&b, &p));
}

fn run_with(id: &str, rtype: &str, data: &str) -> Run {
    Run {
        profile_version: 1,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: vec![],
        checks: vec![],
        blocks: vec![records(id, rtype, data)],
    }
}

// Requirement 18: dns.a and dns.aaaa stay compared; the volatile list must not cover them.
#[tokio::test]
async fn address_record_changes_change_the_fingerprint() {
    let p = paths().await;
    let rows = [
        ("dns.a", "A", "192.0.2.1", "192.0.2.2"),
        ("dns.aaaa", "AAAA", "2001:db8::1", "2001:db8::2"),
    ];
    for (id, rtype, first, second) in rows {
        let a = run_with(id, rtype, first);
        let b = run_with(id, rtype, second);
        assert_ne!(fingerprint(&a, &p), fingerprint(&b, &p), "{id}");
    }
}
