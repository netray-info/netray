// The `[api]` table (specs/features/v2-api, Phase 1, requirement 8): defaults, known keys,
// empty per-module tables, and strict keys in `[api]` and `[api.ip]`.

use lens::config::Config;

fn load_toml(contents: &str) -> Result<Config, impl std::fmt::Display> {
    let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    std::io::Write::write_all(&mut file, contents.as_bytes()).unwrap();
    Config::load(Some(file.path().to_str().unwrap()))
}

fn load_ok(contents: &str) -> Config {
    match load_toml(contents) {
        Ok(c) => c,
        Err(e) => panic!("config must load: {e}\n---\n{contents}"),
    }
}

fn load_err(contents: &str) -> String {
    match load_toml(contents) {
        Ok(_) => panic!("config must be refused:\n{contents}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn api_table_loading_and_strict_keys() {
    // C32: defaults without the table.
    let cfg = load_ok("");
    assert!(cfg.api.enabled);
    assert_eq!(cfg.api.per_ip_per_minute, 60);
    assert_eq!(cfg.api.per_ip_burst, 10);

    // C32: known keys load as written.
    let cfg = load_ok("[api]\nenabled = false\nper_ip_per_minute = 5\nper_ip_burst = 2\n");
    assert!(!cfg.api.enabled);
    assert_eq!(cfg.api.per_ip_per_minute, 5);
    assert_eq!(cfg.api.per_ip_burst, 2);

    // C32: the empty per-module tables load.
    let cfg = load_ok("[api.dns]\n[api.tls]\n[api.http]\n[api.email]\n[api.ip]\n");
    assert!(cfg.api.enabled);
    assert_eq!(cfg.api.per_ip_per_minute, 60);

    // C32: an unknown key under [api] is rejected and named.
    let msg = load_err("[api]\nbogus = 1\n");
    assert!(
        msg.contains("bogus"),
        "error must name the unknown key, got: {msg}"
    );

    // C32: an unknown key under [api.ip] is rejected and named.
    let msg = load_err("[api.ip]\nbogus = 1\n");
    assert!(
        msg.contains("bogus"),
        "error must name the unknown key, got: {msg}"
    );
}
