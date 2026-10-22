// specs/features/v2-tls-versions-coop, C1, C4, C5, C6, C7, C8 (R1): the probe's ClientHello.
// Contract for the coder: `netray_tls::tls::probe` holds `ProbeKind` (`ALL`, `client_version`,
// `suites`, `key`) and `client_hello(kind, sni) -> Vec<u8>`, one TLS record with one
// ClientHello. The parser below reads the bytes back; it shares nothing with the builder.

use netray_tls::tls::probe::{ProbeKind, client_hello};

const SCSV_RENEG: u16 = 0x00FF;
const SCSV_FALLBACK: u16 = 0x5600;
const EXT_SERVER_NAME: u16 = 0x0000;
const EXT_SIG_ALGS: u16 = 0x000D;
const EXT_SUPPORTED_VERSIONS: u16 = 0x002B;

struct Hello {
    record_type: u8,
    record_version: u16,
    handshake_type: u8,
    client_version: u16,
    suites: Vec<u16>,
    /// None: the body ends right after the compression methods.
    extensions: Option<Vec<(u16, Vec<u8>)>>,
}

impl Hello {
    fn has_ext(&self, t: u16) -> bool {
        self.extensions
            .as_ref()
            .is_some_and(|e| e.iter().any(|(x, _)| *x == t))
    }

    fn sni_host(&self) -> Option<String> {
        let (_, data) = self
            .extensions
            .as_ref()?
            .iter()
            .find(|(t, _)| *t == EXT_SERVER_NAME)?;
        // list length (2), name type (1), name length (2), name
        assert_eq!(u16_at(data, 0) as usize, data.len() - 2);
        assert_eq!(data[2], 0, "name type host_name");
        let n = u16_at(data, 3) as usize;
        assert_eq!(data.len(), 5 + n);
        Some(String::from_utf8(data[5..].to_vec()).unwrap())
    }
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([b[i], b[i + 1]])
}

fn parse(b: &[u8]) -> Hello {
    assert!(b.len() > 9, "record too short");
    assert_eq!(b.len() - 5, u16_at(b, 3) as usize, "record length");
    let body = &b[5..];
    let hs_len = ((body[1] as usize) << 16) | ((body[2] as usize) << 8) | body[3] as usize;
    assert_eq!(body.len() - 4, hs_len, "handshake length");
    let h = &body[4..];
    let client_version = u16_at(h, 0);
    let mut i = 2 + 32;
    let sid = h[i] as usize;
    i += 1 + sid;
    let cs_len = u16_at(h, i) as usize;
    i += 2;
    assert_eq!(cs_len % 2, 0);
    let suites = (0..cs_len / 2).map(|k| u16_at(h, i + 2 * k)).collect();
    i += cs_len;
    let comp = h[i] as usize;
    i += 1 + comp;
    let extensions = if i == h.len() {
        None
    } else {
        let total = u16_at(h, i) as usize;
        i += 2;
        assert_eq!(h.len() - i, total, "extensions length");
        let mut v = Vec::new();
        while i < h.len() {
            let t = u16_at(h, i);
            let l = u16_at(h, i + 2) as usize;
            v.push((t, h[i + 4..i + 4 + l].to_vec()));
            i += 4 + l;
        }
        assert_eq!(i, h.len());
        Some(v)
    };
    Hello {
        record_type: b[0],
        record_version: u16_at(b, 1),
        handshake_type: body[0],
        client_version,
        suites,
        extensions,
    }
}

fn hello(kind: ProbeKind, sni: Option<&str>) -> Hello {
    parse(&client_hello(kind, sni))
}

