//! Evidence section links and live rendering (V2 spec v2-inspectors, requirement 7, interface I8;
//! criteria C12 and C4): `href` builds the entry-view anchor, and the section renderer gives a
//! live (incomplete) run the same markup as the stored view.
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::evidence::{entry_run_line, href, render_section, section};

#[test]
fn href_names_the_entry_protocol_and_optional_block() {
    let block = BlockId::parse("tls.chain@192.0.2.1:443").unwrap();
    let cases = [
        (Protocol::Dns, "dns"),
        (Protocol::Tls, "tls"),
        (Protocol::Http, "http"),
        (Protocol::Email, "email"),
        (Protocol::Ip, "ip"),
    ];
    for (protocol, segment) in cases {
        assert_eq!(
            href("Ab3dE5fG", protocol, None),
            format!("/r/Ab3dE5fG/{segment}")
        );
    }
    assert_eq!(
        href("Ab3dE5fG", Protocol::Tls, Some(&block)),
        "/r/Ab3dE5fG/tls#ev-tls.chain@192.0.2.1:443"
    );
}

fn run(complete: bool) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("tls.chain_trusted").unwrap(),
            status: Status::Pass,
            findings: vec!["chain verifies against the system roots".into()],
            evidence: vec![BlockId::parse("tls.chain@192.0.2.1:443").unwrap()],
        }],
        blocks: vec![Block {
            id: BlockId::parse("tls.chain@192.0.2.1:443").unwrap(),
            title: "Certificate chain".into(),
            shape: Shape::Text("example.com issued by Example CA".to_string()),
        }],
    }
}

#[test]
fn live_run_renders_the_same_markup_as_the_stored_view() {
    let render = |complete: bool| {
        let run = run(complete);
        let line = entry_run_line(1_791_469_920, "Ab3dE5fG");
        render_section(&section(&run, Protocol::Tls, line)).unwrap()
    };
    let stored = render(true);
    let live = render(false);
    assert_eq!(stored, live);
    assert!(
        live.contains("id=\"ev-tls.chain@192.0.2.1:443\""),
        "block missing in: {live}"
    );
    assert!(
        live.contains("chain verifies against the system roots"),
        "check row missing in: {live}"
    );
}
