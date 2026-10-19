//! Contract tests for the single tagline (V2 spec v2-design-system, requirement 9,
//! criteria C4 and C6): the partial holds it, no other template or stylesheet repeats it.
use std::path::{Path, PathBuf};

const TAGLINE: &str = "Every check of a domain, in one spectrum.";
const PARTIAL: &str = "templates/partials/tagline.html";

/// All files below `dir` with the given extension; a missing directory is empty.
fn files_with_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_with_ext(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// Files under `root` (templates/**/*.html, static/**/*.css) containing the tagline,
/// case-insensitively, as paths relative to `root` with `/` separators. The partial is excluded.
fn copies_outside_partial(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    files_with_ext(&root.join("templates"), "html", &mut files);
    files_with_ext(&root.join("static"), "css", &mut files);
    let needle = TAGLINE.to_lowercase();
    let mut found: Vec<String> = files
        .iter()
        .filter_map(|p| {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if rel == PARTIAL {
                return None;
            }
            let text = std::fs::read_to_string(p).unwrap_or_default();
            text.to_lowercase().contains(&needle).then_some(rel)
        })
        .collect();
    found.sort();
    found
}

fn web_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn partial_holds_exactly_the_tagline() {
    let path = web_root().join(PARTIAL);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is missing or unreadable: {e}", path.display()));
    assert_eq!(
        text.trim(),
        TAGLINE,
        "{PARTIAL} must hold exactly the tagline"
    );
}

#[test]
fn tagline_appears_in_no_other_template_or_stylesheet() {
    let found = copies_outside_partial(&web_root());
    assert!(
        found.is_empty(),
        "tagline duplicated outside {PARTIAL}: {}",
        found.join(", ")
    );
}

#[test]
fn search_helper_finds_a_planted_nested_copy() {
    let root = std::env::temp_dir().join(format!("netray-tagline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let nested = root.join("templates/pages/deep");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(root.join("templates/partials")).unwrap();
    std::fs::write(root.join(PARTIAL), format!("{TAGLINE}\n")).unwrap();
    std::fs::write(
        nested.join("copy.html"),
        format!("<p>{}</p>\n", TAGLINE.to_uppercase()),
    )
    .unwrap();

    let found = copies_outside_partial(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(found, vec!["templates/pages/deep/copy.html".to_string()]);
}
