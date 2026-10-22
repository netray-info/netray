// specs/features/v2-tls-versions-coop Phase 1, review finding on R2 (amended): only a fatal
// handshake_failure, protocol_version or insufficient_security alert refuses a probe. A warning
// alert before the ServerHello is skipped; close_notify and other fatal alerts (internal_error,
// decode_error, unrecognized_name) leave the probe unanswered.

use netray_tls::tls::probe::{ProbeKind, StreamEnd, Verdict, classify};

fn record(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut r = vec![kind, 0x03, 0x01];
    r.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    r.extend_from_slice(payload);
    r
}

fn server_hello(version: u16, suite: u16) -> Vec<u8> {
    let mut body = version.to_be_bytes().to_vec();
    body.extend_from_slice(&[7u8; 32]);
    body.push(0);
    body.extend_from_slice(&suite.to_be_bytes());
    body.push(0);
    let mut msg = vec![2, 0, (body.len() >> 8) as u8, body.len() as u8];
    msg.extend_from_slice(&body);
    record(22, &msg)
}

#[test]
fn warning_alert_before_server_hello_is_skipped() {
    for (kind, suite) in [
        (ProbeKind::Tls10, 0xC013),
        (ProbeKind::SuitesInsecure, 0x0005),
    ] {
        let mut bytes = record(21, &[1, 112]);
        bytes.extend(server_hello(0x0301, suite));
        let o = classify(kind, &bytes, StreamEnd::Eof);
        assert_eq!(o.verdict, Verdict::Accepted, "{kind:?}: {o:?}");
    }
}

#[test]
fn only_refusing_fatal_alerts_refuse() {
    for (level, desc, want) in [
        (2u8, 40u8, Verdict::Refused),
        (2, 70, Verdict::Refused),
        (2, 71, Verdict::Refused),
        (2, 80, Verdict::Unanswered),
        (2, 50, Verdict::Unanswered),
        (2, 112, Verdict::Unanswered),
        (1, 0, Verdict::Unanswered),
        (2, 0, Verdict::Unanswered),
    ] {
        let o = classify(
            ProbeKind::Tls10,
            &record(21, &[level, desc]),
            StreamEnd::Eof,
        );
        assert_eq!(o.verdict, want, "level {level} desc {desc}: {o:?}");
    }
}

#[test]
fn suite_group_server_hello_below_ssl3_is_not_accepted() {
    let o = classify(
        ProbeKind::SuitesCbc,
        &server_hello(0x0000, 0xC013),
        StreamEnd::Eof,
    );
    assert_ne!(o.verdict, Verdict::Accepted, "{o:?}");
}
