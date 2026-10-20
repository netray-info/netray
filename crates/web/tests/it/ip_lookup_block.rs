//! V2 spec v2-ip-ui requirement 1 (C1, C9, C10): `netray_ip::module::lookup_block` projects one
//! lookup into the `ip.lookup` fields block, registered in `REGISTER`; `translate_lookup` carries
//! it with the `ip.reputation` check.
use std::net::IpAddr;

use netray_engine::SectionOutcome;
use netray_ip::LookupOptions;
use netray_ip::backend::Ifconfig;
use netray_ip::module::{REGISTER, lookup_block, translate, translate_lookup};
use netray_ip::testing::StubIpLookup;
use netray_ip::IpLookup;
use netray_model::Status;
use netray_model::evidence::{BlockId, Shape, Value, validate};

const ADDR: &str = "203.0.113.42";

async fn stub(reverse_dns: bool) -> Ifconfig {
    StubIpLookup::new()
        .lookup_with(
            ADDR.parse().unwrap(),
            LookupOptions {
                reverse_dns,
                ..Default::default()
            },
        )
        .await
        .unwrap()
}

fn fields_of(block: &netray_model::evidence::Block) -> Vec<(String, Value)> {
    match &block.shape {
        Shape::Fields(f) => f.clone(),
        other => panic!("expected fields, got {other:?}"),
    }
}

#[tokio::test]
async fn lookup_block_projects_the_stub_document() {
    let block = lookup_block(&stub(true).await);
    assert_eq!(block.id, BlockId::parse("ip.lookup").unwrap());
    assert_eq!(block.title, "Lookup");
    validate("ip", &REGISTER, std::slice::from_ref(&block)).expect("block fits the register");

    let fields = fields_of(&block);
    let get = |k: &str| fields.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let text = |s: &str| Some(Value::Text(s.into()));

    assert_eq!(get("address"), Some(Value::Addr(ADDR.parse::<IpAddr>().unwrap())));
    assert_eq!(get("hostname"), text("stub.example.com"));
    assert_eq!(get("asn"), text("AS64496"));
    assert_eq!(get("cloud_provider"), text("aws"));
    assert_eq!(get("latitude"), text("52.52"));
    for (flag, expected) in [
        ("internal", false),
        ("datacenter", true),
        ("vpn", false),
        ("tor", false),
        ("bot", false),
        ("c2", false),
        ("spamhaus", false),
        ("anycast", false),
        ("cins", false),
    ] {
        assert_eq!(get(flag), Some(Value::Bool(expected)), "flag {flag}");
    }
    for absent in [
        "accuracy_km",
        "registered_country",
        "tcp_port",
        "vpn_provider",
        "bot_provider",
    ] {
        assert_eq!(get(absent), None, "key {absent} must be omitted");
    }
    assert!(
        fields.iter().all(|(k, _)| !k.starts_with("ua_")),
        "no ua_* key without a user agent"
    );
}

#[tokio::test]
async fn translate_lookup_carries_the_block_and_the_reputation_check() {
    let ip: IpAddr = ADDR.parse().unwrap();
    let mut tor = stub(false).await;
    tor.network.is_tor = true;
    let mut same = stub(false).await;
    same.network.is_tor = true;

    let expected = match translate(&[(ip, Ok(same))], 1) {
        SectionOutcome::Measured { checks, .. } => checks,
        other => panic!("expected Measured, got {other:?}"),
    };

    match translate_lookup(ip, Ok(tor)) {
        SectionOutcome::Measured { checks, evidence } => {
            assert_eq!(evidence.len(), 1);
            let lookup_id = BlockId::parse("ip.lookup").unwrap();
            assert_eq!(evidence[0].id, lookup_id);
            let rep = checks
                .iter()
                .find(|c| c.id.to_string() == "ip.reputation")
                .expect("ip.reputation check");
            assert_eq!(rep.status, Status::Fail);
            assert_eq!(rep.evidence, vec![lookup_id]);
            let want = expected
                .iter()
                .find(|c| c.id.to_string() == "ip.reputation")
                .expect("translate's ip.reputation");
            assert_eq!(rep.findings, want.findings);
        }
        other => panic!("expected Measured, got {other:?}"),
    }

    assert!(matches!(
        translate_lookup(ip, Err("lookup failed".into())),
        SectionOutcome::Incomplete { .. }
    ));
}
