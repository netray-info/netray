//! Raw protocol-version and suite-group probes.
//!
//! Each probe opens one TCP connection, sends one hand-built ClientHello and reads only the
//! server's first answer: a ServerHello or an alert says something about the target, and so does
//! a close (FIN or RST) with nothing sent after the ClientHello arrived: the server read the offer
//! and ended the connection, which is how Windows SChannel refuses a disabled version
//! (v2-tls-probe-close). A timeout, garbage, a partial answer before the close, a suite that was
//! never offered or a failure before the ClientHello was written says nothing, and is
//! `Unanswered`. The connection is dropped after that answer; no handshake is completed.

use std::io;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

use super::connect::smtp_starttls;

const SCSV_EMPTY_RENEGOTIATION: u16 = 0x00FF;
const EXT_SERVER_NAME: u16 = 0x0000;
const EXT_SUPPORTED_GROUPS: u16 = 0x000A;
const EXT_EC_POINT_FORMATS: u16 = 0x000B;
const EXT_SIGNATURE_ALGORITHMS: u16 = 0x000D;
const EXT_SUPPORTED_VERSIONS: u16 = 0x002B;

/// Fatal alerts that refuse what the probe offered: handshake_failure, protocol_version,
/// insufficient_security. Any other alert (internal_error, decode_error, unrecognized_name) says
/// nothing about the version or suite and leaves the probe unanswered.
const REFUSING_ALERTS: &[u8] = &[40, 70, 71];
/// Upper bound on the bytes read from one probe connection.
const READ_CAP: usize = 20 * 1024;
/// Largest record payload TLS allows, with headroom for compression and padding.
const MAX_RECORD_PAYLOAD: usize = 16384 + 2048;

const SUITES_INSECURE: &[u16] = &[
    0x0005, 0x0004, 0xC011, 0xC007, 0x000A, 0xC012, 0xC008, 0x0016, 0x0009, 0x0015, 0x0003, 0x0006,
    0x0008, 0x0014, 0x0001, 0x0002, 0x003B, 0xC010, 0xC006, 0x0018, 0x001B, 0x0034, 0x003A, 0xC018,
    0xC019,
];
const SUITES_CBC: &[u16] = &[
    0xC013, 0xC014, 0xC009, 0xC00A, 0xC027, 0xC028, 0xC023, 0xC024, 0x0033, 0x0039, 0x0067, 0x006B,
];
const SUITES_STATIC_RSA: &[u16] = &[
    0x002F, 0x0035, 0x003C, 0x003D, 0x009C, 0x009D, 0x0041, 0x0084, 0x000A, 0x0005, 0x0004, 0x0009,
    0x0003, 0x0006, 0x0008, 0x0001, 0x0002, 0x003B,
];
const SUITES_TLS10: &[u16] = &[
    0xC013, 0xC014, 0xC009, 0xC00A, 0x0033, 0x0039, 0x002F, 0x0035, 0x000A, 0xC012, 0x0016, 0x0005,
    0x0004, 0xC011,
];
const SUITES_TLS12: &[u16] = &[
    0xC02F, 0xC030, 0xC02B, 0xC02C, 0xCCA8, 0xCCA9, 0x009E, 0x009F, 0xC027, 0xC028, 0xC023, 0xC024,
    0xC013, 0xC014, 0xC009, 0xC00A, 0x0033, 0x0039, 0x002F, 0x0035, 0x000A, 0xC012, 0x0016, 0x0005,
    0x0004, 0xC011,
];
const SUITES_SSL3: &[u16] = &[
    0x0005, 0x0004, 0x000A, 0x0016, 0x002F, 0x0035, 0x0033, 0x0039, 0x0009,
];

const SIGNATURE_ALGORITHMS: &[u16] = &[
    0x0403, 0x0503, 0x0603, 0x0804, 0x0805, 0x0806, 0x0401, 0x0501, 0x0601, 0x0203, 0x0201,
];
/// x25519, secp256r1, secp384r1, secp521r1.
const SUPPORTED_GROUPS: &[u16] = &[0x001D, 0x0017, 0x0018, 0x0019];

