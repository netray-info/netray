//! Contract tests for the design-system gate (V2 spec v2-design-system, requirement 6,
//! criteria C1 to C5). Every case plants files in a per-call temp tree and runs
//! `netray_web::gate::check` over it; the table covers the three rules and their passing twins.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use netray_web::gate::check;

const TOKEN_RULE: u8 = 1;
const OWNERSHIP_RULE: u8 = 2;
const SCRIPT_RULE: u8 = 3;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh, empty directory per call (process id plus counter).
fn temp_root() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("netray-gate-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn plant(root: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}

/// Expected outcome: `None` passes; `Some((rule, file))` must report that rule against that file.
struct Case {
    name: &'static str,
    files: &'static [(&'static str, &'static str)],
    expect: Option<(u8, &'static str)>,
}

const CASES: &[Case] = &[
    // C1 rule 1: colour literal in an island, a template style attribute, a nested template.
    Case {
        name: "colour literal in an island",
        files: &[("islands/x.ts", "el.style.color = '#ff0000';\n")],
        expect: Some((TOKEN_RULE, "islands/x.ts")),
    },
    Case {
        name: "colour literal in a template style attribute",
        files: &[(
            "templates/page.html",
            "<div style=\"color: rgb(1, 2, 3)\">x</div>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/page.html")),
    },
    Case {
        name: "named colour in a stylesheet",
        files: &[("static/x.css", ".a { background: red; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "nested template is scanned",
        files: &[("templates/pages/x.html", "<p style=\"color: #fff\">x</p>\n")],
        expect: Some((TOKEN_RULE, "templates/pages/x.html")),
    },
    Case {
        name: "literal font-size",
        files: &[("static/x.css", ".a { font-size: 12px; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal border width",
        files: &[(
            "static/x.css",
            ".a { border: 2px solid var(--color-line); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal border-width",
        files: &[("static/x.css", ".a { border-width: 1px; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal font-family",
        files: &[("static/x.css", ".a { font-family: Arial, sans-serif; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal font shorthand",
        files: &[("static/x.css", ".a { font: 12px/1.2 Arial; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal stroke-width",
        files: &[("static/x.css", ".a { stroke-width: 3; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    // C3: token references pass, literals and non-token properties fail.
    Case {
        name: "colour literal in a stylesheet",
        files: &[("static/x.css", ".a { color: #fff; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "radius token passes",
        files: &[("static/x.css", ".a { border-radius: var(--radius-max); }\n")],
        expect: None,
    },
    Case {
        name: "tokens for border, font and colour pass",
        files: &[(
            "static/x.css",
            ".a { border: var(--line-hair) solid var(--color-line); font-family: var(--font-sans); color: var(--color-ink); }\n",
        )],
        expect: None,
    },
    Case {
        name: "property that is no token fails",
        files: &[(
            "static/x.css",
            ".a { --r: 50%; border-radius: var(--r); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    // C1 rule 2 and C4: component ownership.
    Case {
        name: "component class in a foreign template",
        files: &[(
            "templates/pages/x.html",
            "<div class=\"check-row\">x</div>\n",
        )],
        expect: Some((OWNERSHIP_RULE, "templates/pages/x.html")),
    },
    Case {
        name: "component class in its own macro file",
        files: &[(
            "templates/components/check_row.html",
            "<div class=\"check-row\">x</div>\n",
        )],
        expect: None,
    },
    Case {
        name: "component class defined in its own stylesheet",
        files: &[("static/check_row.css", ".check-row { display: block; }\n")],
        expect: None,
    },
    Case {
        name: "component class defined in a foreign stylesheet",
        files: &[("static/other.css", ".check-row { display: block; }\n")],
        expect: Some((OWNERSHIP_RULE, "static/other.css")),
    },
    // C5 rule 3: inline scripts.
    Case {
        name: "inline script",
        files: &[("templates/page.html", "<script>alert(1)</script>\n")],
        expect: Some((SCRIPT_RULE, "templates/page.html")),
    },
    Case {
        name: "speculationrules script passes",
        files: &[(
            "templates/page.html",
            "<script type=\"speculationrules\">{\"prerender\":[]}</script>\n",
        )],
        expect: None,
    },
];

#[test]
fn planted_fixtures_are_refused_and_clean_ones_pass() {
    for case in CASES {
        let root = temp_root();
        plant(&root, case.files);
        let report = check(&root);
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(
            report.scanned,
            case.files.len(),
            "{}: scanned must equal the planted file count",
            case.name
        );
        match case.expect {
            None => {
                assert!(
                    report.violations.is_empty(),
                    "{}: expected no violations, got {:#?}",
                    case.name,
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
            Some((rule, file)) => {
                assert!(
                    report
                        .violations
                        .iter()
                        .any(|v| v.rule == rule && v.path.ends_with(file)),
                    "{}: expected rule {rule} against {file}, got {:?}",
                    case.name,
                    report
                        .violations
                        .iter()
                        .map(|v| format!("rule {} {}", v.rule, v.path.display()))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

/// C2: the crate's real tree passes, and the scan is not vacuous.
#[test]
fn real_tree_passes_the_gate() {
    let report = check(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(report.scanned > 0, "a scan of zero files proves nothing");
    assert!(
        report.violations.is_empty(),
        "real tree violations: {:#?}",
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
