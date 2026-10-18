//! Phase 4 (spec v2-result-page, requirement 23): the countdown island exists, is registered and
//! passes the design gate; pages.css dims a refreshing result and a running stream form.
use std::fs;
use std::path::Path;

use netray_web::gate::check;

fn violations_in(rel: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    check(root)
        .violations
        .iter()
        .filter(|v| v.path == Path::new(rel))
        .map(|v| format!("rule {} {}:{} {}", v.rule, v.path.display(), v.line, v.message))
        .collect()
}

#[test]
fn countdown_island_exists_is_registered_and_passes_the_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        root.join("islands/src/countdown.ts").is_file(),
        "islands/src/countdown.ts is missing"
    );
    let main = fs::read_to_string(root.join("islands/src/main.ts")).unwrap();
    assert!(
        main.contains("register(\"countdown\""),
        "main.ts does not register the countdown island"
    );
    let found = violations_in("islands/src/countdown.ts");
    assert!(found.is_empty(), "gate violations: {found:#?}");
}

#[test]
fn pages_css_dims_refreshing_bands_and_a_running_stream_and_passes_the_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let css = fs::read_to_string(root.join("static/pages.css")).unwrap();
    let selectors: Vec<&str> = css
        .split('{')
        .map(|chunk| chunk.rsplit(['}', ';']).next().unwrap_or(chunk).trim())
        .collect();

    assert!(
        selectors
            .iter()
            .any(|s| s.contains("[data-refreshing]") && s.contains(":not([data-live])")),
        "no rule dims [data-refreshing] bands lacking data-live"
    );
    assert!(
        selectors.iter().any(|s| s.contains("[data-stream-state=\"running\"]")
            || s.contains("[data-stream-state=running]")),
        "no rule dims the result while the stream form runs"
    );
    let found = violations_in("static/pages.css");
    assert!(found.is_empty(), "gate violations: {found:#?}");
}
