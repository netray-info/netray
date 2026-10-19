//! Cross-document view transitions (V2 spec v2-pages, requirement 13; criteria C6, C18):
//! `base.css` opts in with `@view-transition { navigation: auto; }`, and every
//! `@view-transition` in the shipped stylesheets sits inside
//! `@media (prefers-reduced-motion: no-preference) { ... }`.
use std::fs;
use std::path::PathBuf;

const MEDIA: &str = "@media(prefers-reduced-motion:no-preference){";
const RULE: &str = "@view-transition{navigation:auto;}";

fn static_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static")
}

/// The stylesheet without `/* ... */` comments and without any whitespace.
fn squash(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = match rest[start + 2..].find("*/") {
            Some(end) => &rest[start + 2 + end + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Spans (start of the `{` .. index of its matching `}`) of every reduced-motion media block.
fn media_spans(s: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(i) = s[from..].find(MEDIA) {
        let open = from + i + MEDIA.len() - 1;
        let mut depth = 0usize;
        let mut close = s.len();
        for (j, c) in s[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + j;
                        break;
                    }
                }
                _ => {}
            }
        }
        spans.push((open, close));
        from = open + 1;
    }
    spans
}

/// Err names the first `@view-transition` outside a reduced-motion block.
fn all_guarded(css: &str) -> Result<usize, String> {
    let s = squash(css);
    let spans = media_spans(&s);
    let mut count = 0;
    for (i, _) in s.match_indices("@view-transition") {
        count += 1;
        if !spans.iter().any(|&(a, b)| a < i && i < b) {
            return Err(format!(
                "@view-transition at {i} is outside the media block"
            ));
        }
    }
    Ok(count)
}

fn has_opt_in(css: &str) -> bool {
    squash(css).contains(RULE)
}

#[test]
fn base_css_opts_in_to_view_transitions_inside_the_reduced_motion_block() {
    let css = fs::read_to_string(static_dir().join("base.css")).expect("static/base.css");
    assert!(
        has_opt_in(&css),
        "base.css lacks `@view-transition {{ navigation: auto; }}`"
    );
    all_guarded(&css).unwrap_or_else(|e| panic!("base.css: {e}"));
}

#[test]
fn no_stylesheet_holds_view_transition_outside_the_block() {
    let mut seen = 0;
    for entry in fs::read_dir(static_dir()).expect("static dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "css") {
            let css = fs::read_to_string(&path).expect("read css");
            all_guarded(&css).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            seen += 1;
        }
    }
    assert!(seen > 1, "expected several stylesheets, saw {seen}");
}

#[test]
fn checker_rejects_top_level_and_accepts_guarded_rule() {
    let top = "a { color: red; }\n@view-transition { navigation: auto; }\n";
    assert!(all_guarded(top).is_err());
    let guarded = "/* x */ @media (prefers-reduced-motion: no-preference) {\n  @view-transition { navigation: auto; }\n}\n";
    assert_eq!(all_guarded(guarded), Ok(1));
    assert!(has_opt_in(guarded));
    let other = "@media (min-width: 1px) { @view-transition { navigation: auto; } }";
    assert!(all_guarded(other).is_err());
    let commented = "/* @view-transition { navigation: auto; } */ a { b: c; }";
    assert!(!has_opt_in(commented));
}
