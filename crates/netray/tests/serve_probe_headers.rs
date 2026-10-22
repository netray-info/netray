//! v2-cutover Phase 2 review: what the host router answers itself (`/health`, `/ready`, the
//! apex root files, the MTA-STS policy, the 404 of a foreign host) carries lens's response
//! headers as `netray lens` did: security headers, CORS with its preflight, `X-Request-Id`
//! (tests/acceptance/smoke/security-headers.spec.ts, CLAUDE.md layer order). The MTA-STS policy
//! keeps `Cache-Control: no-cache`, as `netray site` served it.

mod support;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
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

struct Resp {
    status: u16,
    headers: String,
    body: Vec<u8>,
}

impl Resp {
    fn content_type(&self) -> String {
        self.headers
            .lines()
            .find_map(|l| l.strip_prefix("content-type:"))
            .unwrap_or("")
            .trim()
            .to_owned()
    }
}

/// Raw request; `host: None` sends an HTTP/1.0 request without a Host header.
fn request(port: u16, host: Option<&str>, path: &str) -> Option<Resp> {
    raw(port, host, "GET", path, "")
}

fn raw(port: u16, host: Option<&str>, method: &str, path: &str, extra: &str) -> Option<Resp> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    let req = match host {
        Some(h) => {
            format!("{method} {path} HTTP/1.1\r\nHost: {h}\r\n{extra}Connection: close\r\n\r\n")
        }
        None => format!("{method} {path} HTTP/1.0\r\n{extra}\r\n"),
    };
    s.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&buf[..split]).to_lowercase();
    let status = head
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(Resp {
        status,
        headers: head,
        body: buf[split + 4..].to_vec(),
    })
}

fn get(port: u16, host: Option<&str>, path: &str) -> Resp {
    request(port, host, path).unwrap_or_else(|| panic!("no response for {host:?} {path}"))
}

struct Server {
    port: u16,
    metrics: u16,
    _child: Killed,
}

fn start(tag: &str, apex: Option<&str>) -> Server {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("netray-probe-headers-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut attempt = 0;
    'attempt: loop {
        attempt += 1;
        let (port, metrics) = (free_port(), free_port());
        let apex_line = apex
            .map(|a| format!("apex = \"{a}\"\n"))
            .unwrap_or_default();
        let cfg = dir.join("netray.toml");
        std::fs::write(
        &cfg,
        format!(
            "[server]\nbind = \"127.0.0.1:{port}\"\nmetrics_bind = \"127.0.0.1:{metrics}\"\n{apex_line}\n[store]\nenabled = true\ndb_path = \"{}\"\n",
            dir.join("history.db").display()
        ),
    )
    .unwrap();
        let log = std::fs::File::create(dir.join("serve.log")).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_netray"));
        cmd.args(["serve", cfg.to_str().unwrap()])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        for key in ["PATH", "HOME"] {
            if let Some(v) = std::env::var_os(key) {
                cmd.env(key, v);
            }
        }
        let mut child = Killed(cmd.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(30);
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
            if support::listening(&dir.join("serve.log"))
                && request(port, Some("netray.info"), "/health").is_some_and(|r| r.status == 200)
            {
                return Server {
                    port,
                    metrics,
                    _child: child,
                };
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("GET /health never answered 200 within 30 s");
    }
}

impl Resp {
    fn header(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}:");
        self.headers
            .lines()
            .find_map(|l| l.strip_prefix(prefix.as_str()))
            .map(|v| v.trim().to_owned())
    }
}

#[test]
fn self_answered_responses_carry_lens_headers() {
    let s = start("headers", None);
    let rows: &[(Option<&str>, &str)] = &[
        (Some("netray.info"), "/health"),
        (Some("netray.info"), "/ready"),
        (Some("lens.netray.info"), "/health"),
        (Some("dns.netray.info"), "/health"),
        (Some("netray.info"), "/favicon.svg"),
        (Some("mta-sts.netray.info"), "/.well-known/mta-sts.txt"),
        (Some("www.example.com"), "/"),
        (None, "/health"),
    ];
    let mut failures = Vec::new();
    for (host, path) in rows {
        let r = get(s.port, *host, path);
        for name in [
            "x-request-id",
            "strict-transport-security",
            "x-content-type-options",
            "content-security-policy",
        ] {
            if r.header(name).is_none_or(|v| v.is_empty()) {
                failures.push(format!("{host:?} {path}: no {name}"));
            }
        }
        if r.header("access-control-allow-origin").as_deref() != Some("*") {
            failures.push(format!(
                "{host:?} {path}: access-control-allow-origin is not *"
            ));
        }
    }
    let pre = raw(
        s.port,
        Some("netray.info"),
        "OPTIONS",
        "/health",
        "Origin: https://example.com\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: content-type\r\n",
    )
    .expect("preflight answered");
    if !(200..300).contains(&pre.status) {
        failures.push(format!("OPTIONS /health preflight answered {}", pre.status));
    }
    if pre.header("access-control-allow-origin").as_deref() != Some("*") {
        failures.push("OPTIONS /health preflight: access-control-allow-origin is not *".into());
    }
    assert!(
        failures.is_empty(),
        "missing headers:\n{}",
        failures.join("\n")
    );
}

#[test]
fn mta_sts_policy_is_not_cached() {
    let s = start("mta-cache", None);
    let r = get(
        s.port,
        Some("mta-sts.netray.info"),
        "/.well-known/mta-sts.txt",
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.header("cache-control").as_deref(), Some("no-cache"));
}
