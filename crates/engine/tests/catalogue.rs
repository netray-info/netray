//! C2/C3: the embedded catalogue holds the 54 checks of SDD §3.9; `Catalogue::from_toml` refuses
//! malformed entries naming the ID. The per-protocol count belongs to the embedded test, not the loader.

use std::collections::HashSet;

use netray_engine::catalogue;
use netray_engine::catalogue::Catalogue;
use netray_model::{CheckId, Protocol};

const IDS: &[&str] = &[
    "dns.dnssec", "dns.dnskey_algorithm", "dns.dnssec_rollover", "dns.caa", "dns.ns",
    "dns.ns_lame", "dns.ns_delegation", "dns.cname_apex", "dns.https_svcb", "dns.ttl",
    "tls.tls_reachable", "tls.chain_trusted", "tls.not_expired", "tls.hostname_match",
    "tls.chain_complete", "tls.strong_signature", "tls.key_strength", "tls.expiry_window",
    "tls.cert_lifetime", "tls.san_quality", "tls.aia_reachability", "tls.tls_version",
    "tls.forward_secrecy", "tls.aead_cipher", "tls.ocsp_stapled", "tls.ct_logged",
    "tls.caa_compliant", "tls.dane_valid", "tls.consistency", "tls.alpn_consistency",
    "tls.ech_advertised",
    "http.https_redirect", "http.hsts", "http.cors", "http.cookie_secure", "http.csp",
    "http.x_frame_options", "http.x_content_type_options", "http.referrer_policy",
    "http.permissions_policy", "http.deprecated_headers", "http.info_leakage", "http.caching",
    "email.mx", "email.spf", "email.dkim", "email.dmarc", "email.mta_sts", "email.tls_rpt",
    "email.bimi", "email.dane", "email.dnsbl", "email.fcrdns",
    "ip.reputation",
];

#[test]
fn embedded_catalogue_holds_the_54_checks_of_the_sdd() {
    let cat = catalogue();
    let entries = cat.entries();
    assert_eq!(entries.len(), 54);

    for (proto, n) in [
        (Protocol::Dns, 10),
        (Protocol::Tls, 21),
        (Protocol::Http, 12),
        (Protocol::Email, 10),
        (Protocol::Ip, 1),
    ] {
        let got = entries.iter().filter(|e| e.protocol == proto).count();
        assert_eq!(got, n, "{proto:?} count");
    }

    let mut seen = HashSet::new();
    for e in entries {
        let id = e.id.to_string();
        assert!(seen.insert(id.clone()), "duplicate id {id}");
        assert_eq!(e.id.protocol(), e.protocol, "{id}: prefix vs protocol");
        assert!(!e.short.is_empty() && e.short.chars().count() <= 8, "{id}: short {:?}", e.short);
        for (name, v) in [
            ("title", &e.title),
            ("finding", &e.finding),
            ("impact", &e.impact),
            ("fix_hint", &e.fix_hint),
            ("source", &e.source),
            ("guide", &e.guide),
        ] {
            assert!(!v.trim().is_empty(), "{id}: {name} is empty");
        }
    }

    let want: HashSet<String> = IDS.iter().map(|s| s.to_string()).collect();
    assert_eq!(IDS.len(), 54);
    assert_eq!(seen, want);

    for id in IDS {
        let cid = CheckId::parse(id).unwrap();
        assert!(cat.get(&cid).is_some(), "get({id})");
    }
}

fn entry(id: &str, short: &str, impact: Option<&str>, owner: &str) -> String {
    let impact = impact.map(|i| format!("impact = \"{i}\"\n")).unwrap_or_default();
    format!(
        "[[check]]\nid = \"{id}\"\nprotocol = \"tls\"\ntitle = \"Chain trusted\"\n\
         finding = \"The chain does not validate.\"\n{impact}fix_hint = \"Install the intermediate.\"\n\
         fix_owner = \"{owner}\"\nseverity = \"high\"\nsource = \"RFC 5280\"\n\
         guide = \"chain-trusted\"\nshort = \"{short}\"\n"
    )
}

#[test]
fn from_toml_accepts_a_valid_single_entry() {
    let cat = Catalogue::from_toml(&entry("tls.chain_trusted", "chain", Some("Browsers warn."), "web_server"))
        .expect("valid fixture");
    assert_eq!(cat.entries().len(), 1);
    assert!(cat.get(&CheckId::parse("tls.chain_trusted").unwrap()).is_some());
}

#[test]
fn from_toml_refuses_malformed_entries_naming_the_id() {
    let long = entry("tls.chain_trusted", "123456789", Some("x"), "web_server");
    let msg = Catalogue::from_toml(&long).expect_err("9-char short").to_string();
    assert!(msg.contains("tls.chain_trusted"), "{msg}");

    let missing = entry("tls.chain_trusted", "chain", None, "web_server");
    let msg = Catalogue::from_toml(&missing).expect_err("missing impact").to_string();
    assert!(msg.contains("tls.chain_trusted"), "{msg}");

    let dup = format!(
        "{}\n{}",
        entry("tls.chain_trusted", "chain", Some("x"), "web_server"),
        entry("tls.chain_trusted", "chain", Some("x"), "web_server")
    );
    let msg = Catalogue::from_toml(&dup).expect_err("duplicate").to_string();
    assert!(msg.contains("tls.chain_trusted"), "{msg}");

    let owner = entry("tls.chain_trusted", "chain", Some("x"), "nobody");
    assert!(Catalogue::from_toml(&owner).is_err());
}
