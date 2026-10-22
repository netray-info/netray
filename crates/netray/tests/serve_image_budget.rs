// `[admission] image_runs_per_minute` (specs/features/v2-release-cleanup, Phase 1, requirement 4):
// the badge and OG image-run budget loads from the file and `NETRAY_ADMISSION__IMAGE_RUNS_PER_MINUTE`,
// refuses zero, and the example shows its default.

use std::path::PathBuf;
use std::process::Command;

struct Outcome {
    code: Option<i32>,
    text: String,
}

fn write_file(tag: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "netray-image-budget-{}-{tag}.toml",
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
fn image_runs_per_minute_loads_and_refuses_zero() {
    accepted("file", "[admission]\nimage_runs_per_minute = 5\n", &[]);
    accepted(
        "env",
        "",
        &[("NETRAY_ADMISSION__IMAGE_RUNS_PER_MINUTE", "5")],
    );
    refused(
        "zero",
        "[admission]\nimage_runs_per_minute = 0\n",
        &[],
        "admission.image_runs_per_minute must not be zero",
    );
    refused(
        "env-zero",
        "",
        &[("NETRAY_ADMISSION__IMAGE_RUNS_PER_MINUTE", "0")],
        "admission.image_runs_per_minute must not be zero",
    );
}

#[test]
fn the_example_shows_the_default_of_20() {
    let example =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/netray.example.toml"))
            .unwrap();
    let admission = example
        .split("\n[")
        .find(|t| t.starts_with("admission]"))
        .expect("[admission] table");
    assert!(
        admission.contains("# image_runs_per_minute = 20"),
        "[admission] must show image_runs_per_minute at its default 20:\n{admission}"
    );
}

#[test]
fn the_web_default_is_20() {
    assert_eq!(
        netray_web::config::ApexConfig::default()
            .limits
            .image_runs_per_minute,
        20
    );
}
