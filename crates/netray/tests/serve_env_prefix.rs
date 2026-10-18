// `netray serve` refuses every retired env prefix (specs/features/v2-cutover, Phase 1,
// requirement 10): C3/C10 the eleven legacy prefixes, case-insensitively, C11 the per-module
// `NETRAY_<P>_` prefixes, and the neighbour `NETRAY_SERVER__*` that stays valid.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const VALID_CONFIG: &str = "[server]\nbind = \"127.0.0.1:0\"\nmetrics_bind = \"127.0.0.1:0\"\n";

const LEGACY_PREFIXES: [&str; 11] = [
    "LENS_",
    "NETRAY_DNS_",
    "NETRAY_TLS_",
    "NETRAY_HTTP_",
    "NETRAY_EMAIL_",
    "NETRAY_IP_",
    "PRISM_",
    "TLSIGHT_",
    "SPECTRA_",
    "BEACON_",
    "IFCONFIG_",
];

/// Runs `netray <args>` with a cleared environment plus `var`; kills it after 10 s.
/// Returns (exit code or None when killed, combined output).
fn run(args: &[&str], var: (&str, &str)) -> (Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_netray"))
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .env(var.0, var.1)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let code = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status.code();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    child.stderr.take().unwrap().read_to_string(&mut out).unwrap();
    (code, out)
}

fn config_file() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("netray-serve-env-prefix-{}.toml", std::process::id()));
    std::fs::write(&path, VALID_CONFIG).unwrap();
    path
}

fn assert_refused(args: &[&str], var: &str) {
    let (code, out) = run(args, (var, "127.0.0.1:1"));
    assert_eq!(code, Some(1), "`netray {args:?}` with {var} must exit 1, output:\n{out}");
    for needle in [var, "NETRAY_*", "NETRAY_CONFIG"] {
        assert!(out.contains(needle), "output of {args:?} with {var} must name {needle}, got:\n{out}");
    }
}

#[test]
fn serve_refuses_retired_env_prefixes() {
    let file = config_file();
    let path = file.to_str().unwrap();

    // C3/C10 and C11: every legacy prefix, `netray serve` and `--check-config`.
    for prefix in LEGACY_PREFIXES {
        let var = format!("{prefix}SERVER__BIND");
        assert_refused(&["serve", "--check-config", path], &var);
        assert_refused(&["serve", path], &var);
    }

    // ASCII case-insensitive.
    assert_refused(&["serve", "--check-config", path], "lens_server__bind");
    assert_refused(&["serve", path], "lens_server__bind");

    // Neighbour: the new prefix alone is not refused.
    let (code, out) = run(&["serve", "--check-config", path], ("NETRAY_SERVER__BIND", "127.0.0.1:0"));
    assert_eq!(code, Some(0), "NETRAY_SERVER__BIND alone must pass the config check, output:\n{out}");
}
