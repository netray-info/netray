// `netray serve --check-config` refuses the removed enrichment-backend keys
// (specs/features/v2-cutover, requirements 28 and 11): exit 1, the message names the key
// path, says it was removed in 0.30.0 because enrichment is in-process, and points to the
// CHANGELOG. A module table without those keys stays valid.

use std::path::PathBuf;
use std::process::Command;

fn check_config(name: &str, toml: &str) -> (Option<i32>, String) {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "netray-legacy-enrich-{}-{}.toml",
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
fn serve_refuses_removed_enrichment_keys() {
    // (name, whole config file, key path the message names, expected exit code)
    let rows: &[(&str, &str, &str, i32)] = &[
        (
            "http-enrichment",
            "[modules.http.enrichment]\nip_url = \"http://x\"\n",
            "modules.http.enrichment",
            1,
        ),
        (
            "dns-backends-ip",
            "[modules.dns.backends.ip]\nurl = \"http://x\"\n",
            "modules.dns.backends.ip",
            1,
        ),
        (
            "tls-backends-ip",
            "[modules.tls.backends.ip]\nurl = \"http://x\"\ntimeout_ms = 500\n",
            "modules.tls.backends.ip",
            1,
        ),
        (
            "email-backends-ip-url",
            "[modules.email.backends]\nip_url = \"http://x\"\n",
            "modules.email.backends.ip_url",
            1,
        ),
        ("neighbour-http-alone", "[modules.http]\n", "", 0),
    ];
    let mut failures = Vec::new();
    for (name, toml, key, want) in rows {
        let (code, text) = check_config(name, toml);
        if code != Some(*want) {
            failures.push(format!("{name}: exit {code:?}, want {want}\n{text}"));
            continue;
        }
        if *want == 0 {
            continue;
        }
        for needle in [*key, "removed in 0.30.0", "in-process", "CHANGELOG"] {
            if !text.contains(needle) {
                failures.push(format!("{name}: output lacks {needle:?}\n{text}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}
