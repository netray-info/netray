// v2-cutover branch review (f268717d..90a63b3): a legacy refusal names a destination that loads,
// and a module table the binary does not know is refused even when it holds only its section
// window (requirements 9 and 11).

use std::process::Command;

fn check(tag: &str, contents: &str) -> (Option<i32>, String) {
    let path =
        std::env::temp_dir().join(format!("netray-review-{}-{tag}.toml", std::process::id()));
    std::fs::write(&path, contents).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_netray"));
    cmd.env_clear();
    if let Ok(v) = std::env::var("PATH") {
        cmd.env("PATH", v);
    }
    let out = cmd
        .args(["serve", "--check-config", path.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
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
fn spectras_enrichment_table_is_removed_not_moved() {
    let (code, text) = check("enrichment", "[enrichment]\nip_url = \"http://x\"\n");
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("removed in 0.30.0"), "{text}");
    assert!(!text.contains("[modules.http].enrichment"), "{text}");
}

#[test]
fn ifconfigs_exempt_cidrs_is_removed_not_moved() {
    let (code, text) = check(
        "exempt",
        "[rate_limit]\nexempt_cidrs = [\"192.0.2.0/24\"]\n",
    );
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("removed in 0.30.0"), "{text}");
    assert!(!text.contains("[api.ip] exempt_cidrs"), "{text}");
}

#[test]
fn an_unknown_module_holding_only_its_window_is_refused() {
    let (code, text) = check("emial", "[modules.emial]\ndeadline_ms = 8000\n");
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("modules.emial"), "{text}");
}
