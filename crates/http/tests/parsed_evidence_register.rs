// V2 http checks, Phase 2 (criteria C8, C23, C24, C25): the register declares the parsed
// header blocks, the volatile list names their unstable keys, a refused HTTPS probe emits none
// of them, and the twelve check ids stay as they were.
//
// Spec: specs/features/v2-http-checks/spec.md.

use std::collections::BTreeSet;

use netray_engine::profile::Profiles;
use netray_engine::{EvidencePath, Module, SectionOutcome};
use netray_http::inspect::assembler::InspectResponse;
use netray_http::module::{REGISTER, translate};
use netray_http::{HttpModule, ModuleConfig};
use netray_model::Protocol;
use netray_model::evidence::{ShapeKind, ValueType as T, check_evidence_rule, validate};
use serde_json::json;

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

const PARSED: [&str; 5] = [
    "http.csp",
    "http.hsts",
    "http.caching",
    "http.cdn",
    "http.isolation",
];

const TWELVE: [&str; 12] = [
    "http.https_redirect",
    "http.hsts",
    "http.cors",
    "http.cookie_secure",
    "http.csp",
    "http.x_frame_options",
    "http.x_content_type_options",
    "http.referrer_policy",
    "http.permissions_policy",
    "http.deprecated_headers",
    "http.info_leakage",
    "http.caching",
];

fn module() -> HttpModule {
    HttpModule::new(toml::from_str::<ModuleConfig>("").expect("empty config")).expect("builds")
}

#[test]
fn register_declares_the_parsed_header_blocks_and_keys() {
    let blocks: Vec<(&str, ShapeKind)> = REGISTER.blocks.to_vec();
    for (id, kind) in [
        ("http.csp", ShapeKind::Tree),
        ("http.hsts", ShapeKind::Fields),
        ("http.caching", ShapeKind::Fields),
        ("http.cdn", ShapeKind::Fields),
        ("http.isolation", ShapeKind::Fields),
    ] {
        assert!(blocks.contains(&(id, kind)), "block {id} {kind:?} missing");
    }

    let want: &[(&str, &str, T)] = &[
        ("http.csp", "report_only", T::Bool),
        ("http.csp", "issue_count", T::Int),
        ("http.csp", "issue", T::Text),
        ("http.csp", "directive", T::Text),
        ("http.csp", "source", T::Text),
        ("http.hsts", "max_age", T::Int),
        ("http.hsts", "include_subdomains", T::Bool),
        ("http.hsts", "preload", T::Bool),
        ("http.caching", "directive", T::Text),
        ("http.caching", "max_age", T::Int),
        ("http.caching", "etag", T::Text),
        ("http.caching", "etag_weak", T::Bool),
        ("http.caching", "last_modified", T::Time),
        ("http.caching", "vary", T::Text),
        ("http.caching", "age", T::Int),
        ("http.cdn", "provider", T::Text),
        ("http.cdn", "cache_status", T::Text),
        ("http.cdn", "indicator", T::Text),
        ("http.isolation", "coop", T::Text),
        ("http.isolation", "coep", T::Text),
        ("http.isolation", "corp", T::Text),
        ("http.isolation", "missing", T::Text),
    ];
    for k in want {
        assert!(REGISTER.keys.contains(k), "key {k:?} missing");
    }
}

#[test]
fn volatile_list_names_the_unstable_parsed_keys() {
    let m = module();
    let volatile: &[EvidencePath] = m.volatile();
    let got: BTreeSet<&str> = volatile.iter().map(|p| p.0).collect();
    for p in [
        "http.caching/etag",
        "http.caching/last_modified",
        "http.caching/age",
        "http.cdn/cache_status",
        "http.csp/source",
    ] {
        assert!(got.contains(p), "volatile misses {p}");
        let (id, key, _) = EvidencePath::parse(p).expect(p);
        let name = format!("{}.{}", id.module(), id.name());
        assert!(
            REGISTER
                .keys
                .iter()
                .any(|(b, k, _)| *b == name && *k == key),
            "{p} names no registered key"
        );
    }
}

#[test]
fn refused_https_probe_emits_no_parsed_blocks() {
    let mut v: serde_json::Value = serde_json::from_str(SPECTRA_JSON).unwrap();
    v["status"] = json!(0);
    v["headers"] = json!({});
    v["error"] = json!({"kind": "refused", "message": "connection refused"});
    let resp: InspectResponse = serde_json::from_value(v).unwrap();
    let SectionOutcome::Measured {
        checks, evidence, ..
    } = translate(&resp)
    else {
        panic!("expected Measured");
    };
    for id in PARSED {
        assert!(
            !evidence.iter().any(|b| b.id.to_string() == id),
            "{id} emitted on a refused probe"
        );
    }
    validate("http", &REGISTER, &evidence).expect("blocks fit the register");
    check_evidence_rule(&checks, &evidence).expect("evidence rule holds");
}

#[test]
fn the_twelve_http_check_ids_are_unchanged() {
    let ids: Vec<String> = module().checks().iter().map(|c| c.to_string()).collect();
    assert_eq!(ids, TWELVE);

    let section = &Profiles::current().sections[&Protocol::Http];
    let in_profile: BTreeSet<String> = section.checks.keys().map(|c| c.to_string()).collect();
    let want: BTreeSet<String> = TWELVE.iter().map(|s| s.to_string()).collect();
    assert_eq!(in_profile, want);
}
