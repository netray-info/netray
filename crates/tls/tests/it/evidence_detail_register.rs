// v2-tls-backend, Phase 1 (C5/R5, C16, C17): the register declares the detail keys of the
// chain and handshake blocks, the contract fixture still validates, and the volatile paths
// cover the OCSP timestamps and live answer but not the certificate serial.
//
// Spec: specs/features/v2-tls-backend/spec.md, Phase 1.

use std::collections::BTreeSet;
use std::path::PathBuf;

use netray_engine::SectionOutcome;
use netray_engine::{EvidencePath, Module};
use netray_model::evidence::Block;
use netray_model::evidence::{ValueType, validate};
use netray_tls::module::REGISTER;
use netray_tls::routes::InspectResponse;
use netray_tls::{ModuleConfig, TlsModule, translate};

#[test]
fn register_declares_the_detail_keys() {
    use ValueType::{Bool, Text, Time};
    let rows: &[(&str, &str, ValueType)] = &[
        ("tls.chain", "serial", Text),
        ("tls.chain", "fingerprint_sha1", Text),
        ("tls.chain", "policy", Text),
        ("tls.chain", "self_signed", Bool),
        ("tls.chain", "expired", Bool),
        ("tls.chain", "ocsp_url", Text),
        ("tls.handshake", "sni", Text),
        ("tls.handshake", "starttls", Text),
        ("tls.handshake", "ocsp_status", Text),
        ("tls.handshake", "ocsp_this_update", Time),
        ("tls.handshake", "ocsp_next_update", Time),
        ("tls.handshake", "ocsp_live_status", Text),
        ("tls.handshake", "ocsp_live_revoked_at", Time),
        ("tls.handshake", "ocsp_live_reason", Text),
    ];
    for (block, key, ty) in rows {
        let got = REGISTER
            .keys
            .iter()
            .find(|(b, k, _)| b == block && k == key)
            .map(|(_, _, t)| *t);
        assert_eq!(got, Some(*ty), "{block}/{key}");
    }
}

#[test]
fn fixture_translation_validates_against_the_register() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts/tlsight-inspect.json");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let resp: InspectResponse = serde_json::from_value(doc).unwrap();
    let blocks: Vec<Block> = match translate(&resp) {
        SectionOutcome::Measured { evidence, .. } => evidence,
        other => panic!("expected Measured, got {other:?}"),
    };
    validate("tls", &REGISTER, &blocks).unwrap_or_else(|e| panic!("{e:?}"));
}

#[tokio::test]
async fn volatile_paths_cover_ocsp_times_and_live_answer_but_not_serial() {
    let cfg: ModuleConfig = toml::from_str("[limits]\nmax_ports = 3").unwrap();
    let module = TlsModule::new(cfg, None).await.expect("module builds");
    let got: BTreeSet<&str> = module
        .volatile()
        .iter()
        .map(|p: &EvidencePath| p.0)
        .collect();
    for want in [
        "tls.handshake/ocsp_this_update",
        "tls.handshake/ocsp_next_update",
        "tls.handshake/ocsp_live_status",
        "tls.handshake/ocsp_live_revoked_at",
        "tls.handshake/ocsp_live_reason",
        "tls.chain/days_remaining",
        "tls.handshake/handshake_ms",
    ] {
        assert!(got.contains(want), "{want} is volatile; got {got:?}");
    }
    assert!(
        !got.contains("tls.chain/serial"),
        "serial is stable, not volatile"
    );
}
