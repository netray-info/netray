// specs/features/v2-correctness-followups Phase 1, criteria C1/R1, C3, C4, C5, C6, C7, C8:
// a legacy-version probe (Ssl3, Tls10, Tls11, Tls12) is Refused by close_notify (either level) or
// a fatal alert 10, 40, 47, 50, 70, 71, 86 or 112, with detail "alert <name>". Fatal 80, 90, an
// unregistered code, a timeout, garbage and a partial answer stay Unanswered; a warning alert
// other than close_notify is skipped.

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

const LEGACY: [ProbeKind; 4] = [
    ProbeKind::Ssl3,
    ProbeKind::Tls10,
    ProbeKind::Tls11,
    ProbeKind::Tls12,
];

#[test]
fn legacy_probe_alert_verdicts() {
    // (level, description, verdict, detail)
    let rows: [(u8, u8, Verdict, &str); 12] = [
        (1, 0, Verdict::Refused, "alert close_notify"),
        (2, 0, Verdict::Refused, "alert close_notify"),
        (2, 10, Verdict::Refused, "alert unexpected_message"),
        (2, 40, Verdict::Refused, "alert handshake_failure"),
        (2, 47, Verdict::Refused, "alert illegal_parameter"),
        (2, 50, Verdict::Refused, "alert decode_error"),
        (2, 70, Verdict::Refused, "alert protocol_version"),
        (2, 71, Verdict::Refused, "alert insufficient_security"),
        (2, 86, Verdict::Refused, "alert inappropriate_fallback"),
        (2, 112, Verdict::Refused, "alert unrecognized_name"),
        (2, 80, Verdict::Unanswered, ""),
        (2, 200, Verdict::Unanswered, ""),
    ];
    for kind in LEGACY {
        for (level, desc, want, detail) in rows {
            let o = classify(kind, &record(21, &[level, desc]), StreamEnd::Eof);
            assert_eq!(o.verdict, want, "{kind:?} level {level} desc {desc}: {o:?}");
            if want == Verdict::Refused {
                assert_eq!(o.detail, detail, "{kind:?} level {level} desc {desc}");
            }
        }
    }
    let o = classify(ProbeKind::Tls10, &record(21, &[2, 90]), StreamEnd::Eof);
    assert_eq!(o.verdict, Verdict::Unanswered, "user_canceled: {o:?}");
}

#[test]
fn tls10_no_bytes_timeout_is_unanswered() {
    let o = classify(ProbeKind::Tls10, &[], StreamEnd::Timeout);
    assert_eq!(o.verdict, Verdict::Unanswered, "{o:?}");
    assert_eq!(o.detail, "timed out");
}

#[test]
fn tls10_warning_unrecognized_name_then_server_hello_is_accepted() {
    let mut bytes = record(21, &[1, 112]);
    bytes.extend(server_hello(0x0301, 0xC013));
    let o = classify(ProbeKind::Tls10, &bytes, StreamEnd::Eof);
    assert_eq!(o.verdict, Verdict::Accepted, "{o:?}");
}
