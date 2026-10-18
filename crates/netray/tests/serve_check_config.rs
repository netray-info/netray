// `netray serve --check-config` (specs/features/v2-cutover, Phase 1: requirements 1, 9, 12).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PREFIXES: [&str; 7] = [
    "LENS_",
    "NETRAY_",
    "PRISM_",
    "TLSIGHT_",
    "SPECTRA_",
    "BEACON_",
    "IFCONFIG_",
];

fn netray(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_netray"));
    cmd.env_clear();
    for (k, v) in std::env::vars() {
        if (k == "PATH" || k == "HOME") && !PREFIXES.iter().any(|p| k.starts_with(p)) {
            cmd.env(k, v);
        }
    }
    cmd.args(args).output().expect("run netray")
}

fn temp_config(tag: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "netray-serve-check-{}-{}.toml",
        std::process::id(),
        tag
    ));
    std::fs::write(&path, contents).unwrap();
    path
}

fn crate_file(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn check(path: &Path) -> (Option<i32>, String, String) {
    let out = netray(&["serve", "--check-config", path.to_str().unwrap()]);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn serve_check_config_accepts_a_valid_file_and_opens_no_named_file() {
    let path = temp_config(
        "valid",
        "[store]\ndb_path = \"/nonexistent-dir/x/history.db\"\n\n\
         [modules.ip]\ngeoip_city_db = \"/nonexistent-dir/city.mmdb\"\n\
         geoip_asn_db = \"/nonexistent-dir/asn.mmdb\"\n",
    );
    let (code, stdout, stderr) = check(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, Some(0), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stdout.trim(), format!("config ok: {}", path.display()));
}

#[test]
fn serve_check_config_rejects_an_invalid_file() {
    let path = temp_config("invalid", "[server\nbind = \n");
    let (code, stdout, stderr) = check(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, Some(1), "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        stderr.contains(&format!("config error: {}: ", path.display())),
        "stderr: {stderr}"
    );
}

#[test]
fn serve_help_mentions_check_config() {
    let out = netray(&["serve", "--help"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "serve --help failed: {text}");
    assert!(text.contains("--check-config"), "help: {text}");
}

#[test]
fn shipped_config_files_check_ok() {
    for name in ["netray.example.toml", "netray.dev.toml"] {
        let path = crate_file(name);
        assert!(path.exists(), "{name} must exist");
        let (code, stdout, stderr) = check(&path);
        assert_eq!(code, Some(0), "{name}: stdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout.trim(), format!("config ok: {}", path.display()));
    }
}

#[test]
fn example_config_holds_every_table_and_key() {
    let text = std::fs::read_to_string(crate_file("netray.example.toml"))
        .expect("netray.example.toml must exist");
    let table_keys: &[(&str, &[&str])] = &[
        (
            "server",
            &["bind", "metrics_bind", "trusted_proxies", "apex"],
        ),
        ("engine", &["resolve_timeout_ms", "profile_path"]),
        ("store", &["enabled", "db_path"]),
        (
            "admission",
            &[
                "client_budget_per_hour",
                "domain_run_slots",
                "per_ip_per_minute",
                "per_ip_burst",
                "global_per_minute",
                "global_burst",
                "cache_enabled",
                "cache_window_seconds",
            ],
        ),
        ("badges", &[]),
        ("og_cards", &[]),
        ("telemetry", &[]),
        ("modules.dns", &["deadline_ms"]),
        ("modules.tls", &["deadline_ms"]),
        ("modules.http", &["deadline_ms"]),
        ("modules.email", &["deadline_ms"]),
        ("modules.ip", &["deadline_ms"]),
    ];
    let mut missing = Vec::new();
    for (table, keys) in table_keys {
        // Keys are looked up inside the table's own section, up to the next header.
        let header = format!("[{table}]");
        let section = text
            .lines()
            .position(|l| l.trim_start_matches('#').trim() == header)
            .map(|i| {
                text.lines()
                    .skip(i + 1)
                    .take_while(|l| {
                        let t = l.trim_start_matches('#').trim();
                        !(t.starts_with('[') && t.ends_with(']'))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        let Some(section) = section else {
            missing.push(header);
            continue;
        };
        for key in *keys {
            let found = section.lines().any(|l| {
                let t = l.trim_start_matches('#').trim_start();
                t.strip_prefix(key)
                    .is_some_and(|rest| rest.trim_start().starts_with('='))
            });
            if !found {
                missing.push(format!("[{table}] {key}"));
            }
        }
    }
    assert!(missing.is_empty(), "missing from example: {missing:?}");
}

#[test]
fn unknown_server_key_is_rejected_and_named() {
    let example = std::fs::read_to_string(crate_file("netray.example.toml"))
        .expect("netray.example.toml must exist");
    let mut out = String::new();
    let mut added = false;
    for line in example.lines() {
        out.push_str(line);
        out.push('\n');
        if !added && line.trim() == "[server]" {
            out.push_str("binds = \"x\"\n");
            added = true;
        }
    }
    assert!(added, "example must hold an uncommented [server] table");
    let path = temp_config("unknown-key", &out);
    let (code, stdout, stderr) = check(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, Some(1), "stdout: {stdout}\nstderr: {stderr}");
    assert!(
        format!("{stdout}{stderr}").contains("binds"),
        "output must name `binds`: {stdout}{stderr}"
    );
}
