// specs/features/v2-tls-versions-coop, criteria C2 (R2), C9-C15: `classify(kind, bytes, end)`
// reads the server's first answer to a probe and returns Accepted, Refused or Unanswered.
// Contract for the coder: `netray_tls::tls::probe::{ProbeKind, Verdict, ProbeOutcome,
// StreamEnd, classify}`. Only a ServerHello or an alert is an answer; a reset, a timeout,
// a close, a failed connect, garbage or an unoffered suite says nothing about the target.

use netray_tls::tls::probe::{ProbeKind, StreamEnd, Verdict, classify};

fn handshake_message(version: u16, suite: u16) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&version.to_be_bytes());
    body.extend_from_slice(&[0x42; 32]);
    body.push(0);
    body.extend_from_slice(&suite.to_be_bytes());
    body.push(0);
    let mut msg = vec![2, 0, (body.len() >> 8) as u8, body.len() as u8];
    msg.extend_from_slice(&body);
    msg
}

fn record(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut r = vec![
        kind,
        0x03,
        0x01,
        (payload.len() >> 8) as u8,
        payload.len() as u8,
    ];
    r.extend_from_slice(payload);
    r
}

fn server_hello(version: u16, suite: u16) -> Vec<u8> {
    record(22, &handshake_message(version, suite))
}

fn alert(desc: u8) -> Vec<u8> {
    vec![21, 3, 1, 0, 2, 2, desc]
}

fn alert_warning(desc: u8) -> Vec<u8> {
    vec![21, 3, 1, 0, 2, 1, desc]
}

#[test]
fn accepted_at_probed_version_carries_version_and_suite() {
    let o = classify(
        ProbeKind::Tls10,
        &server_hello(0x0301, 0xC013),
        StreamEnd::Eof,
    );
    assert_eq!(o.verdict, Verdict::Accepted);
    assert_eq!(o.version.as_deref(), Some("TLSv1.0"));
    assert_eq!(
        o.cipher_suite.as_deref(),
        Some("TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA")
    );
}

#[test]
fn server_hello_verdicts_table() {
    // (kind, server version, suite, expected verdict, detail fragment for a refusal)
    let rows: [(ProbeKind, u16, u16, Verdict, &str); 5] = [
        // C10: another version than probed is refused, naming it
        (
            ProbeKind::Tls11,
            0x0303,
            0xC02F,
            Verdict::Refused,
            "TLSv1.2",
        ),
        // C12: a suite group accepts a hello up to TLS 1.2 choosing an offered suite
        (
            ProbeKind::SuitesInsecure,
            0x0301,
            0x000A,
            Verdict::Accepted,
            "",
        ),
        // C13: a suite that was not offered is no answer
        (
            ProbeKind::SuitesInsecure,
            0x0303,
            0x1301,
            Verdict::Unanswered,
            "",
        ),
        (
            ProbeKind::SuitesInsecure,
            0x0303,
            0xC02F,
            Verdict::Unanswered,
            "",
        ),
        (ProbeKind::Tls12, 0x0303, 0xC02F, Verdict::Accepted, ""),
    ];
    for (kind, ver, suite, want, frag) in rows {
        let o = classify(kind, &server_hello(ver, suite), StreamEnd::Eof);
        assert_eq!(o.verdict, want, "{kind:?} {ver:#06x} {suite:#06x}: {o:?}");
        if want == Verdict::Refused {
            assert!(o.detail.contains(frag), "{o:?}");
        }
    }
}

#[test]
fn alert_is_refused_naming_the_alert() {
    for (desc, name) in [(70u8, "protocol_version"), (40, "handshake_failure")] {
        let o = classify(ProbeKind::Tls11, &alert(desc), StreamEnd::Eof);
        assert_eq!(o.verdict, Verdict::Refused, "{name}: {o:?}");
        assert!(o.detail.contains(name), "{o:?}");
    }
}

#[test]
fn server_hello_split_over_two_records_is_read_whole() {
    let msg = handshake_message(0x0301, 0xC013);
    let mut bytes = record(22, &msg[..10]);
    bytes.extend(record(22, &msg[10..]));
    let o = classify(ProbeKind::Tls10, &bytes, StreamEnd::Eof);
    assert_eq!(o.verdict, Verdict::Accepted, "{o:?}");
    assert_eq!(o.version.as_deref(), Some("TLSv1.0"));
}

#[test]
fn no_answer_is_unanswered_never_refused() {
    let truncated = {
        let mut b = server_hello(0x0301, 0xC013);
        b.truncate(b.len() - 20);
        b
    };
    let rows: Vec<(&str, Vec<u8>, StreamEnd, &str)> = vec![
        // v2-tls-probe-close: an end of stream or reset with nothing received is a refusal now
        // (`close_after_the_hello_is_refused`); a warning alert before the close decides nothing.
        ("warning then eof", alert_warning(112), StreamEnd::Eof, ""),
        ("timeout", vec![], StreamEnd::Timeout, "timed out"),
        (
            "not sent",
            vec![],
            StreamEnd::NotSent("starttls: 554 no".into()),
            "starttls",
        ),
        (
            "connect",
            vec![],
            StreamEnd::ConnectFailed("refused".into()),
            "",
        ),
        (
            "http",
            b"HTTP/1.1 400 Bad Request\r\n\r\n".to_vec(),
            StreamEnd::Eof,
            "",
        ),
        ("truncated", truncated, StreamEnd::Eof, ""),
    ];
    for (name, bytes, end, frag) in rows {
        let o = classify(ProbeKind::Tls10, &bytes, end);
        assert_eq!(o.verdict, Verdict::Unanswered, "{name}: {o:?}");
        assert!(o.detail.contains(frag), "{name}: {o:?}");
    }
}

// v2-tls-probe-close, C1, C5, C6: a close (FIN or RST) after the written ClientHello with nothing
// received is the server's refusal, recorded as `closed` or `closed (reset)`.
#[test]
fn close_after_the_hello_is_refused() {
    for kind in [ProbeKind::Ssl3, ProbeKind::Tls10, ProbeKind::SuitesInsecure] {
        let o = classify(kind, &[], StreamEnd::Eof);
        assert_eq!(o.verdict, Verdict::Refused, "{kind:?} eof: {o:?}");
        assert_eq!(o.detail, "closed", "{kind:?} eof: {o:?}");
        assert_eq!(o.version, None);
        assert_eq!(o.cipher_suite, None);

        let o = classify(kind, &[], StreamEnd::Reset);
        assert_eq!(o.verdict, Verdict::Refused, "{kind:?} reset: {o:?}");
        assert_eq!(o.detail, "closed (reset)", "{kind:?} reset: {o:?}");
        assert_eq!(o.version, None);
        assert_eq!(o.cipher_suite, None);
    }
}

// v2-tls-probe-close, C2, C7, C8: bytes that decide nothing before a close or a reset keep the
// probe unanswered.
#[test]
fn a_partial_answer_before_the_close_stays_unanswered() {
    let mut partial = server_hello(0x0301, 0xC013);
    partial.truncate(3);
    for (name, bytes) in [("partial", partial), ("warning", alert_warning(112))] {
        for end in [StreamEnd::Eof, StreamEnd::Reset] {
            let o = classify(ProbeKind::Tls10, &bytes, end.clone());
            assert_eq!(o.verdict, Verdict::Unanswered, "{name} {end:?}: {o:?}");
        }
    }
}
