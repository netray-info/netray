//! Integration tests for `netray_common::config::load` / `load_with_env`.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use netray_common::config::{ConfigError, load_with_env};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    per_ip_per_minute: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestConfig {
    limits: Limits,
}

fn env(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect()
}

fn load_toml(name: &str, toml: &str) -> Result<TestConfig, ConfigError> {
    let path = std::env::temp_dir().join(format!(
        "netray-common-config-loader-{}-{name}.toml",
        std::process::id()
    ));
    std::fs::write(&path, toml).unwrap();
    let result = load_with_env::<TestConfig>(
        Some(path.to_str().unwrap()),
        "PRISM_",
        Vec::<(OsString, OsString)>::new(),
    );
    let _ = std::fs::remove_file(&path);
    result
}

#[test]
fn unknown_key_in_toml_file_is_rejected_and_named() {
    let err = load_toml(
        "unknown-key",
        "[limits]\nper_ip_per_minute = 5\nper_ip_per_hour = 9\n",
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("per_ip_per_hour"), "{err}");
}

#[test]
fn env_overrides_with_each_prefix_style_and_config_var_is_not_a_key() {
    // (prefix, config-file variable, value variable)
    let cases = [
        ("PRISM_", "PRISM_CONFIG", "PRISM_LIMITS__PER_IP_PER_MINUTE"),
        (
            "SPECTRA__",
            "SPECTRA_CONFIG",
            "SPECTRA__LIMITS__PER_IP_PER_MINUTE",
        ),
        (
            "BEACON__",
            "BEACON_CONFIG",
            "BEACON__LIMITS__PER_IP_PER_MINUTE",
        ),
    ];
    for (prefix, config_var, value_var) in cases {
        let cfg = load_with_env::<TestConfig>(
            None,
            prefix,
            env(&[(config_var, "/x.toml"), (value_var, "60")]),
        )
        .unwrap_or_else(|e| panic!("prefix {prefix}: {e}"));
        assert_eq!(cfg.limits.per_ip_per_minute, 60, "prefix {prefix}");
    }
}

#[test]
fn variables_of_another_prefix_are_ignored() {
    let cfg = load_with_env::<TestConfig>(
        None,
        "PRISM_",
        env(&[
            ("PRISM_LIMITS__PER_IP_PER_MINUTE", "60"),
            ("LENS_X__Y", "1"),
        ]),
    )
    .unwrap();
    assert_eq!(cfg.limits.per_ip_per_minute, 60);
}

#[test]
fn non_utf8_env_entries_do_not_panic() {
    let bad = || OsString::from_vec(vec![0xff, 0xfe]);
    let vars = vec![
        (
            OsString::from("PRISM_LIMITS__PER_IP_PER_MINUTE"),
            OsString::from("60"),
        ),
        (OsString::from("PRISM_BADVALUE"), bad()),
        (bad(), OsString::from("1")),
    ];
    let cfg = load_with_env::<TestConfig>(None, "PRISM_", vars).unwrap();
    assert_eq!(cfg.limits.per_ip_per_minute, 60);
}
