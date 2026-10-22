//! Contract test: lens runs the DNS module (`netray_dns`) on the golden its prism writes; the TLS
//! module (`netray_tls`) and `netray_http` translate the TLS and HTTP goldens (all sections run
//! in-process).
//!
//! Each backend's own `tests/contract_golden.rs` writes its golden under
//! `tests/fixtures/contracts/` from its real response types. Here the DNS and TLS modules run
//! the goldens through the engine; both must produce checks and no error and carry values that
//! come from the golden rather than defaults.
//!
//! Copied from crates/lens/tests/contract_backends.rs (spec v2-cutover, Phase 6); dropped as V1:
//! lens's `BackendExtra` (the DNS `resolved_ips` and the TLS `raw_headline`); the golden's values
//! they carried are asserted on the `dns.a`, `tls.handshake@…` and `tls.chain@…` evidence blocks
//! lens built them from.

use std::net::IpAddr;
use std::time::Duration;

use crate::common;

use common::{dns_golden_raw, golden, run_dns, run_tls, tls_golden};
use netray_engine::SectionOutcome;
use netray_engine::domain_run::CheckVerdict;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::translate;
use netray_model::Status;
use netray_model::evidence::{Block, Shape, Value};

const TIMEOUT: Duration = Duration::from_secs(5);

/// The addresses of the `dns.a` records block.
fn a_records(evidence: &[Block]) -> Vec<IpAddr> {
    let Some(Shape::Records(records)) = evidence
        .iter()
        .find(|b| b.id.to_string() == "dns.a")
        .map(|b| &b.shape)
    else {
        return Vec::new();
    };
    records
        .iter()
        .filter_map(|r| match (&r.rtype, &r.data) {
            (Value::Text(t), Value::Text(data)) if t == "A" => data.parse().ok(),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn parses_prism_golden() {
    let result = run_dns(dns_golden_raw("prism.sse"), TIMEOUT)
        .await
        .expect("the DNS module must run on prism's golden without a section error");

    assert!(
        !result.checks.is_empty(),
        "no checks parsed from prism's golden"
    );
    let ns = result
        .checks
        .iter()
        .find(|c| c.name == "ns")
        .expect("lint finding `ns` missing");
    assert_eq!(ns.verdict, CheckVerdict::Pass);
    let caa = result
        .checks
        .iter()
        .find(|c| c.name == "caa")
        .expect("lint finding `caa` missing");
    assert_eq!(caa.verdict, CheckVerdict::Warn);
    let resolved_ips = a_records(&result.evidence);
    assert!(
        resolved_ips.iter().any(|ip| ip.to_string() == "192.0.2.10"),
        "resolved IPs {resolved_ips:?} do not come from the golden's A batch",
    );
}

#[tokio::test]
async fn parses_tlsight_golden() {
    let result = run_tls(tls_golden("tlsight-inspect.json"), TIMEOUT)
        .await
        .expect("the TLS module must run on tlsight's golden without a section error");

    assert!(
        !result.checks.is_empty(),
        "no checks translated from tlsight's golden"
    );
    let ocsp = result
        .checks
        .iter()
        .find(|c| c.name == "ocsp_stapled")
        .expect("check `ocsp_stapled` missing");
    // v3 reads ocsp_stapled once from the summary: an unstapled response with an OCSP URL warns.
    assert_eq!(ocsp.verdict, CheckVerdict::Warn);
    // HTTP owns hsts and https_redirect; the TLS section no longer copies tlsight's
    // hostname checks (grade-integrity requirement 9).
    for owned_by_http in ["hsts", "https_redirect"] {
        assert!(
            result.checks.iter().all(|c| c.name != owned_by_http),
            "TLS section must not carry `{owned_by_http}`"
        );
    }
    // lens's headline read the handshake version and the leaf's days remaining.
    let versions: Vec<&Value> = result
        .evidence
        .iter()
        .filter(|b| b.id.to_string().starts_with("tls.handshake@"))
        .filter_map(|b| match &b.shape {
            Shape::Fields(f) => f.iter().find(|(k, _)| k == "version").map(|(_, v)| v),
            _ => None,
        })
        .collect();
    assert!(
        versions.contains(&&Value::Text("TLSv1.3".into())),
        "handshake versions {versions:?} do not carry the golden's TLSv1.3",
    );
    let days: Vec<&Value> = result
        .evidence
        .iter()
        .filter(|b| b.id.to_string().starts_with("tls.chain@"))
        .filter_map(|b| match &b.shape {
            Shape::Chain(certs) => certs
                .first()?
                .iter()
                .find(|(k, _)| k == "days_remaining")
                .map(|(_, v)| v),
            _ => None,
        })
        .collect();
    assert!(
        days.contains(&&Value::Int(60)),
        "leaf days remaining {days:?} do not carry the golden's 60",
    );
}

#[test]
fn netray_http_translates_spectra_golden() {
    let resp: InspectResponse = serde_json::from_str(&golden("spectra-inspect.json"))
        .expect("netray-http must decode spectra's golden");

    let (checks, evidence) = match translate(&resp) {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    };

    assert!(
        !checks.is_empty(),
        "no checks translated from spectra's golden"
    );
    let check = |id: &str| {
        checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("check {id} missing"))
    };
    assert_eq!(check("http.https_redirect").status, Status::Pass);
    let headers = check("http.csp");
    assert_eq!(headers.status, Status::Warn);
    assert!(
        headers
            .findings
            .iter()
            .any(|m| m.contains("Content-Security-Policy")),
        "findings {:?} do not come from the golden's csp check",
        headers.findings
    );
    let server = evidence
        .iter()
        .find(|b| b.id.to_string() == "http.server")
        .expect("http.server block missing");
    let Shape::Fields(fields) = &server.shape else {
        panic!("http.server is not fields: {server:?}");
    };
    assert!(fields.contains(&("status_code".into(), Value::Int(200))));
    assert!(fields.contains(&("org".into(), Value::Text("Example Hosting".into()))));
}