const SUITE_NAMES: &[(u16, &str)] = &[
    (0x0001, "TLS_RSA_WITH_NULL_MD5"),
    (0x0002, "TLS_RSA_WITH_NULL_SHA"),
    (0x0003, "TLS_RSA_EXPORT_WITH_RC4_40_MD5"),
    (0x0004, "TLS_RSA_WITH_RC4_128_MD5"),
    (0x0005, "TLS_RSA_WITH_RC4_128_SHA"),
    (0x0006, "TLS_RSA_EXPORT_WITH_RC2_CBC_40_MD5"),
    (0x0008, "TLS_RSA_EXPORT_WITH_DES40_CBC_SHA"),
    (0x0009, "TLS_RSA_WITH_DES_CBC_SHA"),
    (0x000A, "TLS_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0x0014, "TLS_DHE_RSA_EXPORT_WITH_DES40_CBC_SHA"),
    (0x0015, "TLS_DHE_RSA_WITH_DES_CBC_SHA"),
    (0x0016, "TLS_DHE_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0x0018, "TLS_DH_anon_WITH_RC4_128_MD5"),
    (0x001B, "TLS_DH_anon_WITH_3DES_EDE_CBC_SHA"),
    (0x002F, "TLS_RSA_WITH_AES_128_CBC_SHA"),
    (0x0033, "TLS_DHE_RSA_WITH_AES_128_CBC_SHA"),
    (0x0034, "TLS_DH_anon_WITH_AES_128_CBC_SHA"),
    (0x0035, "TLS_RSA_WITH_AES_256_CBC_SHA"),
    (0x0039, "TLS_DHE_RSA_WITH_AES_256_CBC_SHA"),
    (0x003A, "TLS_DH_anon_WITH_AES_256_CBC_SHA"),
    (0x003B, "TLS_RSA_WITH_NULL_SHA256"),
    (0x003C, "TLS_RSA_WITH_AES_128_CBC_SHA256"),
    (0x003D, "TLS_RSA_WITH_AES_256_CBC_SHA256"),
    (0x0041, "TLS_RSA_WITH_CAMELLIA_128_CBC_SHA"),
    (0x0067, "TLS_DHE_RSA_WITH_AES_128_CBC_SHA256"),
    (0x006B, "TLS_DHE_RSA_WITH_AES_256_CBC_SHA256"),
    (0x0084, "TLS_RSA_WITH_CAMELLIA_256_CBC_SHA"),
    (0x009C, "TLS_RSA_WITH_AES_128_GCM_SHA256"),
    (0x009D, "TLS_RSA_WITH_AES_256_GCM_SHA384"),
    (0x009E, "TLS_DHE_RSA_WITH_AES_128_GCM_SHA256"),
    (0x009F, "TLS_DHE_RSA_WITH_AES_256_GCM_SHA384"),
    (0xC006, "TLS_ECDHE_ECDSA_WITH_NULL_SHA"),
    (0xC007, "TLS_ECDHE_ECDSA_WITH_RC4_128_SHA"),
    (0xC008, "TLS_ECDHE_ECDSA_WITH_3DES_EDE_CBC_SHA"),
    (0xC009, "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA"),
    (0xC00A, "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA"),
    (0xC010, "TLS_ECDHE_RSA_WITH_NULL_SHA"),
    (0xC011, "TLS_ECDHE_RSA_WITH_RC4_128_SHA"),
    (0xC012, "TLS_ECDHE_RSA_WITH_3DES_EDE_CBC_SHA"),
    (0xC013, "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA"),
    (0xC014, "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA"),
    (0xC018, "TLS_ECDH_anon_WITH_AES_128_CBC_SHA"),
    (0xC019, "TLS_ECDH_anon_WITH_AES_256_CBC_SHA"),
    (0xC023, "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256"),
    (0xC024, "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384"),
    (0xC027, "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256"),
    (0xC028, "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384"),
    (0xC02B, "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256"),
    (0xC02C, "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384"),
    (0xC02F, "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"),
    (0xC030, "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"),
    (0xCCA8, "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256"),
    (0xCCA9, "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256"),
];

/// What one probe offers: a protocol version, or a group of suites at TLS 1.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    Ssl3,
    Tls10,
    Tls11,
    Tls12,
    SuitesInsecure,
    SuitesCbc,
    SuitesStaticRsa,
}

impl ProbeKind {
    pub const ALL: [ProbeKind; 7] = [
        ProbeKind::Ssl3,
        ProbeKind::Tls10,
        ProbeKind::Tls11,
        ProbeKind::Tls12,
        ProbeKind::SuitesInsecure,
        ProbeKind::SuitesCbc,
        ProbeKind::SuitesStaticRsa,
    ];

    /// The `client_version` the ClientHello carries.
    pub fn client_version(self) -> u16 {
        match self {
            ProbeKind::Ssl3 => 0x0300,
            ProbeKind::Tls10 => 0x0301,
            ProbeKind::Tls11 => 0x0302,
            ProbeKind::Tls12
            | ProbeKind::SuitesInsecure
            | ProbeKind::SuitesCbc
            | ProbeKind::SuitesStaticRsa => 0x0303,
        }
    }

    /// The cipher suites the ClientHello offers, without the renegotiation SCSV.
    pub fn suites(self) -> &'static [u16] {
        match self {
            ProbeKind::Ssl3 => SUITES_SSL3,
            ProbeKind::Tls10 | ProbeKind::Tls11 => SUITES_TLS10,
            ProbeKind::Tls12 => SUITES_TLS12,
            ProbeKind::SuitesInsecure => SUITES_INSECURE,
            ProbeKind::SuitesCbc => SUITES_CBC,
            ProbeKind::SuitesStaticRsa => SUITES_STATIC_RSA,
        }
    }

    /// The field name of this probe in the result.
    pub fn key(self) -> &'static str {
        match self {
            ProbeKind::Ssl3 => "ssl3",
            ProbeKind::Tls10 => "tls1_0",
            ProbeKind::Tls11 => "tls1_1",
            ProbeKind::Tls12 => "tls1_2",
            ProbeKind::SuitesInsecure => "suites_insecure",
            ProbeKind::SuitesCbc => "suites_cbc",
            ProbeKind::SuitesStaticRsa => "suites_static_rsa",
        }
    }

    fn is_suite_group(self) -> bool {
        matches!(
            self,
            ProbeKind::SuitesInsecure | ProbeKind::SuitesCbc | ProbeKind::SuitesStaticRsa
        )
    }
}

