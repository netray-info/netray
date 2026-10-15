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
    // Amended requirement 6: comments, custom properties, SVG attributes, logical sides, case.
    Case {
        name: "comment line before a declaration does not hide its colour",
        files: &[("static/x.css", ".a {\n  /* marker */\n  color: #fff;\n}\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "declaration after a comment is still checked",
        files: &[(
            "static/x.css",
            ".a { margin: 0; /* x */ border-radius: 50%; }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "colour literal in a custom property value",
        files: &[("static/x.css", ".a { --c: #fff; color: var(--c); }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "stylesheet redefines a token",
        files: &[(
            "static/x.css",
            ".a { --radius-max: 50%; border-radius: var(--radius-max); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "colour literal in a custom property in a style attribute",
        files: &[("templates/x.html", "<p style=\"--c: red\">x</p>\n")],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "colour literal in an SVG presentation attribute",
        files: &[(
            "templates/x.html",
            "<svg><line stroke=\"#c42b22\"/></svg>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "literal stroke-width SVG attribute",
        files: &[(
            "templates/x.html",
            "<svg><line stroke-width=\"3\"/></svg>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "style element with attributes is scanned",
        files: &[(
            "templates/x.html",
            "<style media=\"screen\">.a { color: #fff; }</style>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "uppercase style element is scanned",
        files: &[("templates/x.html", "<STYLE>.a { color: #fff; }</STYLE>\n")],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "logical border side with literal width",
        files: &[(
            "static/x.css",
            ".a { border-inline-start: 4px solid var(--color-line); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "radius longhand with literal",
        files: &[("static/x.css", ".a { border-top-left-radius: 8px; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "logical border width longhand with literal",
        files: &[("static/x.css", ".a { border-block-end-width: 2px; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "component class defined in a style element of a foreign template",
        files: &[(
            "templates/pages/x.html",
            "<style>.check-row { display: block; }</style>\n",
        )],
        expect: Some((OWNERSHIP_RULE, "templates/pages/x.html")),
    },
    Case {
        name: "component class next to template syntax in a foreign template",
        files: &[(
            "templates/pages/x.html",
            "<div class=\"check-row{% if open %} is-open{% endif %}\">x</div>\n",
        )],
        expect: Some((OWNERSHIP_RULE, "templates/pages/x.html")),
    },
    Case {
        name: "uppercase inline script",
        files: &[("templates/x.html", "<SCRIPT>alert(1)</SCRIPT>\n")],
        expect: Some((SCRIPT_RULE, "templates/x.html")),
    },
    Case {
        name: "data-src is no src",
        files: &[(
            "templates/x.html",
            "<script data-src=\"x\">alert(1)</script>\n",
        )],
        expect: Some((SCRIPT_RULE, "templates/x.html")),
    },
    Case {
        name: "self-closing slash script",
        files: &[("templates/x.html", "<script/>alert(1)</script>\n")],
        expect: Some((SCRIPT_RULE, "templates/x.html")),
    },
    Case {
        name: "radius from the wrong token family",
        files: &[(
            "static/x.css",
            ".a { border-radius: var(--font-size-grade); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "font-size from the wrong token family",
        files: &[("static/x.css", ".a { font-size: var(--space-9); }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "oklab colour function",
        files: &[("static/x.css", ".a { color: oklab(0.6 0.1 0.1); }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "system colour",
        files: &[("static/x.css", ".a { color: CanvasText; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "color-mix function",
        files: &[(
            "static/x.css",
            ".a { color: color-mix(in srgb, var(--color-ink) 40%, var(--color-bg)); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "lch colour function",
        files: &[("static/x.css", ".a { color: lch(50% 30 40); }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "component element class in a foreign stylesheet",
        files: &[(
            "static/other.css",
            ".check-row__title { color: var(--color-ink); }\n",
        )],
        expect: Some((OWNERSHIP_RULE, "static/other.css")),
    },
    Case {
        name: "component prefixed class in a foreign template",
        files: &[(
            "templates/pages/x.html",
            "<span class=\"check-row-status\">x</span>\n",
        )],
        expect: Some((OWNERSHIP_RULE, "templates/pages/x.html")),
    },
    Case {
        name: "named colour in an island string literal",
        files: &[("islands/x.ts", "el.style.color = \"red\";\n")],
        expect: Some((TOKEN_RULE, "islands/x.ts")),
    },
    Case {
        name: "border-left with line token and transparent passes",
        files: &[(
            "static/x.css",
            ".a { border-left: var(--line-marker) solid transparent; }\n",
        )],
        expect: None,
    },
    Case {
        name: "marker comment naming a component is no definition",
        files: &[(
            "static/base.css",
            "/* marker rule, used by .grouping-switch */\nbody { margin: 0; }\n",
        )],
        expect: None,
    },
    // A string literal is a colour only when it is exactly a hex colour or the hex follows a CSS
    // value context; the selector `#feed` is no colour, and comments never count.
    Case {
        name: "island selector string and comment words are no colour",
        files: &[(
            "islands/x.ts",
            "const a = Math.tan(theta); // the red line\ndocument.querySelector('#feed');\n",
        )],
        expect: None,
    },
    Case {
        name: "CSS-wide keywords and zero pass",
        files: &[(
            "static/x.css",
            ".a { font: inherit; border: 0; border-radius: 0; }\n",
        )],
        expect: None,
    },
    Case {
        name: "component class with element in its own macro file",
        files: &[(
            "templates/components/check_row.html",
            "<div class=\"check-row\"><span class=\"check-row__title\">x</span></div>\n",
        )],
        expect: None,
    },
    Case {
        name: "component prefixed class in its own stylesheet",
        files: &[(
            "static/check_row.css",
            ".check-row-status { color: var(--color-ink); }\n",
        )],
        expect: None,
    },
    Case {
        name: "script with src passes",
        files: &[("templates/x.html", "<script src=\"/x.js\"></script>\n")],
        expect: None,
    },
    Case {
        name: "font and stroke tokens pass",
        files: &[(
            "static/x.css",
            ".a { font-family: var(--font-mono); font-size: var(--font-size-body); stroke-width: var(--line-hair); }\n",
        )],
        expect: None,
    },
    // Amended requirement 6, third reading.
    Case {
        name: "askama expression in an SVG tag still checks its colour",
        files: &[(
            "templates/x.html",
            "<svg><rect {% if n > 0 %}fill=\"#c42b22\"{% endif %}/></svg>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "askama expression in a foreign template tag still checks ownership",
        files: &[(
            "templates/pages/x.html",
            "<p {% if n > 0 %}class=\"check-row\"{% endif %}>x</p>\n",
        )],
        expect: Some((OWNERSHIP_RULE, "templates/pages/x.html")),
    },
    Case {
        name: "askama comparison does not hide an inline script",
        files: &[(
            "templates/x.html",
            "{% if rows.len()<max %}\n<script>alert(1)</script>{% endif %}\n",
        )],
        expect: Some((SCRIPT_RULE, "templates/x.html")),
    },
    Case {
        name: "literal font-size SVG attribute",
        files: &[(
            "templates/x.html",
            "<svg><text font-size=\"40\">net</text></svg>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "literal font-family SVG attribute",
        files: &[(
            "templates/x.html",
            "<svg><text font-family=\"Arial\">net</text></svg>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "vendor-prefixed radius",
        files: &[("static/x.css", ".a { -webkit-border-radius: 50%; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "vendor-prefixed radius longhand",
        files: &[(
            "static/x.css",
            ".a { -webkit-border-top-left-radius: 9px; }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "deprecated system colours",
        files: &[(
            "static/x.css",
            ".a { color: WindowText; background: ButtonHighlight; }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "colour literal in a data URI",
        files: &[(
            "static/x.css",
            ".a { background-image: url(\"data:image/svg+xml;utf8,<svg><rect fill='red'/></svg>\"); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal rx SVG attribute",
        files: &[(
            "templates/x.html",
            "<svg><rect rx=\"8\" width=\"10\" height=\"10\"/></svg>\n",
        )],
        expect: Some((TOKEN_RULE, "templates/x.html")),
    },
    Case {
        name: "rx as a CSS property",
        files: &[("static/x.css", ".a { rx: 6px; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal outline width",
        files: &[(
            "static/x.css",
            ".a { outline: 2px solid var(--color-ink); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "literal text-decoration-thickness",
        files: &[("static/x.css", ".a { text-decoration-thickness: 3px; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "inline event handler",
        files: &[(
            "templates/x.html",
            "<button onclick=\"alert(1)\">x</button>\n",
        )],
        expect: Some((SCRIPT_RULE, "templates/x.html")),
    },
    Case {
        name: "javascript URL",
        files: &[(
            "templates/x.html",
            "<a href=\"javascript:alert(1)\">x</a>\n",
        )],
        expect: Some((SCRIPT_RULE, "templates/x.html")),
    },
    Case {
        name: "literal outline-width",
        files: &[("static/x.css", ".a { outline-width: 3px; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "non-colour words that look like colours pass",
        files: &[(
            "static/x.css",
            ".a { grid-area: mark; animation-name: highlight; background-image: url(/static/red.png); }\n",
        )],
        expect: None,
    },
    Case {
        name: "tan() is no colour",
        files: &[(
            "static/x.css",
            ".a { width: calc(tan(45deg) * var(--space-1)); }\n",
        )],
        expect: None,
    },
    Case {
        name: "fill with a url reference passes",
        files: &[(
            "templates/x.html",
            "<svg><rect fill=\"url(#fade)\"/></svg>\n",
        )],
        expect: None,
    },
    Case {
        name: "font tokens in SVG attributes pass",
        files: &[(
            "templates/x.html",
            "<svg><text font-size=\"var(--font-size-caption)\" font-family=\"var(--font-mono)\">x</text></svg>\n",
        )],
        expect: None,
    },
    Case {
        name: "outline and decoration line tokens pass",
        files: &[(
            "static/x.css",
            ".a { outline: var(--line-marker) solid var(--color-ink); text-decoration-thickness: var(--line-hair); }\n",
        )],
        expect: None,
    },
    Case {
        name: "rx token passes",
        files: &[(
            "templates/x.html",
            "<svg><rect rx=\"var(--radius-max)\"/></svg>\n",
        )],
        expect: None,
    },
    Case {
        name: "data-on attribute is no event handler",
        files: &[(
            "templates/x.html",
            "<button type=\"button\" data-on=\"x\">x</button>\n",
        )],
        expect: None,
    },
    Case {
        name: "less-than in an askama condition passes",
        files: &[("templates/x.html", "<p>{% if a < b %}x{% endif %}</p>\n")],
        expect: None,
    },
    Case {
        name: "askama-valued script src is an external script",
        files: &[(
            "templates/x.html",
            "<script type=\"module\" src=\"{{ asset }}\"></script>\n",
        )],
        expect: None,
    },
    Case {
        name: "askama-valued script src with whitespace control passes",
        files: &[(
            "templates/x.html",
            "<script src=\"{{- asset -}}\" defer></script>\n",
        )],
        expect: None,
    },
    Case {
        name: "empty script src is still inline",
        files: &[("templates/x.html", "<script src=\"\">alert(1)</script>\n")],
        expect: Some((SCRIPT_RULE, "templates/x.html")),
    },
    Case {
        name: "mask-image gradient is refused",
        files: &[(
            "static/x.css",
            ".a { mask-image: linear-gradient(to right, black 90%, transparent); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "vendor-prefixed colour property is refused",
        files: &[("static/x.css", ".a { -webkit-tap-highlight-color: red; }\n")],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "backdrop-filter drop-shadow is refused",
        files: &[(
            "static/x.css",
            ".a { backdrop-filter: drop-shadow(0 0 4px black); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "list-style-image gradient is refused",
        files: &[(
            "static/x.css",
            ".a { list-style-image: linear-gradient(red, blue); }\n",
        )],
        expect: Some((TOKEN_RULE, "static/x.css")),
    },
    Case {
        name: "colour-like identifiers and strings stay allowed",
        files: &[(
            "static/x.css",
            ".a { grid-area: mark; animation-name: highlight; counter-reset: red; font-family: var(--font-sans); content: \"red\"; }\n",
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
