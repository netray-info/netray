// The `[history]` table (specs/features/v2-history, Phase 4): strict keys (C18), defaults
// enabled = true and db_path = "history.db" (C15), and the `LENS_HISTORY__DB_PATH` override.

use lens::config::Config;

fn load_toml(contents: &str) -> Result<Config, impl std::fmt::Display> {
    let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    std::io::Write::write_all(&mut file, contents.as_bytes()).unwrap();
    Config::load(Some(file.path().to_str().unwrap()))
}

#[test]
fn history_table_rejects_an_unknown_key() {
    let msg = match load_toml("[history]\nretention_days = 30\n") {
        Ok(_) => panic!("an unknown [history] key must be rejected"),
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("retention_days"),
        "error must name the unknown key, got: {msg}"
    );
}

// One test, sequential: it is the only one in this binary that touches the process environment.
#[test]
fn history_defaults_and_env_override() {
    let cfg = match load_toml("") {
        Ok(c) => c,
        Err(e) => panic!("an empty config must load: {e}"),
    };
    assert!(cfg.history.enabled, "history is enabled by default");
    assert_eq!(cfg.history.db_path, std::path::PathBuf::from("history.db"));

    // SAFETY: no other test in this binary reads or writes the environment concurrently.
    unsafe { std::env::set_var("LENS_HISTORY__DB_PATH", "/var/lib/example.com/h.db") };
    let loaded = load_toml("");
    unsafe { std::env::remove_var("LENS_HISTORY__DB_PATH") };
    let cfg = match loaded {
        Ok(c) => c,
        Err(e) => panic!("config with env override must load: {e}"),
    };
    assert_eq!(
        cfg.history.db_path,
        std::path::PathBuf::from("/var/lib/example.com/h.db")
    );
}
