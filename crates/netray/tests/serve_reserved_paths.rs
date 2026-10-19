//! v2-cutover Phase 2, C10: every reserved segment and root file of SDD §5.1 never reaches
//! the result page. The result page is what the apex answers for `GET /example.com`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct Killed(Child);
impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Raw HTTP/1.1 GET; returns (status, body), or None when the connection fails.
fn get(port: u16, path: &str) -> Option<(u16, String)> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    s.write_all(
        format!("GET {path} HTTP/1.1\r\nHost: netray.info\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .ok()?;
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    let body = text
        .split_once("\r\n\r\n")
        .map(|x| x.1)
        .unwrap_or("")
        .to_owned();
    Some((status, body))
}

#[test]
fn reserved_segments_and_root_files_never_reach_the_result_page() {
    let dir = std::env::temp_dir().join(format!("netray-reserved-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("netray.toml");
    std::fs::write(
        &cfg,
        format!(
            "[store]\ndb_path = \"{}\"\n",
            dir.join("history.db").display()
        ),
    )
    .unwrap();
    let (port, metrics) = (free_port(), free_port());
    let log = std::fs::File::create(dir.join("serve.log")).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_netray"));
    cmd.args(["serve", cfg.to_str().unwrap()])
        .env_clear()
        .stdin(Stdio::null())
        .env("NETRAY_SERVER__BIND", format!("127.0.0.1:{port}"))
        .env(
            "NETRAY_SERVER__METRICS_BIND",
            format!("127.0.0.1:{metrics}"),
        )
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    for key in ["PATH", "HOME"] {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    let mut child = Killed(cmd.spawn().unwrap());

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(s) = child.0.try_wait().unwrap() {
            panic!(
                "serve exited early ({s}): {}",
                std::fs::read_to_string(dir.join("serve.log")).unwrap_or_default()
            );
        }
        if get(port, "/health").is_some_and(|(st, _)| st == 200) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "GET /health never answered 200 within 30 s"
        );
        std::thread::sleep(Duration::from_millis(200));
    }

    let (ref_status, ref_body) = get(port, "/example.com").expect("GET /example.com");
    assert_eq!(ref_status, 200, "reference /example.com status");
    // The result page is server-rendered since v2-result-page; its passed-checks block is its own.
    let marker = "<details id=\"passed\"";
    assert!(
        ref_body.contains(marker),
        "reference body lacks the result page marker {marker:?}: {ref_body}"
    );

    let paths = [
        "/r",
        "/r/",
        "/api",
        "/api/",
        "/guide",
        "/guide/",
        "/status",
        "/compare",
        "/inspect",
        "/inspect/",
        "/privacy",
        "/static/",
        "/.well-known/",
        "/.well-known/x",
        "/robots.txt",
        "/sitemap.xml",
        "/favicon.svg",
        "/favicon.ico",
    ];
    let mut failures = Vec::new();
    for path in paths {
        let (status, body) = get(port, path).unwrap_or((0, String::new()));
        let mut why = Vec::new();
        if body == ref_body {
            why.push("body equals the result page".to_owned());
        }
        if body.contains(marker) {
            why.push(format!("body contains the result page marker {marker:?}"));
        }
        if (path == "/favicon.ico" || path.starts_with("/.well-known/")) && status != 404 {
            why.push(format!("status {status}, want 404"));
        }
        if !why.is_empty() {
            failures.push(format!("{path} (status {status}): {}", why.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "reserved paths reaching the result page:\n{}",
        failures.join("\n")
    );
}
