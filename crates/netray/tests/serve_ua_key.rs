// C6: a top-level `user_agent_regexes` is refused with its destination, `[modules.ip].user_agent_regexes`.

use std::process::Command;

#[test]
fn legacy_user_agent_regexes_names_the_module_key() {
    let path = std::env::temp_dir().join(format!("netray-ua-key-{}.toml", std::process::id()));
    std::fs::write(&path, "user_agent_regexes = \"x\"\n").unwrap();
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
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("[modules.ip].user_agent_regexes"), "{text}");
}