#[test]
fn every_kind_builds_one_well_formed_client_hello() {
    // (kind, client version, key)
    let table = [
        (ProbeKind::Ssl3, 0x0300, "ssl3"),
        (ProbeKind::Tls10, 0x0301, "tls1_0"),
        (ProbeKind::Tls11, 0x0302, "tls1_1"),
        (ProbeKind::Tls12, 0x0303, "tls1_2"),
        (ProbeKind::SuitesInsecure, 0x0303, "suites_insecure"),
        (ProbeKind::SuitesCbc, 0x0303, "suites_cbc"),
        (ProbeKind::SuitesStaticRsa, 0x0303, "suites_static_rsa"),
    ];
    assert_eq!(ProbeKind::ALL.len(), 7);
    for (kind, version, key) in table {
        assert!(ProbeKind::ALL.contains(&kind), "{key}");
        assert_eq!(kind.client_version(), version, "{key}");
        assert_eq!(kind.key(), key);
        let h = hello(kind, Some("example.com"));
        assert_eq!(h.record_type, 22, "{key}");
        let rec = if kind == ProbeKind::Ssl3 {
            0x0300
        } else {
            0x0301
        };
        assert_eq!(h.record_version, rec, "{key} record version");
        assert_eq!(h.handshake_type, 1, "{key}");
        assert_eq!(h.client_version, version, "{key}");
        assert!(h.suites.contains(&SCSV_RENEG), "{key} offers 0x00FF");
        assert!(!h.suites.contains(&SCSV_FALLBACK), "{key} no fallback SCSV");
        assert!(
            !h.has_ext(EXT_SUPPORTED_VERSIONS),
            "{key} no supported_versions"
        );
        assert_eq!(
            h.has_ext(EXT_SIG_ALGS),
            version == 0x0303,
            "{key} signature_algorithms only at 0x0303"
        );
        let offered: Vec<u16> = h
            .suites
            .iter()
            .copied()
            .filter(|s| *s != SCSV_RENEG)
            .collect();
        assert_eq!(
            offered,
            kind.suites(),
            "{key} offers the kind's suites plus 0x00FF"
        );
        assert_eq!(
            h.has_ext(EXT_SERVER_NAME),
            kind != ProbeKind::Ssl3,
            "{key} SNI"
        );
    }
}

#[test]
fn tls10_with_sni_example_com() {
    let h = hello(ProbeKind::Tls10, Some("example.com"));
    assert_eq!(h.record_type, 22);
    assert_eq!(h.record_version, 0x0301);
    assert_eq!(h.handshake_type, 1);
    assert_eq!(h.client_version, 0x0301);
    assert_eq!(h.sni_host().as_deref(), Some("example.com"));
    assert!(!h.has_ext(EXT_SUPPORTED_VERSIONS));
    assert!(!h.has_ext(EXT_SIG_ALGS));
    assert!(!h.suites.contains(&SCSV_FALLBACK));
}

#[test]
fn ssl3_has_no_extensions() {
    let h = hello(ProbeKind::Ssl3, Some("example.com"));
    assert_eq!(h.record_version, 0x0300);
    assert_eq!(h.client_version, 0x0300);
    assert!(
        h.extensions.is_none(),
        "body ends right after compression methods"
    );
}

#[test]
fn tls12_carries_signature_algorithms() {
    let h = hello(ProbeKind::Tls12, Some("example.com"));
    assert_eq!(h.client_version, 0x0303);
    assert!(h.has_ext(EXT_SIG_ALGS));
}

#[test]
fn no_server_name_for_an_ip_address_or_without_sni() {
    for kind in ProbeKind::ALL {
        for sni in [Some("192.0.2.1"), None] {
            let h = hello(kind, sni);
            assert!(!h.has_ext(EXT_SERVER_NAME), "{} {sni:?}", kind.key());
        }
    }
}

#[test]
fn suite_groups_hold_their_families() {
    let insecure = ProbeKind::SuitesInsecure.suites();
    for code in [0x0005, 0x000A, 0x0009, 0x0003, 0x0002, 0x0034] {
        assert!(insecure.contains(&code), "insecure holds {code:#06x}");
    }

    let static_rsa_list = [0x002F, 0x0035, 0x003C, 0x003D, 0x009C, 0x009D];
    let cbc = ProbeKind::SuitesCbc.suites();
    for code in [0xC013, 0x0033] {
        assert!(cbc.contains(&code), "cbc holds {code:#06x}");
    }
    for code in static_rsa_list.into_iter().chain([0xC02F]) {
        assert!(!cbc.contains(&code), "cbc holds no {code:#06x}");
    }

    let srsa = ProbeKind::SuitesStaticRsa.suites();
    for code in [0x002F, 0x009C] {
        assert!(srsa.contains(&code), "static_rsa holds {code:#06x}");
    }
    for code in srsa {
        assert_ne!(
            code & 0xFF00,
            0xC000,
            "static_rsa holds no ECDHE: {code:#06x}"
        );
        assert!(
            ![0x0033, 0x0039, 0x0067, 0x006B, 0x009E].contains(code),
            "static_rsa holds no DHE: {code:#06x}"
        );
    }
}
