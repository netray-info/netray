//! Chain completeness: `chain_complete` and `aia_reachability` on real rcgen chains.
//!
//! A served chain is complete when its last certificate is self-signed, is itself
//! a trust anchor, or is issued by one (subject-name comparison) -- independent
//! of whether the leaf is expired.

use netray_tls::quality::checks::{check_aia_reachability, check_chain_complete};
use netray_tls::tls::chain::parse_chain;
use netray_tls::validate::CheckStatus;
use netray_tls::validate::chain_trust::validate_chain;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
};
use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;

const HOST: &str = "example.com";

struct Ca {
    der: CertificateDer<'static>,
    issuer: Issuer<'static, KeyPair>,
}

fn named(cn: &str, sans: Vec<String>) -> CertificateParams {
    let mut params = CertificateParams::new(sans).unwrap();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    params.distinguished_name = dn;
    params
}

fn ca_params(cn: &str) -> CertificateParams {
    let mut params = named(cn, vec![]);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
}

fn root(cn: &str) -> Ca {
    let key = KeyPair::generate().unwrap();
    let params = ca_params(cn);
    let cert = params.self_signed(&key).unwrap();
    Ca {
        der: cert.der().clone(),
        issuer: Issuer::new(params, key),
    }
}

fn intermediate(cn: &str, parent: &Ca) -> Ca {
    let key = KeyPair::generate().unwrap();
    let params = ca_params(cn);
    let cert = params.signed_by(&key, &parent.issuer).unwrap();
    Ca {
        der: cert.der().clone(),
        issuer: Issuer::new(params, key),
    }
}

fn leaf(parent: &Ca, expired: bool) -> CertificateDer<'static> {
    let key = KeyPair::generate().unwrap();
    let mut params = named(HOST, vec![HOST.to_string()]);
    if expired {
        params.not_before = rcgen::date_time_ymd(2020, 1, 1);
        params.not_after = rcgen::date_time_ymd(2021, 1, 1);
    }
    params
        .signed_by(&key, &parent.issuer)
        .unwrap()
        .der()
        .clone()
}

struct Case {
    name: &'static str,
    /// Served chain, leaf first unless the row is about wrong order.
    served: Vec<CertificateDer<'static>>,
    /// Override `chain_trusted` (the wrong-order row states "chain trusted").
    force_trusted: bool,
    ca_issuers_url: Option<&'static str>,
    chain_complete: CheckStatus,
    /// `None`: the AIA check is not part of the row.
    aia: Option<CheckStatus>,
}

#[test]
fn chain_completeness_table() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let r = root("Test Root R");
    let i = intermediate("Test Intermediate I", &r);
    let x = root("Test Root X");

    let mut store = RootCertStore::empty();
    store.add(r.der.clone()).unwrap();

    let cases = vec![
        Case {
            name: "C29 lone leaf, issuer X not in store, no AIA URL",
            served: vec![leaf(&x, false)],
            force_trusted: false,
            ca_issuers_url: None,
            chain_complete: CheckStatus::Fail,
            aia: Some(CheckStatus::Fail),
        },
        Case {
            name: "C29 lone leaf, issuer X not in store, with AIA URL",
            served: vec![leaf(&x, false)],
            force_trusted: false,
            ca_issuers_url: Some("http://ca.example.com/x.crt"),
            chain_complete: CheckStatus::Fail,
            aia: Some(CheckStatus::Warn),
        },
        Case {
            name: "C30 expired lone leaf, issuer X not in store",
            served: vec![leaf(&x, true)],
            force_trusted: false,
            ca_issuers_url: None,
            chain_complete: CheckStatus::Fail,
            aia: None,
        },
        Case {
            name: "C31 lone leaf signed directly by anchor R",
            served: vec![leaf(&r, false)],
            force_trusted: false,
            ca_issuers_url: None,
            chain_complete: CheckStatus::Pass,
            aia: Some(CheckStatus::Pass),
        },
        Case {
            name: "C32 expired leaf + intermediate I, I issued by anchor R",
            served: vec![leaf(&i, true), i.der.clone()],
            force_trusted: false,
            ca_issuers_url: None,
            chain_complete: CheckStatus::Pass,
            aia: None,
        },
        Case {
            name: "C33 leaf + self-signed X, X not in store",
            served: vec![leaf(&x, false), x.der.clone()],
            force_trusted: false,
            ca_issuers_url: None,
            chain_complete: CheckStatus::Pass,
            aia: None,
        },
        Case {
            name: "C34 wrong order [I, leaf], chain trusted",
            served: vec![i.der.clone(), leaf(&i, false)],
            force_trusted: true,
            ca_issuers_url: None,
            chain_complete: CheckStatus::Warn,
            aia: None,
        },
    ];

    for case in cases {
        let mut chain = parse_chain(&case.served);
        if let Some(url) = case.ca_issuers_url {
            chain[0].ca_issuers_url = Some(url.to_string());
        }
        let mut validation = validate_chain(&chain, Some(HOST), &store, &case.served);
        if case.force_trusted {
            validation.chain_trusted = true;
        }

        let complete = check_chain_complete(&validation);
        assert_eq!(
            complete.status, case.chain_complete,
            "{}: chain_complete",
            case.name
        );

        if let Some(expected) = case.aia {
            let aia = check_aia_reachability(&chain, complete.status != CheckStatus::Fail);
            assert_eq!(aia.status, expected, "{}: aia_reachability", case.name);
        }
    }
}
