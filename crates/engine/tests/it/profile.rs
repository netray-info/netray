//! C1/C2: profile v3 weights exactly the catalogue's 54 IDs, `Profile::from_toml` refuses malformed
//! profiles, and the lock pins the embedded `v3.toml` byte for byte.

use std::collections::{BTreeMap, BTreeSet};

use netray_engine::catalogue;
use netray_engine::profile::{Profile, Profiles, check_lock};
use netray_model::{Grade, Protocol};

const V3: &str = include_str!("../../profiles/v3.toml");

/// (check ID, weight): the unchanged V1 weights and the SDD §3.9 split.
const WEIGHTS: &[(&str, u32)] = &[
    ("dns.dnssec", 5),
    ("dns.caa", 5),
    ("dns.ns", 3),
    ("dns.ns_lame", 5),
    ("dns.ns_delegation", 3),
    ("dns.cname_apex", 5),
    ("dns.https_svcb", 2),
    ("dns.ttl", 2),
    ("dns.dnskey_algorithm", 3),
    ("dns.dnssec_rollover", 2),
    ("tls.tls_reachable", 10),
    ("tls.chain_trusted", 10),
    ("tls.not_expired", 10),
    ("tls.hostname_match", 10),
    ("tls.chain_complete", 5),
    ("tls.strong_signature", 5),
    ("tls.key_strength", 5),
    ("tls.expiry_window", 5),
    ("tls.cert_lifetime", 3),
    ("tls.san_quality", 2),
    ("tls.aia_reachability", 2),
    ("tls.tls_version", 5),
    ("tls.forward_secrecy", 5),
    ("tls.aead_cipher", 5),
    ("tls.ocsp_stapled", 3),
    ("tls.ct_logged", 3),
    ("tls.caa_compliant", 5),
    ("tls.dane_valid", 3),
    ("tls.consistency", 3),
    ("tls.alpn_consistency", 2),
    ("tls.ech_advertised", 2),
    ("http.https_redirect", 5),
    ("http.hsts", 5),
    ("http.cors", 3),
    ("http.cookie_secure", 2),
    ("http.csp", 1),
    ("http.x_frame_options", 1),
    ("http.x_content_type_options", 1),
    ("http.referrer_policy", 1),
    ("http.permissions_policy", 1),
    ("http.deprecated_headers", 1),
    ("http.info_leakage", 1),
    ("http.caching", 1),
    ("email.spf", 3),
    ("email.dkim", 3),
    ("email.dmarc", 4),
    ("email.mx", 2),
    ("email.dnsbl", 2),
    ("email.fcrdns", 1),
    ("email.mta_sts", 2),
    ("email.dane", 2),
    ("email.tls_rpt", 1),
    ("email.bimi", 2),
    ("ip.reputation", 5),
];

#[test]
fn profile_v3_is_current_and_weights_exactly_the_catalogue() {
    let p = Profiles::current();
    assert_eq!(p.version, 3);
    assert_eq!(p.name, "v3");

    let mut got: BTreeMap<String, u32> = BTreeMap::new();
    for section in p.sections.values() {
        for (id, w) in &section.checks {
            assert!(*w >= 1, "{id}: weight {w}");
            assert!(got.insert(id.to_string(), *w).is_none(), "{id} twice");
        }
    }

    let cat = catalogue();
    let catalogue_ids: BTreeSet<String> = cat.entries().iter().map(|e| e.id.to_string()).collect();
    let profile_ids: BTreeSet<String> = got.keys().cloned().collect();
    assert_eq!(profile_ids, catalogue_ids);

    assert_eq!(WEIGHTS.len(), 54);
    let want: BTreeMap<String, u32> = WEIGHTS.iter().map(|(i, w)| (i.to_string(), *w)).collect();
    assert_eq!(got, want);

    for (proto, w) in [
        (Protocol::Dns, 20),
        (Protocol::Tls, 35),
        (Protocol::Http, 20),
        (Protocol::Email, 15),
        (Protocol::Ip, 10),
    ] {
        let s = p
            .sections
            .get(&proto)
            .unwrap_or_else(|| panic!("{proto:?} section"));
        assert_eq!(s.weight, w, "{proto:?} section weight");
    }
    assert_eq!(p.sections.len(), 5);

    let hard: BTreeSet<String> = p
        .sections
        .values()
        .flat_map(|s| s.hard_fail.iter().map(|id| id.to_string()))
        .collect();
    let want_hard: BTreeSet<String> = [
        "tls.chain_trusted",
        "tls.hostname_match",
        "tls.not_expired",
        "tls.tls_reachable",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(hard, want_hard);
    let tls_hard = p.sections[&Protocol::Tls].hard_fail.len();
    assert_eq!(tls_hard, 4, "hard fail lives in the tls section");

    assert_eq!(
        p.thresholds,
        vec![
            (Grade::APlus, 97),
            (Grade::A, 90),
            (Grade::B, 75),
            (Grade::C, 60),
            (Grade::D, 40),
            (Grade::F, 0),
        ]
    );

    assert!(Profiles::get(3).is_some());
    assert_eq!(Profiles::get(3).unwrap().version, 3);
    assert!(Profiles::get(2).is_none());
}

fn fixture(weight: u32, hard_fail: &str) -> String {
    format!(
        "[meta]\nname = \"t\"\nversion = 1\n\n\
         [sections.tls]\nweight = 35\nhard_fail = [{hard_fail}]\n\n\
         [sections.tls.checks]\ntls_reachable = {weight}\nchain_trusted = 1\n\n\
         [thresholds]\n\"A+\" = 97\n\"A\" = 90\n\"B\" = 75\n\"C\" = 60\n\"D\" = 40\n\"F\" = 0\n"
    )
}

#[test]
fn from_toml_accepts_a_valid_fixture_and_refuses_malformed_ones() {
    let p = Profile::from_toml(&fixture(2, "\"chain_trusted\"")).expect("valid fixture");
    assert_eq!(p.sections[&Protocol::Tls].checks.len(), 2);

    assert!(
        Profile::from_toml(&fixture(0, "\"chain_trusted\"")).is_err(),
        "weight 0"
    );
    assert!(
        Profile::from_toml(&fixture(2, "\"not_a_check\"")).is_err(),
        "hard_fail name outside the section's checks"
    );
}

#[test]
fn lock_accepts_the_embedded_v3_and_refuses_a_changed_byte_or_unknown_version() {
    check_lock(3, V3).expect("embedded v3.toml matches the lock");
    assert!(check_lock(3, &format!("{V3} ")).is_err(), "appended byte");
    assert!(check_lock(99, V3).is_err(), "version without a lock entry");
}

#[test]
fn every_hard_fail_check_is_critical_in_the_catalogue() {
    let p = Profiles::current();
    let cat = catalogue();
    for section in p.sections.values() {
        for id in &section.hard_fail {
            let spec = cat
                .get(id)
                .unwrap_or_else(|| panic!("{id} in the catalogue"));
            assert_eq!(
                spec.severity,
                netray_model::Severity::Critical,
                "{id}: a hard fail is critical"
            );
        }
    }
}
