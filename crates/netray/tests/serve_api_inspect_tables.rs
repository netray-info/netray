//! v2-cutover Phase 1 (requirement 9, C15, C18 second half): `netray.toml` carries the `[api]`
//! and `[inspect.*]` tables, each key under its own `validate()` rule, and the values reach serve.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("netray-api-inspect-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn command(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_netray"));
    cmd.args(args).env_clear().stdin(Stdio::null());
    for key in ["PATH", "HOME"] {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    cmd
}

struct Killed(Child);
impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Runs the binary to completion within 30 s; returns (exit code, stdout + stderr).
fn run(args: &[&str], dir: &Path) -> (Option<i32>, String) {
    let log = dir.join("run.log");
    let file = std::fs::File::create(&log).unwrap();
    let mut child = Killed(
        command(args)
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(s) = child.0.try_wait().unwrap() {
            break s;
        }
        assert!(
            Instant::now() < deadline,
            "{args:?} did not exit within 30 s: {}",
            std::fs::read_to_string(&log).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    (
        status.code(),
        std::fs::read_to_string(&log).unwrap_or_default(),
    )
}

fn config(dir: &Path, extra: &str) -> PathBuf {
    let cfg = dir.join("netray.toml");
    std::fs::write(&cfg, format!("[store]\nenabled = false\n\n{extra}")).unwrap();
    cfg
}

#[test]
fn check_config_accepts_api_and_inspect_tables() {
    let dir = scratch("accept");
    let cfg = config(
        &dir,
        "[api]\nenabled = true\nper_ip_per_minute = 60\nper_ip_burst = 10\n\n\
         [api.ip]\nbatch_max_size = 50\n\n\
         [api.dns]\nper_ip_per_minute = 30\n\n\
         [inspect.tls]\ndeadline_ms = 15000\n\n\
         [inspect.http]\nmax_concurrent = 4\n",
    );
    let path = cfg.to_str().unwrap();
    let (code, out) = run(&["serve", "--check-config", path], &dir);
    assert_eq!(code, Some(0), "output: {out}");
    assert_eq!(out.trim(), format!("config ok: {path}"));
}

#[test]
fn check_config_accepts_inspect_http_max_concurrent() {
    let dir = scratch("inspect-http");
    let cfg = config(&dir, "[inspect.http]\nmax_concurrent = 4\n");
    let (code, out) = run(&["serve", "--check-config", cfg.to_str().unwrap()], &dir);
    assert_eq!(code, Some(0), "output: {out}");
}

#[test]
fn api_ip_batch_max_size_out_of_range_is_refused_naming_the_key() {
    let dir = scratch("batch101");
    let cfg = config(&dir, "[api.ip]\nbatch_max_size = 101\n");
    let path = cfg.to_str().unwrap();
    // The table must parse first: a deny_unknown_fields error names `api`, not the key.
    let (code, out) = run(&["serve", path], &dir);
    assert!(code.is_some_and(|c| c != 0), "serve exited {code:?}: {out}");
    assert!(
        out.contains("batch_max_size"),
        "output lacks the key: {out}"
    );
    // validate() runs at load, so --check-config refuses it too.
    let (code, out) = run(&["serve", "--check-config", path], &dir);
    assert_eq!(code, Some(1), "--check-config: {out}");
    assert!(
        out.contains("batch_max_size"),
        "output lacks the key: {out}"
    );
}

fn get_status(port: u16, path: &str) -> Option<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: netray.info\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf.lines().next().map(str::to_owned)
}

/// Starts serve with `extra` appended to the config, waits for /health, returns GET `path`'s status line.
fn serve_and_get(tag: &str, extra: &str, path: &str) -> String {
    let dir = scratch(tag);
    let cfg = config(&dir, extra);
    let (port, metrics) = (free_port(), free_port());
    let log = std::fs::File::create(dir.join("serve.log")).unwrap();
    let mut child = Killed(
        command(&["serve", cfg.to_str().unwrap()])
            .env("NETRAY_SERVER__BIND", format!("127.0.0.1:{port}"))
            .env(
                "NETRAY_SERVER__METRICS_BIND",
                format!("127.0.0.1:{metrics}"),
            )
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some(s) = child.0.try_wait().unwrap() {
            panic!(
                "serve exited early ({s}): {}",
                std::fs::read_to_string(dir.join("serve.log")).unwrap_or_default()
            );
        }
        if get_status(port, "/health").is_some_and(|l| l.contains(" 200")) {
            return get_status(port, path).expect("status line");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("serve never answered /health within 30 s");
}

#[test]
fn api_enabled_false_reaches_serve() {
    let on = serve_and_get("api-on", "", "/api/v1/meta");
    assert!(on.contains(" 200"), "default config: {on}");
    let off = serve_and_get("api-off", "[api]\nenabled = false\n", "/api/v1/meta");
    assert!(off.contains(" 404"), "[api] enabled = false: {off}");
}
