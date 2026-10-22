// `netray serve --check-config` (specs/features/v2-cutover, Phase 1, requirements 1 and 9):
// one config for the whole binary, keys overridable as `NETRAY_<SECTION>__<KEY>`, the validate
// rules of the old lens config kept, `NETRAY_CONFIG` never read as a key.

use std::path::PathBuf;
use std::process::Command;

struct Outcome {
    code: Option<i32>,
    text: String,
}

fn write_file(tag: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "netray-serve-config-{}-{tag}.toml",
        std::process::id()
    ));
    std::fs::write(&path, contents).unwrap();
    path
}

fn check(tag: &str, contents: &str, env: &[(&str, &str)]) -> Outcome {
    let path = write_file(tag, contents);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_netray"));
    cmd.env_clear();
    for key in ["PATH", "HOME"] {
        if let Ok(v) = std::env::var(key) {
            cmd.env(key, v);
        }
    }
    for (k, v) in env {
        let v = v.replace("<FILE>", path.to_str().unwrap());
        cmd.env(k, v);
    }
    let out = cmd
        .args(["serve", "--check-config", path.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Outcome {
        code: out.status.code(),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

fn refused(tag: &str, contents: &str, env: &[(&str, &str)], needle: &str) {
    let o = check(tag, contents, env);
    assert_eq!(o.code, Some(1), "{tag}: must exit 1, output: {}", o.text);
    assert!(
        o.text.contains(needle),
        "{tag}: output must contain {needle:?}, got: {}",
        o.text
    );
}

fn accepted(tag: &str, contents: &str, env: &[(&str, &str)]) {
    let o = check(tag, contents, env);
    assert_eq!(o.code, Some(0), "{tag}: must exit 0, output: {}", o.text);
    assert!(
        o.text.contains("config ok:"),
        "{tag}: must print `config ok:`, got: {}",
        o.text
    );
}

#[test]
fn serve_check_config_env_overrides_and_validation() {
    // C9: the IP module needs both databases; the env var supplies the missing one.
    refused(
        "c9-alone",
        "[modules.ip]\ngeoip_asn_db = \"/y\"\n",
        &[],
        "geoip_city_db",
    );
    accepted(
        "c9-env",
        "[modules.ip]\ngeoip_asn_db = \"/y\"\n",
        &[("NETRAY_MODULES__IP__GEOIP_CITY_DB", "/x")],
    );

    // C2: validate rules kept.
    refused(
        "c2-slots",
        "",
        &[("NETRAY_ADMISSION__DOMAIN_RUN_SLOTS", "0")],
        "admission.domain_run_slots",
    );
    refused(
        "c2-burst",
        "",
        &[("NETRAY_ADMISSION__PER_IP_BURST", "0")],
        "admission.per_ip_burst",
    );
    refused(
        "c2-badge",
        "[badges]\nttl_seconds = 0\n",
        &[],
        "badges.ttl_seconds",
    );
    refused(
        "c2-resolve",
        "[engine]\nresolve_timeout_ms = 5000\n",
        &[],
        "resolve_timeout_ms",
    );
    refused(
        "c2-sum",
        "[modules.tls]\ndeadline_ms = 20000\n",
        &[],
        "deadline",
    );
    // A module table holding only `deadline_ms` sets the section window and does not switch
    // the module on: `[modules.ip]` without data paths stays off, as lens's `[backends.ip]` did.
    accepted(
        "c2-window-only",
        "[engine]\nresolve_timeout_ms = 2500\n[modules.ip]\ndeadline_ms = 3000\n[modules.tls]\ndeadline_ms = 15000\n[modules.http]\ndeadline_ms = 15000\n",
        &[],
    );
    refused("c2-unknown", "[store]\nbogus = 1\n", &[], "bogus");
    accepted("c2-apex", "[server]\napex = \"localhost\"\n", &[]);

    // C1: NETRAY_CONFIG names the file and is never read as a key.
    accepted("c1-config", "", &[("NETRAY_CONFIG", "<FILE>")]);
}
