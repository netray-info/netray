// Golden test for the `GET /api/inspect?h=<host>` response lens consumes.
//
// The golden `tests/fixtures/contracts/tlsight-inspect.json` is written here from tlsight's
// real `InspectResponse` (and the types it nests), serialized by axum's `Json` exactly as the
// route does. lens's `tests/contract_backends.rs` reads the same file.
//
// UPDATE_GOLDEN=1 cargo test -p netray-tls --test contract_golden   writes the golden.

use std::path::PathBuf;

use axum::Json;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use netray_tls::quality::assess_port;
use netray_tls::quality::types::Category;
use netray_tls::quality::{HealthCheck, PortQualityResult, QualityResult};
use netray_tls::routes::{CaaInfo, DnsContext, InspectResponse, PortResult};
use netray_tls::tls::chain::CertInfo;
use netray_tls::tls::ocsp::OcspInfo;
use netray_tls::tls::params::TlsParams;
use netray_tls::tls::{InspectionError, IpInspectionResult};
use netray_tls::validate::{CheckStatus, Summary, SummaryChecks, ValidationResult};

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "golden {} is missing; run with UPDATE_GOLDEN=1 to write it",
            path.display()
        )
    });
    assert!(
        committed == actual,
        "golden {} differs from what tlsight produces now; if the change is intended, run with UPDATE_GOLDEN=1 and commit the result",
        path.display()
    );
}

fn check(
    id: &str,
    category: Category,
    status: CheckStatus,
    label: &str,
    detail: &str,
) -> HealthCheck {
    HealthCheck {
        id: id.to_string(),
        category,
        status,
        label: label.to_string(),
        detail: detail.to_string(),
    }
}

fn response() -> InspectResponse {
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
    let ip = IpInspectionResult {
        ip: "192.0.2.10".to_string(),
        ip_version: "v4".to_string(),
        tls: Some(TlsParams {
            version: "TLSv1.3".to_string(),
            cipher_suite: "TLS13_AES_256_GCM_SHA384".to_string(),
            alpn: Some("h2".to_string()),
            sni: Some("example.com".to_string()),
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
    };
    let port_checks = vec![
        check(
            "tls_reachable",
            Category::Protocol,
            CheckStatus::Pass,
            "TLS reachable",
            "TLS handshake succeeded",
        ),
        check(
            "chain_trusted",
            Category::Certificate,
            CheckStatus::Pass,
            "Chain trusted",
            "Chain validates to a trusted root",
        ),
        check(
            "expiry_window",
            Category::Certificate,
            CheckStatus::Warn,
            "Expiry window",
            "Certificate expires in 60 days",
        ),
        check(
            "tls_version",
            Category::Protocol,
            CheckStatus::Pass,
            "TLS version",
            "TLSv1.3",
        ),
        check(
            "ocsp_stapled",
            Category::Configuration,
            CheckStatus::Fail,
            "OCSP stapling",
            "No OCSP response stapled",
        ),
    ];
    InspectResponse {
        request_id: "contract-golden".to_string(),
        hostname: "example.com".to_string(),
        input_mode: "hostname",
        summary: Summary {
            verdict: CheckStatus::Warn,
            checks: SummaryChecks {
                chain_trusted: CheckStatus::Pass,
                not_expired: CheckStatus::Pass,
                hostname_match: CheckStatus::Pass,
                caa_compliant: CheckStatus::Skip,
                dane_valid: CheckStatus::Skip,
                ct_logged: CheckStatus::Skip,
                ocsp_stapled: CheckStatus::Fail,
                consistency: CheckStatus::Pass,
            },
        },
        ports: vec![PortResult {
            port: 443,
            ips: vec![ip],
            consistency: None,
            validation: None,
            tlsa: None,
            quality: Some(PortQualityResult {
                verdict: CheckStatus::Fail,
                checks: port_checks,
            }),
            error: None,
        }],
        dns: Some(DnsContext {
            caa: Some(CaaInfo {
                records: vec![],
                issuer_allowed: None,
                issuewild_present: false,
            }),
            resolved_ips: vec!["192.0.2.10".to_string()],
        }),
        quality: Some(QualityResult {
            verdict: CheckStatus::Pass,
            checks: vec![check(
                "hsts",
                Category::Configuration,
                CheckStatus::Pass,
                "HSTS",
                "max-age=31536000",
            )],
            hsts: None,
            https_redirect: None,
        }),
        warnings: vec![],
        skipped_ips: vec![],
        duration_ms: 123,
    }
}

#[tokio::test]
async fn tlsight_inspect_response_matches_golden() {
    let resp = Json(response()).into_response();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut body = serde_json::to_string_pretty(&value).unwrap();
    body.push('\n');
    assert_golden("tlsight-inspect.json", &body);
}

/// Wraps a port assessed by the real `assess_port` over failed IPs in the response lens reads.
fn failed_response(code: &str, message: &str) -> InspectResponse {
    let ips: Vec<IpInspectionResult> = [("192.0.2.10", "v4"), ("2001:db8::10", "v6")]
        .into_iter()
        .map(|(ip, v)| IpInspectionResult {
            ip: ip.to_string(),
            ip_version: v.to_string(),
            tls: None,
            chain: None,
            validation: None,
            ct: None,
            enrichment: None,
            error: Some(InspectionError {
                code: code.to_string(),
                message: message.to_string(),
            }),
            raw_certs: None,
        })
        .collect();
    let quality = assess_port(
        &ips,
        443,
        true,
        CheckStatus::Skip,
        CheckStatus::Skip,
        false,
        None,
        false,
        "example.com",
    );
    let mut resp = response();
    resp.ports = vec![PortResult {
        port: 443,
        ips,
        consistency: None,
        validation: None,
        tlsa: None,
        quality: Some(quality),
        error: None,
    }];
    resp
}

async fn body_of(resp: InspectResponse) -> String {
    let bytes = Json(resp)
        .into_response()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut body = serde_json::to_string_pretty(&value).unwrap();
    body.push('\n');
    body
}

#[tokio::test]
async fn tlsight_unreachable_response_matches_golden() {
    let body = body_of(failed_response("HANDSHAKE_FAILED", "connection refused")).await;
    assert_golden("tlsight-unreachable.json", &body);
}

#[tokio::test]
async fn tlsight_not_tested_response_matches_golden() {
    let body = body_of(failed_response(
        "NOT_TESTED_FROM_HERE",
        "Network is unreachable",
    ))
    .await;
    assert_golden("tlsight-not-tested.json", &body);
}
