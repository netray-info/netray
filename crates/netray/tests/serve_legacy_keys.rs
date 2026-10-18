// `netray serve --check-config` refuses a legacy key path before deserialization
// (specs/features/v2-cutover, requirement 11, Phase 1): exit 1, the message names the key
// path, where its value lives now (or "removed in 0.30.0") and points to the CHANGELOG.

use std::path::PathBuf;
use std::process::Command;

fn check_config(name: &str, toml: &str) -> (Option<i32>, String) {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "netray-legacy-{}-{}.toml",
        std::process::id(),
        name
    ));
    std::fs::write(&path, toml).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_netray"))
        .args(["serve", "--check-config"])
        .arg(&path)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .output()
        .expect("spawn netray");
    let _ = std::fs::remove_file(&path);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

#[test]
fn serve_refuses_legacy_keys_with_their_destination() {
    let rows: &[(&str, &str, &[&str])] = &[
        (
            "c4",
            "[snapshots]\nenabled = true\n",
            &["snapshots", "removed in 0.30.0"],
        ),
        (
            "bare-scoring",
            "[scoring]\n# Uses embedded default profile\n",
            &["scoring", "[engine] profile_path"],
        ),
        ("bare-history", "[history]\n", &["history", "[store]"]),
        (
            "bare-cache",
            "[cache]\n",
            &["cache", "[admission] cache_enabled"],
        ),
        (
            "bare-rate-limit",
            "[rate_limit]\n",
            &["rate_limit", "[admission]"],
        ),
        (
            "bare-backends",
            "[backends.http]\n[backends.email]\n",
            &["backends"],
        ),
        (
            "c12",
            "[history]\ndb_path = \"h.db\"\n",
            &["[store] db_path"],
        ),
        (
            "c13",
            "[cache]\nttl_seconds = 300\n",
            &["[admission] cache_window_seconds"],
        ),
        (
            "c14",
            "[rate_limit]\nper_ip_burst = 3\n",
            &["[admission] per_ip_burst"],
        ),
        (
            "c16",
            "geoip_city_db = \"x\"\n",
            &["[modules.ip].geoip_city_db"],
        ),
        ("c17", "[dns]\nservers = [\"google\"]\n", &["dns"]),
        (
            "c18a",
            "[inspect]\nmax_redirects = 5\n",
            &["[modules.http.inspect].max_redirects"],
        ),
        (
            "c19",
            "[server]\nadmin_bind = \"127.0.0.1:9090\"\n",
            &["[server] metrics_bind"],
        ),
        (
            "c20",
            "[server]\nadmin_token = \"t\"\n",
            &["admin_token", "removed in 0.30.0"],
        ),
        (
            "c21a",
            "[cache]\nttl_secs = 60\n",
            &["removed in 0.30.0", "ifconfig"],
        ),
        (
            "c21b",
            "[cache]\nenabled = true\n",
            &["[admission] cache_enabled", "ifconfig", "removed"],
        ),
        (
            "c21c",
            "[rate_limit]\nper_target_per_minute = 120\n",
            &["[api.ip]"],
        ),
        (
            "c22a",
            "user_agent_regexes = \"x\"\n",
            &["user_agent_regexes"],
        ),
        (
            "c22b",
            "[backends]\nresolve_timeout_ms = 1000\n",
            &["[engine] resolve_timeout_ms"],
        ),
        (
            "c22c",
            "[backends.tls]\ntimeout_ms = 5000\n",
            &["[modules.tls] deadline_ms"],
        ),
        (
            "c22d",
            "[scoring]\nprofile_path = \"p.toml\"\n",
            &["[engine] profile_path"],
        ),
        (
            "c22e",
            "[site]\ntitle = \"x\"\n",
            &["site", "removed in 0.30.0"],
        ),
    ];
    let mut failures = Vec::new();
    for (name, toml, needles) in rows {
        let (code, text) = check_config(name, toml);
        if code != Some(1) {
            failures.push(format!("{name}: exit {code:?}, want 1\n{text}"));
            continue;
        }
        for needle in needles.iter().chain(&["CHANGELOG"]) {
            if !text.contains(needle) {
                failures.push(format!("{name}: output lacks {needle:?}\n{text}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[test]
fn serve_refuses_a_legacy_key_before_deserialization() {
    let (code, text) = check_config("order", "bogus_key = 1\n[history]\ndb_path = \"h.db\"\n");
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("[store] db_path"),
        "the legacy refusal must win over the unknown-key error, got: {text}"
    );
}
