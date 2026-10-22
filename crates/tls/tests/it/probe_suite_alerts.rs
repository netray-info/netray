// specs/features/v2-correctness-followups Phase 1, criteria C2/R2 and C9: a suite-group probe keeps
// today's alert rule. Only a fatal (level 2) handshake_failure (40), protocol_version (70) or
// insufficient_security (71) refuses; close_notify (0, level 1 or 2) and every other alert leave
// it unanswered. These guard against a change to the version probes leaking into suite probes.

use netray_tls::tls::probe::{ProbeKind, StreamEnd, Verdict, classify};

fn record(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut r = vec![kind, 0x03, 0x01];
    r.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    r.extend_from_slice(payload);
    r
}

#[test]
fn suite_group_alerts_refuse_only_when_fatal_refusing() {
    for kind in [
        ProbeKind::SuitesInsecure,
        ProbeKind::SuitesCbc,
        ProbeKind::SuitesStaticRsa,
    ] {
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
            let o = classify(kind, &record(21, &[level, desc]), StreamEnd::Eof);
            assert_eq!(o.verdict, want, "{kind:?} level {level} desc {desc}: {o:?}");
        }
    }
}
