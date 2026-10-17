//! Spec v2-entries requirement 2, criteria C2 and C20: the three page templates exist on
//! `base.html`, take component markup from the R44 macros only, add no stylesheet, and the
//! design-system gate passes over the real tree and refuses a planted component class.
use std::path::{Path, PathBuf};

use netray_web::gate::check;

const OWNERSHIP_RULE: u8 = 2;
const PAGES: [&str; 3] = ["entry", "gone", "privacy"];
const SHEETS: [&str; 11] = [
    "base",
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

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn describe(report: &netray_web::gate::Report) -> Vec<String> {
    report
        .violations
        .iter()
        .map(|v| {
            format!(
                "rule {} {}:{} {}",
                v.rule,
                v.path.display(),
                v.line,
                v.message
            )
        })
        .collect()
}

#[test]
fn page_templates_extend_base_and_use_only_component_macros() {
    let missing: Vec<String> = ["templates/base.html"]
        .iter()
        .map(|s| s.to_string())
        .chain(PAGES.iter().map(|p| format!("templates/pages/{p}.html")))
        .filter(|rel| !root().join(rel).is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "templates missing from the tree: {missing:#?}"
    );

    for page in PAGES {
        let text =
            std::fs::read_to_string(root().join(format!("templates/pages/{page}.html"))).unwrap();
        assert!(
            text.contains("{% extends \"base.html\" %}"),
            "{page}.html does not extend base.html"
        );
    }
    // The entry page and the 410 page render components (grade, check rows, the idle input).
    for page in ["entry", "gone"] {
        let text =
            std::fs::read_to_string(root().join(format!("templates/pages/{page}.html"))).unwrap();
        assert!(
            text.contains("{% import \"components/"),
            "{page}.html imports no components/*.html macro"
        );
    }

    // No new stylesheet: base.css plus the ten component sheets, and v2-pages' pages.css
    // (v2-pages requirement 10), nothing else.
    let mut sheets: Vec<String> = std::fs::read_dir(root().join("static"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".css"))
        .collect();
    sheets.sort();
    let mut want: Vec<String> = SHEETS.iter().map(|s| format!("{s}.css")).collect();
    want.push("pages.css".to_string());
    want.sort();
    assert_eq!(sheets, want);
}

#[test]
fn gate_passes_over_the_real_tree_with_the_page_templates() {
    let report = check(root());
    // 10 component templates + base.html + 3 pages + 11 stylesheets.
    assert!(
        report.scanned >= 25,
        "expected at least 25 scanned files, got {}",
        report.scanned
    );
    assert!(
        report.violations.is_empty(),
        "gate violations: {:#?}",
        describe(&report)
    );
}

#[test]
fn gate_refuses_a_component_class_in_a_page_template() {
    let tmp: PathBuf =
        std::env::temp_dir().join(format!("netray-entries-gate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    copy_dir(&root().join("templates"), &tmp.join("templates"));
    copy_dir(&root().join("static"), &tmp.join("static"));
    std::fs::create_dir_all(tmp.join("templates/pages")).unwrap();
    std::fs::write(
        tmp.join("templates/pages/bad.html"),
        "<div class=\"check-row\">x</div>\n",
    )
    .unwrap();

    // The copy must carry the real page templates, or this would pass on the plant alone.
    for page in PAGES {
        assert!(
            tmp.join(format!("templates/pages/{page}.html")).is_file(),
            "{page}.html missing from the copied tree"
        );
    }

    let report = check(&tmp);
    let hits: Vec<_> = report
        .violations
        .iter()
        .filter(|v| v.rule == OWNERSHIP_RULE && v.path.ends_with("templates/pages/bad.html"))
        .collect();
    assert!(
        !hits.is_empty(),
        "rule 2 not reported for bad.html: {:#?}",
        describe(&report)
    );
    assert_eq!(
        report.violations.len(),
        hits.len(),
        "unexpected extra violations: {:#?}",
        describe(&report)
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
