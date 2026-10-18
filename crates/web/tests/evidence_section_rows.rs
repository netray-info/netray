//! Contract tests for the evidence view of one protocol (spec v2-inspectors, requirement 3; C1, C5).
use netray_engine::catalogue::catalogue;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_web::evidence::{entry_run_line, render_section, section};

const CHAIN: &str = "tls.chain@192.0.2.1:443";
const CERT: &str = "tls.cert@192.0.2.1:443";

fn block(id: &str) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: format!("Title of {id}"),
        shape: Shape::Text(format!("body of {id}")),
    }
}

fn check(id: &str, status: Status, findings: &[&str], evidence: &[&str]) -> RunCheck {
    RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: findings.iter().map(|s| s.to_string()).collect(),
        evidence: evidence
            .iter()
            .map(|s| BlockId::parse(s).unwrap())
            .collect(),
    }
}

fn run(checks: Vec<RunCheck>, blocks: Vec<Block>) -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Tls,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks,
        blocks,
    }
}

fn line() -> netray_web::components::RunLineView {
    entry_run_line(1_791_469_920, "Ab3dE5fG")
}

fn tls_ids() -> Vec<String> {
    catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Tls)
        .map(|e| e.id.to_string())
        .collect()
}

#[test]
fn rows_follow_the_catalogue_with_tone_fix_and_guide() {
    // Statuses are given out round the six kinds, in reverse catalogue order in the run.
    let kinds = [
        (Status::Pass, "pass"),
        (Status::Warn, "warn"),
        (Status::Fail, "fail"),
        (Status::NotApplicable, "untested"),
        (Status::NotTested, "untested"),
        (Status::Unmeasured, "untested"),
    ];
    let ids = tls_ids();
    let mut checks: Vec<RunCheck> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            check(
                id,
                kinds[i % 6].0.clone(),
                &["first finding", "second finding"],
                &[],
            )
        })
        .collect();
    checks.reverse();
    let view = section(&run(checks, vec![]), Protocol::Tls, line());

    assert_eq!(view.rows.len(), ids.len());
    for (i, (row, id)) in view.rows.iter().zip(&ids).enumerate() {
        let spec = catalogue().get(&CheckId::parse(id).unwrap()).unwrap();
        let (status, tone) = &kinds[i % 6];
        assert_eq!(row.check.code, spec.short, "{id}");
        assert_eq!(row.check.title, spec.title, "{id}");
        assert_eq!(row.check.tone.as_str(), *tone, "{id}");
        assert_eq!(row.check.body, "first finding; second finding", "{id}");
        assert_eq!(
            row.check.guide_href.as_deref(),
            Some(spec.guide.as_str()),
            "{id}"
        );
        let fixed = matches!(status, Status::Warn | Status::Fail);
        let want_fix = fixed.then(|| spec.fix_hint.as_str());
        assert_eq!(row.check.fix.as_deref(), want_fix, "{id}");
    }
}

#[test]
fn blocks_of_the_protocol_only_in_run_order() {
    let r = run(
        vec![check(
            "tls.chain_trusted",
            Status::Pass,
            &[],
            &[CERT, CHAIN],
        )],
        vec![block("dns.caa"), block(CERT), block(CHAIN)],
    );
    let view = section(&r, Protocol::Tls, line());
    let ids: Vec<&str> = view.blocks.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, [format!("ev-{CERT}"), format!("ev-{CHAIN}")]);
}

#[test]
fn passing_check_links_its_block_and_the_block_lists_its_short() {
    let trust = catalogue()
        .get(&CheckId::parse("tls.chain_trusted").unwrap())
        .unwrap()
        .short
        .clone();
    assert_eq!(trust, "Trust");
    let r = run(
        vec![check("tls.chain_trusted", Status::Pass, &[], &[CHAIN])],
        vec![block(CHAIN)],
    );
    let view = section(&r, Protocol::Tls, line());
    let row = view.rows.iter().find(|r| r.check.code == trust).unwrap();
    let hrefs: Vec<&str> = row.links.iter().map(|l| l.href.as_str()).collect();
    assert_eq!(hrefs, ["#ev-tls.chain@192.0.2.1:443"]);
    let b = view
        .blocks
        .iter()
        .find(|b| b.id == format!("ev-{CHAIN}"))
        .unwrap();
    assert_eq!(b.feeds, [trust]);
}

#[test]
fn rendered_section_orders_run_line_rows_blocks() {
    let r = run(
        vec![check("tls.chain_trusted", Status::Pass, &[], &[CHAIN])],
        vec![block(CHAIN)],
    );
    let html = render_section(&section(&r, Protocol::Tls, line())).unwrap();
    let run_line = html.find("run of 2026-10-08 14:32 UTC").expect("run line");
    let first_row = html.find("Chain trusted").expect("check row");
    let first_block = html
        .find("id=\"ev-tls.chain@192.0.2.1:443\"")
        .expect("block");
    assert!(run_line < first_row, "run line before the first check row");
    assert!(first_row < first_block, "check rows before the first block");
}