/// What a probe learned about the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The server answered with a ServerHello for what the probe offered.
    Accepted,
    /// The server answered with an alert or another version, or closed after the ClientHello.
    Refused,
    /// The server said nothing usable: not a verdict on the target.
    Unanswered,
}

/// The verdict of one probe with the facts that decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub verdict: Verdict,
    /// The protocol version the server chose, when it sent a ServerHello.
    pub version: Option<String>,
    /// The IANA name of the suite the server chose, when it sent a ServerHello.
    pub cipher_suite: Option<String>,
    pub detail: String,
}

impl ProbeOutcome {
    fn new(verdict: Verdict, detail: impl Into<String>) -> Self {
        Self {
            verdict,
            version: None,
            cipher_suite: None,
            detail: detail.into(),
        }
    }
}

/// How a probe connection ended when no answer had arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEnd {
    /// End of stream after the ClientHello was written.
    Eof,
    /// A reset after the ClientHello was written.
    Reset,
    Timeout,
    ConnectFailed(String),
    /// The connection failed after connecting but before the ClientHello was written (the
    /// STARTTLS upgrade or the write): the server never saw the offer.
    NotSent(String),
}

/// The probes of one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolProbes {
    pub ssl3: ProbeOutcome,
    pub tls1_0: ProbeOutcome,
    pub tls1_1: ProbeOutcome,
    pub tls1_2: ProbeOutcome,
    pub tls1_3: ProbeOutcome,
    pub suites_insecure: ProbeOutcome,
    pub suites_cbc: ProbeOutcome,
    pub suites_static_rsa: ProbeOutcome,
}

