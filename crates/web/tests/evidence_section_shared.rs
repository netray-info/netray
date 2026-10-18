//! A block owned by another module is one shared line, not a second rendering
//! (spec v2-inspectors, requirement 4, Phase 2; C2, C6).
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::components::RunLineView;
use netray_web::evidence::{render_section, section};

fn bid(s: &str) -> BlockId {
    BlockId::parse(s).unwrap()
}

fn check(id: &str, evidence: &[&str]) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status: Status::Pass,
        findings: vec![],
        evidence: evidence.iter().map(|b| bid(b)).collect(),
    }
}

fn run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![
            RunSection {
                protocol: Protocol::Dns,
                outcome: SectionKind::Measured,
                reason: None,
            },
            RunSection {
                protocol: Protocol::Tls,
                outcome: SectionKind::Measured,
                reason: None,
            },
        ],
        // Two TLS checks reference the DNS block: it must still be one shared line.
        checks: vec![
            check("tls.caa_compliant", &["dns.caa"]),
            check("tls.chain_trusted", &["tls.chain", "dns.caa"]),
            check("dns.caa", &["dns.caa"]),
        ],
        blocks: vec![
            Block {
                id: bid("dns.caa"),
                title: "CAA records".into(),
                shape: Shape::Text("0 issue \"ca.example.com\"".into()),
            },
            Block {
                id: bid("tls.chain"),
                title: "Certificate chain".into(),
                shape: Shape::Text("leaf, intermediate".into()),
            },
        ],
    }
}

fn line() -> RunLineView {
    RunLineView {
        parts: vec!["incomplete, not recorded".into()],
    }
}

#[test]
fn foreign_block_is_one_shared_line_in_the_tls_view() {
    let run = run();
    let view = section(&run, Protocol::Tls, line());
    let html = render_section(&view).unwrap();

    assert!(
        !html.contains("id=\"ev-dns.caa\""),
        "dns.caa rendered in TLS view"
    );
    assert!(view.blocks.iter().all(|b| b.id != "ev-dns.caa"));
    assert!(html.contains("id=\"ev-tls.chain\""), "own block missing");

    assert_eq!(view.shared.len(), 1, "two references, one line");
    assert_eq!(view.shared[0].title, "CAA records");
    assert_eq!(view.shared[0].href, "dns#ev-dns.caa");
    assert_eq!(
        html.matches("<a href=\"dns#ev-dns.caa\">CAA records</a>")
            .count(),
        1
    );
}

#[test]
fn owner_view_renders_the_block() {
    let run = run();
    let view = section(&run, Protocol::Dns, line());
    let html = render_section(&view).unwrap();

    assert!(html.contains("id=\"ev-dns.caa\""));
    assert!(view.blocks.iter().any(|b| b.id == "ev-dns.caa"));
    assert!(view.shared.is_empty());
}
