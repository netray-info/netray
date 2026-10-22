// `netray serve` ignores the repository tooling's own `NETRAY_` variables (v2-cutover Phase 7
// review): `NETRAY_BIN` (tests/repo/lib/netray.sh, a prebuilt binary) and `NETRAY_DESIGN_DIR`
// (`just design-check`'s canvas) reach every process the scripts start, and are no config keys;
// `NETRAY_LIVE_TESTS` is filtered the same way. Any other unknown `NETRAY_` key still fails.

use std::process::Command;

fn check(env: &[(&str, &str)]) -> (Option<i32>, String) {
    let config = concat!(env!("CARGO_MANIFEST_DIR"), "/netray.dev.toml");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_netray"));
    cmd.env_clear();
    if let Ok(v) = std::env::var("PATH") {
        cmd.env("PATH", v);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd
        .args(["serve", "--check-config", config])
        .output()
        .unwrap();
    (
        out.status.code(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn tooling_variables_are_no_config_keys() {
    for var in ["NETRAY_BIN", "NETRAY_DESIGN_DIR", "NETRAY_LIVE_TESTS"] {
        let (code, text) = check(&[(var, "/tmp/x")]);
        assert_eq!(code, Some(0), "{var} set: {text}");
        assert!(text.contains("config ok:"), "{var} set: {text}");
    }
}

#[test]
fn another_unknown_netray_variable_still_fails() {
    let (code, text) = check(&[("NETRAY_BOGUS", "1")]);
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("bogus"), "{text}");
}
