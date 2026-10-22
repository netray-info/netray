//! v2-cutover Phase 1: `netray serve <config>` starts the combined server, and refuses at
//! startup what `--check-config` does not open (history store, GeoIP data files).

mod support;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("netray-serve-{tag}-{}", std::process::id()));
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

fn health_status(port: u16) -> Option<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    s.write_all(b"GET /health HTTP/1.1\r\nHost: netray.info\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf.lines().next().map(str::to_owned)
}

#[test]
fn serve_answers_health_with_the_history_store_on() {
    let dir = scratch("health");
    let cfg = dir.join("netray.toml");
    std::fs::write(
        &cfg,
        format!(
            "[store]\nenabled = true\ndb_path = \"{}\"\n",
            dir.join("history.db").display()
        ),
    )
    .unwrap();
    let mut attempt = 0;
    'attempt: loop {
        attempt += 1;
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
        let mut last = None;
        while Instant::now() < deadline {
            if let Some(s) = child.0.try_wait().unwrap() {
                if attempt < support::ATTEMPTS && support::bind_failed(&dir.join("serve.log")) {
                    continue 'attempt;
                }
                panic!(
                    "serve exited early ({s}): {}",
                    std::fs::read_to_string(dir.join("serve.log")).unwrap_or_default()
                );
            }
            if attempt < support::ATTEMPTS && support::bind_failed(&dir.join("serve.log")) {
                continue 'attempt;
            }
            last = health_status(port);
            if support::listening(&dir.join("serve.log"))
                && last.as_deref().is_some_and(|l| l.contains(" 200"))
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("GET /health never answered 200 within 30 s, last: {last:?}");
    }
}

#[test]
fn serve_refuses_an_unopenable_history_store_and_check_config_passes() {
    let dir = scratch("histbad");
    let cfg = dir.join("netray.toml");
    std::fs::write(
        &cfg,
        format!(
            "[store]\nenabled = true\ndb_path = \"/nonexistent-dir-{}/sub/history.db\"\n",
            std::process::id()
        ),
    )
    .unwrap();
    let path = cfg.to_str().unwrap();
    let (code, out) = run(&["serve", path], &dir);
    assert!(code.is_some_and(|c| c != 0), "serve exited {code:?}: {out}");
    assert!(
        out.to_lowercase().contains("history"),
        "output does not name the history store: {out}"
    );
    let (code, out) = run(&["serve", "--check-config", path], &dir);
    assert_eq!(code, Some(0), "--check-config: {out}");
}

#[test]
fn serve_refuses_missing_geoip_files_and_check_config_passes() {
    let dir = scratch("geoip");
    let cfg = dir.join("netray.toml");
    std::fs::write(
        &cfg,
        "[store]\nenabled = false\n\n[modules.ip]\ngeoip_city_db = \"/nonexistent/city.mmdb\"\ngeoip_asn_db = \"/nonexistent/asn.mmdb\"\n",
    )
    .unwrap();
    let path = cfg.to_str().unwrap();
    let (code, out) = run(&["serve", path], &dir);
    assert!(code.is_some_and(|c| c != 0), "serve exited {code:?}: {out}");
    assert!(
        out.contains("/nonexistent/city.mmdb") || out.contains("geoip_city_db"),
        "output does not name the city database: {out}"
    );
    let (code, out) = run(&["serve", "--check-config", path], &dir);
    assert_eq!(code, Some(0), "--check-config: {out}");
}
