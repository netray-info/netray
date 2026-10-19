//! The inspect band's layout over one protocol's checks (V2 spec v2-inspectors, requirements 16
//! and 25): one line per TLS check in catalogue order, one run, and the component renders exactly
//! those lines.
use std::collections::BTreeMap;

use netray_engine::catalogue;
use netray_engine::profile::Profiles;
use netray_model::{CheckId, Protocol, Status};
use netray_web::components::{SpectrumView, spectrum};
use netray_web::spectrum::{RunInput, layout};

#[test]
fn tls_band_has_one_line_per_tls_check_and_one_run() {
    let expected: Vec<String> = catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Tls)
        .map(|e| e.id.to_string())
        .collect();
    let statuses: BTreeMap<CheckId, Status> = catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Tls)
        .map(|e| (e.id.clone(), Status::Pass))
        .collect();
    let input = RunInput {
        statuses,
        sections: BTreeMap::new(),
        profile: Profiles::current(),
    };
    let l = layout(&input, &[], &catalogue().for_protocol(Protocol::Tls), 432).expect("layout");

    let ids: Vec<String> = l.lines.iter().map(|x| x.id.to_string()).collect();
    assert_eq!(ids, expected);
    assert!(l.lines.iter().all(|x| x.protocol == Protocol::Tls));
    assert_eq!(l.runs.len(), 1);

    let html = spectrum(&SpectrumView::from_layout(&l, "TLS checks", 0)).expect("render");
    let mut codes: Vec<String> = Vec::new();
    let mut rest = html.as_str();
    while let Some(i) = rest.find("data-code=\"") {
        rest = &rest[i + 11..];
        let end = rest.find('"').unwrap();
        let c = rest[..end].to_string();
        if !codes.contains(&c) {
            codes.push(c);
        }
    }
    assert_eq!(codes, expected);
    for other in ["dns.", "http.", "email.", "ip."] {
        assert!(!html.contains(&format!("data-code=\"{other}")), "{other}");
    }
}