impl ProtocolProbes {
    /// The probes of an address after a handshake that negotiated `negotiated`: each probed kind
    /// from `outcome`, TLS 1.2 probed only after a TLS 1.3 handshake, TLS 1.3 from the handshake.
    pub fn from_handshake(
        negotiated: &str,
        mut outcome: impl FnMut(ProbeKind) -> ProbeOutcome,
    ) -> Self {
        let tls1_2 = if negotiated == "TLSv1.3" {
            outcome(ProbeKind::Tls12)
        } else if negotiated == "TLSv1.2" {
            ProbeOutcome {
                verdict: Verdict::Accepted,
                version: Some("TLSv1.2".into()),
                cipher_suite: None,
                detail: "handshake TLSv1.2".into(),
            }
        } else {
            ProbeOutcome::new(Verdict::Refused, format!("handshake {negotiated}"))
        };
        let tls1_3 = if negotiated == "TLSv1.3" {
            ProbeOutcome {
                verdict: Verdict::Accepted,
                version: Some("TLSv1.3".into()),
                cipher_suite: None,
                detail: "handshake TLSv1.3".into(),
            }
        } else {
            ProbeOutcome {
                verdict: Verdict::Refused,
                version: Some(negotiated.to_string()),
                cipher_suite: None,
                detail: format!("handshake {negotiated}"),
            }
        };
        Self {
            ssl3: outcome(ProbeKind::Ssl3),
            tls1_0: outcome(ProbeKind::Tls10),
            tls1_1: outcome(ProbeKind::Tls11),
            tls1_2,
            tls1_3,
            suites_insecure: outcome(ProbeKind::SuitesInsecure),
            suites_cbc: outcome(ProbeKind::SuitesCbc),
            suites_static_rsa: outcome(ProbeKind::SuitesStaticRsa),
        }
    }

    /// Every kind the address would have probed unanswered with `detail` (the probe deadline
    /// passed); TLS 1.2 and 1.3 keep their handshake-derived values.
    pub fn unanswered(negotiated: &str, detail: &str) -> Self {
        Self::from_handshake(negotiated, |_| {
            ProbeOutcome::new(Verdict::Unanswered, detail)
        })
    }

