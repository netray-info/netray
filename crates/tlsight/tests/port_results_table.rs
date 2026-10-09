// Pinning tables for tlsight's per-port results when some or all IPs fail.
//
// Table 1 drives the real `quality::assess_port` and records, per row, the port verdict and
// every check id with its verdict, exactly as today's code emits them. Table 2 drives the real
// `tls::inspect_ip` against a closed local port and records the error code it reports.
//
// The tables are pinning tables: they are green against today's code and record what it does,
// not what it ought to do. Later changes to how failed IPs are classified or reported will
// change rows; when they do, update the row and say why in the commit.
//
// Constant across the `assess_port` rows: port 443, hostname input `example.com`, CAA and
// DANE `Skip`, no stapled OCSP response, no consistency result, CT lookup disabled.

use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::time::Duration;

use tlsight::quality::assess_port;
use tlsight::tls::chain::CertInfo;
use tlsight::tls::ocsp::OcspInfo;
use tlsight::tls::params::TlsParams;
use tlsight::tls::{InspectionError, IpInspectionResult, inspect_ip};
use tlsight::validate::{CheckStatus, ValidationResult};

const HOSTNAME: &str = "example.com";

fn failed(ip: &str, version: &str, message: &str) -> IpInspectionResult {
    IpInspectionResult {
        ip: ip.to_string(),
        ip_version: version.to_string(),
        tls: None,
        chain: None,
        validation: None,
        ct: None,
        enrichment: None,
        error: Some(InspectionError {
            code: "HANDSHAKE_FAILED".to_string(),
            message: message.to_string(),
        }),
        raw_certs: None,
    }
}

fn succeeded(ip: &str, version: &str) -> IpInspectionResult {
    let leaf = CertInfo {
        position: "leaf".to_string(),
        subject: "CN=example.com".to_string(),
        issuer: "CN=Example Test CA".to_string(),
        sans: vec!["example.com".to_string(), "www.example.com".to_string()],
        serial: "01:02:03".to_string(),
        not_before: "2030-01-01T00:00:00Z".to_string(),
        not_after: "2030-04-01T00:00:00Z".to_string(),
        days_remaining: 60,
        key_type: "ECDSA".to_string(),
        key_size: 256,
        signature_algorithm: "ecdsa-with-SHA256".to_string(),
        fingerprint_sha256: "AA:BB".to_string(),
        fingerprint_sha1: "CC:DD".to_string(),
        lifetime_days: 90,
        is_expired: false,
        is_self_signed: false,
        cert_policy: "DV".to_string(),
        ocsp_url: Some("http://ocsp.example.com".to_string()),
        ca_issuers_url: None,
    };
    IpInspectionResult {
        ip: ip.to_string(),
        ip_version: version.to_string(),
        tls: Some(TlsParams {
            version: "TLSv1.3".to_string(),
            cipher_suite: "TLS13_AES_256_GCM_SHA384".to_string(),
            alpn: Some("h2".to_string()),
            sni: Some(HOSTNAME.to_string()),
            key_exchange_group: Some("X25519".to_string()),
            ocsp: OcspInfo {
                stapled: false,
                status: None,
                this_update: None,
                next_update: None,
            },
            ocsp_live: None,
            handshake_ms: 42,
            starttls: None,
            ech_advertised: None,
        }),
        chain: Some(vec![leaf]),
        validation: Some(ValidationResult {
            chain_trusted: true,
            chain_trust_reason: None,
            terminates_at_self_signed: false,
            chain_order_correct: true,
            leaf_covers_hostname: true,
            any_expired: false,
            any_not_yet_valid: false,
            weakest_signature: "ecdsa-with-SHA256".to_string(),
            earliest_expiry: "2030-04-01T00:00:00Z".to_string(),
            earliest_expiry_days: 60,
        }),
        ct: None,
        enrichment: None,
        error: None,
        raw_certs: None,
    }
}

struct Row {
    name: &'static str,
    ips: Vec<IpInspectionResult>,
    verdict: CheckStatus,
    checks: Vec<(&'static str, CheckStatus)>,
}

fn rows() -> Vec<Row> {
    use CheckStatus::{Pass, Skip, Warn};
    let refused = "connection refused";
    let unreachable = "Network is unreachable (os error 51)";
    // Check ids and verdicts for a port whose first successful IP carries the chain above.
    // `ocsp_stapled` warns (responder URL present, nothing stapled); CT lookup is off, so
    // `ct_logged` skips.
    let one_ok_ip = vec![
        ("chain_trusted", Pass),
        ("not_expired", Pass),
        ("hostname_match", Pass),
        ("chain_complete", Pass),
        ("strong_signature", Pass),
        ("key_strength", Pass),
        ("expiry_window", Pass),
        ("cert_lifetime", Pass),
        ("san_quality", Pass),
        ("aia_reachability", Pass),
        ("tls_version", Pass),
        ("forward_secrecy", Pass),
        ("aead_cipher", Pass),
        ("ct_logged", Skip),
        ("ocsp_stapled", Warn),
        ("caa_compliant", Skip),
        ("dane_valid", Skip),
        ("consistency", Skip),
        ("alpn_consistency", Skip),
        ("ech_advertised", Skip),
    ];
    let mut two_ok_ips = one_ok_ip.clone();
    for c in &mut two_ok_ips {
        if c.0 == "alpn_consistency" {
            c.1 = ALPN_TWO_IPS;
        }
    }
    vec![
        // C10: every IP failed on the target side.
        Row {
            name: "C10 every IP failed (v4 + v6 refused)",
            ips: vec![
                failed("192.0.2.10", "v4", refused),
                failed("2001:db8::10", "v6", refused),
            ],
            verdict: Skip,
            checks: vec![],
        },
        // C11: v6 failed locally, v4 succeeded.
        Row {
            name: "C11 v6 unreachable, v4 succeeded",
            ips: vec![
                succeeded("192.0.2.10", "v4"),
                failed("2001:db8::10", "v6", unreachable),
            ],
            verdict: Warn,
            checks: one_ok_ip,
        },
        // C12: every IP succeeded.
        Row {
            name: "C12 every IP succeeded",
            ips: vec![
                succeeded("192.0.2.10", "v4"),
                succeeded("2001:db8::10", "v6"),
            ],
            verdict: Warn,
            checks: two_ok_ips,
        },
    ]
}

/// With two successful IPs the ALPN check has something to compare and passes.
const ALPN_TWO_IPS: CheckStatus = CheckStatus::Pass;

#[test]
fn port_quality_results_table() {
    for row in rows() {
        let q = assess_port(
            &row.ips,
            443,
            true,
            CheckStatus::Skip,
            CheckStatus::Skip,
            false,
            None,
            false,
            HOSTNAME,
        );
        let got: Vec<(&str, CheckStatus)> =
            q.checks.iter().map(|c| (c.id.as_str(), c.status)).collect();
        assert_eq!(q.verdict, row.verdict, "{}: verdict", row.name);
        assert_eq!(got, row.checks, "{}: checks", row.name);
    }
}

/// C13: a closed port yields a handshake error with today's code.
#[tokio::test]
async fn inspect_ip_closed_port_error_code() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let r = inspect_ip(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        Some(HOSTNAME),
        Duration::from_secs(5),
    )
    .await;
    let err = r.error.expect("closed port must yield an error");
    assert_eq!(err.code, "HANDSHAKE_FAILED");
    assert!(r.tls.is_none() && r.chain.is_none() && r.validation.is_none());
}
