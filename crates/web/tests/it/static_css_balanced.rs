//! Every stylesheet under `static/` closes each block it opens: a merge that drops one brace
//! (v2-dns-ui, merging v2-ip-ui) nests every later rule inside the open one, and the browser
//! drops them without a test failing.
#[test]
fn every_stylesheet_has_balanced_braces() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "css") {
            continue;
        }
        let css = std::fs::read_to_string(&path).unwrap();
        let mut depth: i64 = 0;
        for (n, line) in css.lines().enumerate() {
            for c in line.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            assert!(
                depth >= 0,
                "{}:{} closes a block never opened",
                path.display(),
                n + 1
            );
            // A rule written at the left margin is a top-level rule: it never opens inside
            // another block (rules inside an at-rule are indented).
            if line.starts_with(|c: char| c == '.' || c == '#') && line.trim_end().ends_with('{') {
                assert!(
                    depth == 1,
                    "{}:{} opens a rule inside an open block",
                    path.display(),
                    n + 1
                );
            }
        }
        assert_eq!(depth, 0, "{} leaves {depth} block(s) open", path.display());
    }
}
