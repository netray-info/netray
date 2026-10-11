// Trust is verified against the root store only; the hostname plays no part in it.
//
// Table rows: C26 (hostname differs from the leaf's SAN), C27 (no hostname), C28 (self-signed
// leaf that is not in the store).

use netray_tls::tls::chain::parse_chain;
use netray_tls::validate::chain_trust::validate_chain;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;

const LEAF_SAN: &str = "www.example.com";

struct Pki {
    ca_der: CertificateDer<'static>,
    leaf_der: CertificateDer<'static>,
    self_signed_der: CertificateDer<'static>,
}

fn build_pki() -> Pki {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, &ca_key);

    let leaf_key = KeyPair::generate().unwrap();
    let leaf_params = CertificateParams::new(vec![LEAF_SAN.to_string()]).unwrap();
    let leaf = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

    let ss_key = KeyPair::generate().unwrap();
    let ss_params = CertificateParams::new(vec![LEAF_SAN.to_string()]).unwrap();
    let ss = ss_params.self_signed(&ss_key).unwrap();

    Pki {
        ca_der: CertificateDer::from(ca_cert.der().to_vec()),
        leaf_der: CertificateDer::from(leaf.der().to_vec()),
        self_signed_der: CertificateDer::from(ss.der().to_vec()),
    }
}

#[test]
fn trust_is_decided_by_the_root_store_not_the_hostname() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let pki = build_pki();

    let mut roots = RootCertStore::empty();
    roots.add(pki.ca_der.clone()).unwrap();

    // (label, raw chain, hostname, chain_trusted, leaf_covers_hostname)
    let rows: Vec<(&str, Vec<CertificateDer<'static>>, Option<&str>, bool, bool)> = vec![
        (
            "C26 other hostname",
            vec![pki.leaf_der.clone()],
            Some("other.example.com"),
            true,
            false,
        ),
        (
            "C27 no hostname",
            vec![pki.leaf_der.clone()],
            None,
            true,
            false,
        ),
        (
            "C28 self-signed not in store",
            vec![pki.self_signed_der.clone()],
            Some(LEAF_SAN),
            false,
            true,
        ),
    ];

    for (label, raw, hostname, trusted, covers) in rows {
        let chain = parse_chain(&raw);
        let result = validate_chain(&chain, hostname, &roots, &raw);
        assert_eq!(
            result.chain_trusted, trusted,
            "{label}: chain_trusted (reason: {:?})",
            result.chain_trust_reason
        );
        assert_eq!(
            result.leaf_covers_hostname, covers,
            "{label}: leaf_covers_hostname"
        );
    }
}
