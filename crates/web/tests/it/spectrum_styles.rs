//! Spectrum label styles (V2 spec v2-spectrum, requirement 16, Phase 3; criteria C2, C15, C16,
//! C18): the stylesheet, the label tokens, the template's use of them, and the gate over both.
use netray_web::gate::check;
use netray_web::tokens::{CSS, values};
use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = crate_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The flat `(prelude, body)` rules of a stylesheet without at-rules' nesting.
fn rules(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        out.push((
            squash(&rest[..open]),
            rest[open + 1..open + close].to_string(),
        ));
        rest = &rest[open + close + 1..];
    }
    out
}

/// The last value `prop` takes over all rules whose selector list holds `selector`.
fn resolve(css: &str, selector: &str, prop: &str) -> Option<String> {
    let wanted = squash(selector);
    let mut found = None;
    for (prelude, body) in rules(css) {
        if prelude.split(',').any(|s| s == wanted) {
            for decl in body.split(';') {
                if let Some((k, v)) = decl.split_once(':')
                    && k.trim() == prop
                {
                    found = Some(v.trim().to_string());
                }
            }
        }
    }
    found
}

#[test]
fn c15_spectrum_stylesheet_resolves_label_rules() {
    let css = read("static/spectrum.css");
    assert_eq!(
        resolve(&css, ".spectrum", "overflow-x").as_deref(),
        Some("auto")
    );
    assert_eq!(
        resolve(&css, ".spectrum__line-label", "font-size").as_deref(),
        Some("var(--font-size-label)")
    );
    assert_eq!(
        resolve(&css, ".spectrum__line-label", "font-family").as_deref(),
        Some("var(--font-mono)")
    );
    assert_eq!(
        resolve(&css, ".spectrum__line-label", "background").as_deref(),
        Some("var(--color-bg)")
    );
    assert!(
        !css.contains("--font-size-caption"),
        "the label no longer uses the caption size"
    );
    let hides = rules(&css).iter().any(|(prelude, body)| {
        prelude.contains("[data-run]:not([data-shown])")
            && body.split(';').any(|d| squash(d) == "display:none")
    });
    assert!(hides, "no rule hides `[data-run]:not([data-shown])`");
}

#[test]
fn c16_label_tokens_are_generated() {
    let root_decls: Vec<(String, String)> = rules(CSS)
        .into_iter()
        .filter(|(prelude, _)| prelude == ":root")
        .flat_map(|(_, body)| {
            body.split(';')
                .filter_map(|d| d.split_once(':'))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect::<Vec<_>>()
        })
        .collect();
    let get = |name: &str| {
        root_decls
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("--spectrum-label-leader"), Some("8px"));
    assert_eq!(get("--spectrum-label-row"), Some("14px"));
    assert_eq!(values::SPECTRUM_LABEL_LEADER, "8px");
    assert_eq!(values::SPECTRUM_LABEL_ROW, "14px");
}

#[test]
fn c2_template_sizes_labels_by_tokens_not_px() {
    let html = read("templates/components/spectrum.html");
    assert!(html.contains("var(--spectrum-label-leader)"));
    assert!(html.contains("var(--spectrum-label-row)"));
    for decl in html.split(['"', ';']) {
        let d = decl.trim();
        if d.starts_with("bottom:") || d.starts_with("height:") {
            assert!(!d.contains("px"), "px in a bottom/height calc: {d}");
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            fs::copy(&src, &dst).unwrap();
        }
    }
}

#[test]
fn c18_gate_passes_the_crate_and_flags_only_the_planted_font_size() {
    let real = check(&crate_root());
    assert!(
        real.violations.is_empty(),
        "the crate tree breaks the gate: {:?}",
        real.violations
            .iter()
            .map(|v| format!("{}:{} {}", v.path.display(), v.line, v.message))
            .collect::<Vec<_>>()
    );

    let scratch = std::env::temp_dir().join(format!("netray-spectrum-gate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    copy_dir(&crate_root().join("templates"), &scratch.join("templates"));
    copy_dir(&crate_root().join("static"), &scratch.join("static"));
    fs::copy(
        crate_root().join("tokens.json"),
        scratch.join("tokens.json"),
    )
    .unwrap();
    let target = scratch.join("templates/components/spectrum.html");
    let mut html = fs::read_to_string(&target).unwrap();
    html.push_str(
        "\n<span class=\"spectrum__line-label\" data-tone=\"warn\" data-code=\"x\" \
         style=\"bottom: 22px; font-size: 12px\">x</span>\n",
    );
    fs::write(&target, html).unwrap();

    let report = check(&scratch);
    let _ = fs::remove_dir_all(&scratch);
    let msgs: Vec<String> = report
        .violations
        .iter()
        .map(|v| format!("{}: {}", v.path.display(), v.message))
        .collect();
    assert_eq!(msgs.len(), 1, "expected exactly one violation: {msgs:?}");
    assert!(msgs[0].contains("font-size"), "{msgs:?}");
    assert!(!msgs[0].contains("bottom"), "{msgs:?}");
}