    /// The probes in evidence order, by key.
    pub fn entries(&self) -> [(&'static str, &ProbeOutcome); 8] {
        [
            ("ssl3", &self.ssl3),
            ("tls1_0", &self.tls1_0),
            ("tls1_1", &self.tls1_1),
            ("tls1_2", &self.tls1_2),
            ("tls1_3", &self.tls1_3),
            ("suites_insecure", &self.suites_insecure),
            ("suites_cbc", &self.suites_cbc),
            ("suites_static_rsa", &self.suites_static_rsa),
        ]
    }
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn push_u16_list(out: &mut Vec<u8>, list: &[u16]) {
    push_u16(out, (list.len() * 2) as u16);
    for v in list {
        push_u16(out, *v);
    }
}

fn push_extension(out: &mut Vec<u8>, kind: u16, data: &[u8]) {
    push_u16(out, kind);
    push_u16(out, data.len() as u16);
    out.extend_from_slice(data);
}

fn random() -> [u8; 32] {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut hasher = Sha256::new();
    hasher.update(nanos.to_be_bytes());
    hasher.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    out
}

fn is_ip_literal(sni: &str) -> bool {
    let bare = sni
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(sni);
    bare.parse::<IpAddr>().is_ok()
}

/// Build one TLS record holding one ClientHello for `kind`.
///
/// No `supported_versions`, no fallback SCSV; SSLv3 carries no extensions at all.
pub fn client_hello(kind: ProbeKind, sni: Option<&str>) -> Vec<u8> {
    let version = kind.client_version();
    let mut body = Vec::new();
    push_u16(&mut body, version);
    body.extend_from_slice(&random());
    body.push(0); // empty session id

    let suites: Vec<u16> = kind
        .suites()
        .iter()
        .copied()
        .chain([SCSV_EMPTY_RENEGOTIATION])
        .collect();
    push_u16_list(&mut body, &suites);
    body.extend_from_slice(&[1, 0]); // compression: null only

    if kind != ProbeKind::Ssl3 {
        let mut ext = Vec::new();
        if let Some(host) = sni.filter(|h| !h.is_empty() && !is_ip_literal(h)) {
            let mut data = Vec::new();
            push_u16(&mut data, (host.len() + 3) as u16);
            data.push(0); // host_name
            push_u16(&mut data, host.len() as u16);
            data.extend_from_slice(host.as_bytes());
            push_extension(&mut ext, EXT_SERVER_NAME, &data);
        }
        let mut groups = Vec::new();
        push_u16_list(&mut groups, SUPPORTED_GROUPS);
        push_extension(&mut ext, EXT_SUPPORTED_GROUPS, &groups);
        push_extension(&mut ext, EXT_EC_POINT_FORMATS, &[1, 0]);
        if version == 0x0303 {
            let mut algs = Vec::new();
            push_u16_list(&mut algs, SIGNATURE_ALGORITHMS);
            push_extension(&mut ext, EXT_SIGNATURE_ALGORITHMS, &algs);
        }
        push_u16(&mut body, ext.len() as u16);
        body.extend_from_slice(&ext);
    }

    let mut handshake = vec![
        1,
        (body.len() >> 16) as u8,
        (body.len() >> 8) as u8,
        body.len() as u8,
    ];
    handshake.extend_from_slice(&body);

    let record_version: u16 = if kind == ProbeKind::Ssl3 {
        0x0300
    } else {
        0x0301
    };
    let mut record = vec![22];
    push_u16(&mut record, record_version);
    push_u16(&mut record, handshake.len() as u16);
    record.extend_from_slice(&handshake);
    record
}

fn version_name(v: u16) -> String {
    match v {
        0x0300 => "SSLv3".into(),
        0x0301 => "TLSv1.0".into(),
        0x0302 => "TLSv1.1".into(),
        0x0303 => "TLSv1.2".into(),
        0x0304 => "TLSv1.3".into(),
        _ => format!("0x{v:04x}"),
    }
}

fn suite_name(code: u16) -> String {
    SUITE_NAMES
        .iter()
        .find(|(c, _)| *c == code)
        .map_or_else(|| format!("0x{code:04x}"), |(_, n)| (*n).to_string())
}

fn alert_name(desc: u8) -> String {
    match desc {
        0 => "close_notify".into(),
        10 => "unexpected_message".into(),
        20 => "bad_record_mac".into(),
        40 => "handshake_failure".into(),
        42 => "bad_certificate".into(),
        47 => "illegal_parameter".into(),
        50 => "decode_error".into(),
        70 => "protocol_version".into(),
        71 => "insufficient_security".into(),
        80 => "internal_error".into(),
        86 => "inappropriate_fallback".into(),
        90 => "user_canceled".into(),
        112 => "unrecognized_name".into(),
        n => n.to_string(),
    }
}

fn not_tls() -> ProbeOutcome {
    ProbeOutcome::new(Verdict::Unanswered, "not a TLS answer")
}

/// Parse a ServerHello message (handshake header included) into (version, suite).
fn parse_server_hello(msg: &[u8]) -> Option<(u16, u16)> {
    let body = msg.get(4..)?;
    let mut version = u16::from_be_bytes([*body.first()?, *body.get(1)?]);
    let sid_len = *body.get(34)? as usize;
    let at = 35 + sid_len;
    let suite = u16::from_be_bytes([*body.get(at)?, *body.get(at + 1)?]);
    let mut i = at + 3; // suite (2) + compression (1)
    if let (Some(&hi), Some(&lo)) = (body.get(i), body.get(i + 1)) {
        let total = u16::from_be_bytes([hi, lo]) as usize;
        i += 2;
        let end = (i + total).min(body.len());
        while i + 4 <= end {
            let kind = u16::from_be_bytes([body[i], body[i + 1]]);
            let len = u16::from_be_bytes([body[i + 2], body[i + 3]]) as usize;
            if kind == EXT_SUPPORTED_VERSIONS && len == 2 && i + 6 <= end {
                version = u16::from_be_bytes([body[i + 4], body[i + 5]]);
            }
            i += 4 + len;
        }
    }
    Some((version, suite))
}

fn judge_server_hello(kind: ProbeKind, version: u16, suite: u16) -> ProbeOutcome {
    let version_str = version_name(version);
    let suite_str = suite_name(suite);
    let not_offered = || {
        (
            Verdict::Unanswered,
            format!("suite not offered: 0x{suite:04x}"),
        )
    };
    let wrong_version = || (Verdict::Refused, format!("server_hello {version_str}"));
    let offered = kind.suites().contains(&suite);
    let (verdict, detail) = if kind.is_suite_group() {
        if !offered {
            not_offered()
        } else if !(0x0300..=0x0303).contains(&version) {
            wrong_version()
        } else {
            (
                Verdict::Accepted,
                format!("server_hello {version_str} {suite_str}"),
            )
        }
    } else if version != kind.client_version() {
        wrong_version()
    } else if !offered {
        not_offered()
    } else {
        (
            Verdict::Accepted,
            format!("server_hello {version_str} {suite_str}"),
        )
    };
    ProbeOutcome {
        verdict,
        version: Some(version_str),
        cipher_suite: Some(suite_str),
        detail,
    }
}

/// Decide from the bytes so far; `None` when more bytes are needed.
fn decide(kind: ProbeKind, bytes: &[u8]) -> Option<ProbeOutcome> {
    let mut pos = 0;
    let mut handshake = Vec::new();
    loop {
        let head = bytes.get(pos..pos + 5)?;
        if !(20..=23).contains(&head[0]) || head[1] != 3 {
            return Some(not_tls());
        }
        let len = u16::from_be_bytes([head[3], head[4]]) as usize;
        if len > MAX_RECORD_PAYLOAD {
            return Some(not_tls());
        }
        let payload = bytes.get(pos + 5..pos + 5 + len)?;
        pos += 5 + len;
        match head[0] {
            21 => {
                let (level, desc) = (*payload.first()?, *payload.get(1)?);
                // A warning before the ServerHello (unrecognized_name) is not the answer; keep
                // reading. close_notify is a close, not a verdict.
                if level == 1 && desc != 0 {
                    continue;
                }
                let verdict = if level == 2 && REFUSING_ALERTS.contains(&desc) {
                    Verdict::Refused
                } else {
                    Verdict::Unanswered
                };
                return Some(ProbeOutcome::new(
                    verdict,
                    format!("alert {}", alert_name(desc)),
                ));
            }
            22 => {
                handshake.extend_from_slice(payload);
                if let Some(&msg_type) = handshake.first()
                    && msg_type != 2
                {
                    return Some(not_tls());
                }
                if handshake.len() >= 4 {
                    let msg_len = ((handshake[1] as usize) << 16)
                        | ((handshake[2] as usize) << 8)
                        | handshake[3] as usize;
                    if handshake.len() >= 4 + msg_len {
                        return Some(match parse_server_hello(&handshake[..4 + msg_len]) {
                            Some((version, suite)) => judge_server_hello(kind, version, suite),
                            None => not_tls(),
                        });
                    }
                }
            }
            _ => return Some(not_tls()),
        }
    }
}

/// Read the server's first answer to a probe of `kind` from `bytes`.
///
/// A close or reset with no byte received refuses the offer (`closed`, `closed (reset)`): the
/// ClientHello was written, so the server read it and ended the connection instead of answering.
/// Bytes that decide nothing before the close leave the probe unanswered.
pub fn classify(kind: ProbeKind, bytes: &[u8], end: StreamEnd) -> ProbeOutcome {
    decide(kind, bytes).unwrap_or_else(|| match end {
        StreamEnd::Eof if bytes.is_empty() => ProbeOutcome::new(Verdict::Refused, "closed"),
        StreamEnd::Reset if bytes.is_empty() => {
            ProbeOutcome::new(Verdict::Refused, "closed (reset)")
        }
        StreamEnd::Eof => ProbeOutcome::new(Verdict::Unanswered, "closed without an answer"),
        StreamEnd::Reset => ProbeOutcome::new(Verdict::Unanswered, "reset"),
        StreamEnd::Timeout => ProbeOutcome::new(Verdict::Unanswered, "timed out"),
        StreamEnd::ConnectFailed(m) => {
            ProbeOutcome::new(Verdict::Unanswered, format!("connect failed: {m}"))
        }
        StreamEnd::NotSent(m) => ProbeOutcome::new(Verdict::Unanswered, format!("not sent: {m}")),
    })
}

fn end_of(e: &io::Error) -> StreamEnd {
    match e.kind() {
        io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe => StreamEnd::Reset,
        _ => StreamEnd::ConnectFailed(e.to_string()),
    }
}

/// Connect, send the hello and read until the answer is decided; `Err` when the stream ended
/// without one.
async fn exchange(
    kind: ProbeKind,
    ip: IpAddr,
    port: u16,
    sni: Option<&str>,
    buf: &mut Vec<u8>,
) -> Result<ProbeOutcome, StreamEnd> {
    let mut stream = TcpStream::connect((ip, port))
        .await
        .map_err(|e| StreamEnd::ConnectFailed(e.to_string()))?;
    let _ = stream.set_nodelay(true);
    if port == 25 || port == 587 {
        smtp_starttls(&mut stream)
            .await
            .map_err(|e| StreamEnd::NotSent(format!("starttls: {e}")))?;
    }
    stream
        .write_all(&client_hello(kind, sni))
        .await
        .map_err(|e| StreamEnd::NotSent(format!("write: {e}")))?;

    let mut chunk = [0u8; 4096];
    while buf.len() < READ_CAP {
        match stream.read(&mut chunk).await {
            Ok(0) => return Err(StreamEnd::Eof),
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(outcome) = decide(kind, buf) {
                    return Ok(outcome);
                }
            }
            Err(e) => return Err(end_of(&e)),
        }
    }
    Err(StreamEnd::Eof)
}

async fn probe(
    kind: ProbeKind,
    ip: IpAddr,
    port: u16,
    sni: Option<String>,
    timeout: Duration,
) -> ProbeOutcome {
    let mut buf = Vec::new();
    let end =
        match tokio::time::timeout(timeout, exchange(kind, ip, port, sni.as_deref(), &mut buf))
            .await
        {
            Ok(Ok(outcome)) => return outcome,
            Ok(Err(end)) => end,
            Err(_) => StreamEnd::Timeout,
        };
    classify(kind, &buf, end)
}

/// Probe one address: SSLv3, TLS 1.0, TLS 1.1 and the three suite groups over one connection
/// each, concurrently. TLS 1.2 is probed only when the main handshake negotiated TLS 1.3;
/// otherwise TLS 1.2 and 1.3 are read from `negotiated`.
pub async fn probe_address(
    ip: IpAddr,
    port: u16,
    sni: Option<&str>,
    negotiated: &str,
    timeout: Duration,
) -> ProtocolProbes {
    let mut set = JoinSet::new();
    for kind in ProbeKind::ALL {
        if kind == ProbeKind::Tls12 && negotiated != "TLSv1.3" {
            continue;
        }
        let sni = sni.map(str::to_owned);
        set.spawn(async move { (kind, probe(kind, ip, port, sni, timeout).await) });
    }
    let mut results = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(r) = joined {
            results.push(r);
        }
    }
    ProtocolProbes::from_handshake(negotiated, |kind| {
        results
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, o)| o.clone())
            .unwrap_or_else(|| ProbeOutcome::new(Verdict::Unanswered, "probe failed"))
    })
}
