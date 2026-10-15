//! Phase 3 criterion C3 (spec v2-design-system): the ten components exist as a macro file plus a
//! stylesheet each, and the real tree, with them in it, passes the gate.
use std::path::Path;

use netray_web::gate::check;

const COMPONENTS: [&str; 10] = [
    "grade",
    "run_line",
    "progress_ray",
    "spectrum",
    "trace",
    "check_row",
    "evidence_block",
    "diff_block",
    "grouping_switch",
    "query_input",
];

#[test]
fn ten_components_are_scanned_and_pass_the_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // The component files must be on disk, so the scan below is not vacuous.
    let missing: Vec<String> = COMPONENTS
        .iter()
        .flat_map(|name| {
            [
                format!("templates/components/{name}.html"),
                format!("static/{name}.css"),
            ]
        })
        .filter(|rel| !root.join(rel).is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "component files missing from the tree: {missing:#?}"
    );

    let report = check(root);
    // 10 templates + 10 stylesheets + base.css.
    assert!(
        report.scanned >= 21,
        "expected at least 21 scanned files, got {}",
        report.scanned
    );
    assert!(
        report.violations.is_empty(),
        "gate violations: {:#?}",
        report
            .violations
            .iter()
            .map(|v| format!(
                "rule {} {}:{} {}",
                v.rule,
                v.path.display(),
                v.line,
                v.message
            ))
            .collect::<Vec<_>>()
    );
}
