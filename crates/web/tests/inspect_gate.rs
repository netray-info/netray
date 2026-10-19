//! specs/features/v2-inspectors, requirement 21: the inspector feature's own files pass the §8.5
//! gate, hold no codename as a whole word, and the dns island is on disk and scanned.
use std::fs;
use std::path::{Path, PathBuf};

use netray_web::gate::check;

const WORDS: [&str; 6] = ["lens", "prism", "tlsight", "spectra", "beacon", "ifconfig"];

const FIXED: [&str; 6] = [
    "templates/pages/inspector.html",
    "templates/pages/evidence.html",
    "templates/partials/evidence_section.html",
    "templates/components/evidence_block.html",
    "static/evidence_block.css",
    "islands/src/dns.ts",
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The feature's files: the fixed list plus every `templates/partials/inspect_*.html` on disk.
fn feature_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = FIXED.iter().map(PathBuf::from).collect();
    let partials = fs::read_dir(root().join("templates/partials")).expect("partials dir");
    for entry in partials.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("inspect_") && name.ends_with(".html") {
            files.push(Path::new("templates/partials").join(name));
        }
    }
    files.sort();
    files
}

/// Words of `text` split on anything but ASCII letters and digits (`-` and `_` split), lowercased.
fn codename_in(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find_map(|w| WORDS.iter().copied().find(|c| *c == w))
}

fn count_ts(dir: &Path) -> usize {
    let mut n = 0;
    for path in fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
    {
        if path.is_dir() {
            if path.file_name().is_some_and(|f| f != "node_modules") {
                n += count_ts(&path);
            }
        } else if path.extension().is_some_and(|e| e == "ts") {
            n += 1;
        }
    }
    n
}

#[test]
fn codename_rule_is_whole_word() {
    assert_eq!(codename_in("How Lens scores"), Some("lens"));
    assert_eq!(codename_in("mhost-prism"), Some("prism"));
    assert_eq!(codename_in("the lensing effect"), None);
}

#[test]
fn inspector_files_pass_the_gate() {
    let files = feature_files();
    let report = check(root());
    let findings: Vec<String> = report
        .violations
        .iter()
        .filter(|v| files.contains(&v.path))
        .map(|v| {
            format!(
                "rule {} {}:{} {}",
                v.rule,
                v.path.display(),
                v.line,
                v.message
            )
        })
        .collect();
    assert!(findings.is_empty(), "gate findings: {findings:#?}");
}

#[test]
fn inspector_files_name_no_codename() {
    let mut hits = Vec::new();
    for rel in feature_files() {
        let text = fs::read_to_string(root().join(&rel))
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", rel.display()));
        if let Some(word) = codename_in(&text) {
            hits.push(format!("{} holds `{word}`", rel.display()));
        }
    }
    assert!(hits.is_empty(), "codenames: {hits:#?}");
}

#[test]
fn dns_island_exists_and_is_scanned() {
    assert!(
        root().join("islands/src/dns.ts").is_file(),
        "islands/src/dns.ts is missing"
    );
    let report = check(root());
    let expected = count_ts(&root().join("islands"))
        + count_files(&root().join("templates"), "html")
        + count_files(&root().join("static"), "css");
    assert_eq!(report.scanned, expected, "gate scanned count");
    assert!(
        count_ts(&root().join("islands")) >= 5,
        "dns.ts must be among the scanned islands"
    );
}

fn count_files(dir: &Path, ext: &str) -> usize {
    let mut n = 0;
    for path in fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
    {
        if path.is_dir() {
            n += count_files(&path, ext);
        } else if path.extension().is_some_and(|e| e == ext) {
            n += 1;
        }
    }
    n
}
