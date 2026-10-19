//! V2 spec v2-result-fidelity, requirement 13 (C13, C26): `/r/<id>` renders inside the site frame.
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::Entry;
use netray_web::entries::render;
use netray_web::entries::view::entry_view;

fn entry() -> Entry {
    Entry {
        id: "aB3dE5gH".into(),
        domain: "example.com".into(),
        created_at: 1_778_262_240,
        last_seen_at: 1_778_262_240,
        predecessor_id: None,
        profile_version: 3,
        grade: Grade::A,
        fingerprint: "f".into(),
        run: Run {
            profile_version: 3,
            engine_version: "0.30.0".into(),
            evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
            grade: Grade::A,
            complete: true,
            sections: vec![RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            }],
            checks: vec![RunCheck {
                id: CheckId::parse("dns.dnssec_valid").unwrap(),
                status: Status::Fail,
                findings: vec![],
                evidence: vec![],
            }],
            blocks: vec![],
        },
        diff: None,
        engine_version: "0.30.0".into(),
        duration_ms: None,
    }
}

#[test]
fn entry_page_renders_inside_the_site_frame() {
    let html = render::entry(&entry_view(&entry())).expect("render entry");
    for needle in [
        "<header class=\"page-header\"",
        "<main",
        "<footer class=\"page-footer\"",
        "<link rel=\"stylesheet\" href=\"/static/app.css\">",
        "<title>example.com · A</title>",
        "og:image\" content=\"/r/aB3dE5gH/og.png\"",
    ] {
        assert!(html.contains(needle), "entry page lacks {needle}");
    }
    let header_start = html.find("<header class=\"page-header\"").expect("header");
    let header = &html[header_start..html.find("</header>").expect("header end")];
    for label in ["Inspectors", "Guides", "API", "Status", "Theme"] {
        assert!(header.contains(label), "nav lacks {label}");
    }
    let main = html.find("<main").unwrap();
    let footer = html.find("<footer class=\"page-footer\"").unwrap();
    assert!(
        header_start < main && main < footer,
        "frame order header, main, footer"
    );
}
