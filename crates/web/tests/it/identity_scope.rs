//! Scope test for the identity SVGs (V2 spec v2-pages, requirement 2, C2/C5): the root `<svg>`
//! carries an id and every selector of the embedded `<style>` starts with it, so an inlined copy
//! restyles nothing outside itself. Colours and geometry are covered by identity_colours.rs,
//! identity_logo.rs and identity_marks.rs.

/// (file, root id the file must carry).
const ROWS: [(&str, &str); 3] = [
    ("logo.svg", "netray-logo"),
    ("favicon.svg", "netray-favicon"),
    ("badge-mark.svg", "netray-badge-mark"),
];

/// The value of `id="..."` on the root `<svg ...>` tag, if any.
fn root_id(src: &str) -> Option<String> {
    let start = src.find("<svg")?;
    let tag = &src[start..start + src[start..].find('>')?];
    let i = tag.find(" id=\"")? + 5;
    Some(tag[i..i + tag[i..].find('"')?].to_string())
}

/// Every selector prelude (text before a `{` whose block holds declarations), descending into
/// at-rules such as `@media`. The at-rule preludes themselves are not selectors.
fn selectors(css: &str, out: &mut Vec<String>) {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let prelude = rest[..open].trim();
        let mut depth = 1;
        let mut end = open + 1;
        for (i, c) in rest[open + 1..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                end = open + 1 + i;
                break;
            }
        }
        let body = &rest[open + 1..end];
        if prelude.starts_with('@') {
            selectors(body, out);
        } else {
            out.push(prelude.to_string());
        }
        rest = &rest[(end + 1).min(rest.len())..];
    }
}

#[test]
fn identity_svgs_scope_every_style_selector_to_the_root_id() {
    let mut bad = Vec::new();
    for (file, want) in ROWS {
        let path = format!("{}/static/{file}", env!("CARGO_MANIFEST_DIR"));
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{file}: cannot read {path}: {e}"));

        match root_id(&src) {
            Some(id) if id == want => {}
            got => bad.push(format!("{file}: root <svg> id is {got:?}, want {want:?}")),
        }

        let s = src.find("<style").expect("style element");
        let css_start = s + src[s..].find('>').expect("style tag end") + 1;
        let css =
            &src[css_start..css_start + src[css_start..].find("</style>").expect("style end")];
        let mut sels = Vec::new();
        selectors(css, &mut sels);
        assert!(!sels.is_empty(), "{file}: no selectors found in <style>");
        let hash = format!("#{want}");
        for list in sels {
            for sel in list.split(',').map(str::trim) {
                let scoped = sel.strip_prefix(&hash).is_some_and(|r| {
                    r.chars()
                        .next()
                        .is_none_or(|c| c.is_whitespace() || ".:[>+~".contains(c))
                });
                if !scoped {
                    bad.push(format!(
                        "{file}: selector {sel:?} does not begin with {hash}"
                    ));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "unscoped identity SVGs:\n{}",
        bad.join("\n")
    );
}
