//! SIGHUP reload (`reload.rs`) calls `Config::load`. A missing `custom_ca_dir` must
//! not make that fail: the existence check belongs to startup and to `--check-config`,
//! so a running server keeps its previous trust store instead of rejecting the reload.

use tlsight::config::Config;

#[test]
fn load_tolerates_missing_custom_ca_dir() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tlsight.production.toml"
    );
    let base = std::fs::read_to_string(fixture).expect("fixture must be readable");
    let patched = base.replacen(
        "[validation]\n",
        "[validation]\ncustom_ca_dir = \"/nonexistent-ca-dir-for-reload-test\"\n",
        1,
    );
    assert_ne!(base, patched, "substitution must change the fixture");

    let path = std::env::temp_dir().join(format!(
        "tlsight-reload-tolerance-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, patched).expect("write config");

    let result = Config::load(path.to_str());
    let _ = std::fs::remove_file(&path);
    assert!(
        result.is_ok(),
        "Config::load must tolerate a missing custom_ca_dir, got: {:?}",
        result.err()
    );
}
