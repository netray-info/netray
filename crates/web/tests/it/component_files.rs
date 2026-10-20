//! Contract test for the R44 component files (V2 spec v2-design-system, requirement 7, R43/R44).
//! Each component exists once as an askama macro and once as a stylesheet; base.css holds the marker.
use std::fs;
use std::path::{Path, PathBuf};

const NAMES: [&str; 11] = [
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
    "facts_grid",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// All `*.html` files below `dir`; a missing directory yields nothing.
fn html_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            html_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "html") {
            out.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

#[test]
fn every_component_has_one_macro_file_and_one_stylesheet() {
    let templates = root().join("templates");
    let mut all = Vec::new();
    html_files(&templates, &mut all);

    for name in NAMES {
        let file = templates.join("components").join(format!("{name}.html"));
        let src = read(&file);
        let needle = format!("{{% macro {name}(");
        assert_eq!(
            src.matches(&needle).count(),
            1,
            "{} must contain `{needle}` exactly once",
            file.display()
        );

        let definers: Vec<_> = all
            .iter()
            .filter(|p| read(p).contains(&needle))
            .map(|p| p.display().to_string())
            .collect();
        assert_eq!(
            definers,
            vec![file.display().to_string()],
            "macro `{name}` must be defined in exactly one template"
        );

        let css_file = root().join("static").join(format!("{name}.css"));
        let css = read(&css_file);
        let class = format!(".{}", name.replace('_', "-"));
        let defined = css.match_indices(&class).any(|(i, _)| {
            css[i + class.len()..]
                .chars()
                .next()
                .is_some_and(|c| matches!(c, ' ' | '{' | ':' | '[' | ',' | '.' | '>' | '\n'))
        });
        assert!(
            defined,
            "{} must define the root class selector `{class}`",
            css_file.display()
        );
    }
}

#[test]
fn base_css_holds_the_marker_rule() {
    let file = root().join("static").join("base.css");
    let css = read(&file);
    for needle in [
        ".marker",
        "var(--line-marker)",
        "var(--color-ink)",
        "aria-pressed=\"true\"",
        "aria-current",
        "aria-selected=\"true\"",
    ] {
        assert!(
            css.contains(needle),
            "{} must contain `{needle}` (marker rule, R43)",
            file.display()
        );
    }
}
