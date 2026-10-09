//! The production config fixture must switch system resolvers off (C7).

use prism::config::Config;

fn fixture_path() -> String {
    format!(
        "{}/tests/fixtures/prism.production.toml",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn production_fixture_disables_system_resolvers() {
    let path = fixture_path();

    // The key must be literally present under [dns]: argus-oci compares key
    // paths, so the default (true) must not be what satisfies this test.
    let text = std::fs::read_to_string(&path).expect("read fixture");
    let mut in_dns = false;
    let mut found = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dns = line == "[dns]";
            continue;
        }
        if in_dns {
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            if compact == "allow_system_resolvers=false" {
                found = true;
            }
        }
    }
    assert!(
        found,
        "fixture [dns] table must set `allow_system_resolvers = false`"
    );

    let config = Config::load(Some(&path)).expect("load production fixture");
    assert!(
        !config.dns.allow_system_resolvers,
        "allow_system_resolvers must be false in the production fixture"
    );
}
