//! v2-cutover Phase 4 (requirement 18; SDD 9.2): `/api/v1` routes count `netray_api_requests_total`
//! by route template, and no exposition label carries a domain, address or other request value.

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
    send(port, host, "GET", path, "")
}

fn send(port: u16, host: Option<&str>, method: &str, path: &str, body: &str) -> Option<Resp> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    let req = match host {
        Some(h) => format!(
            "{method} {path} HTTP/1.1\r\nHost: {h}\r\nConnection: close\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
        None => format!("{method} {path} HTTP/1.0\r\n\r\n"),
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
        std::env::temp_dir().join(format!("netray-metrics-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
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
            panic!(
                "serve exited early ({s}): {}",
                std::fs::read_to_string(dir.join("serve.log")).unwrap_or_default()
            );
        }
        if request(port, Some("netray.info"), "/health").is_some_and(|r| r.status == 200) {
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

fn metrics_text(srv: &Server) -> String {
    let m = get(srv.metrics, Some("netray.info"), "/metrics");
    assert_eq!(m.status, 200);
    String::from_utf8_lossy(&m.body).into_owned()
}

/// Sample lines as (name, [(label, value)], value text).
fn samples(text: &str) -> Vec<(String, Vec<(String, String)>, String)> {
    let mut out = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (series, value) = line.rsplit_once(' ').unwrap_or((line, ""));
        let (name, labels) = match series.split_once('{') {
            Some((n, rest)) => (n, rest.trim_end_matches('}')),
            None => (series, ""),
        };
        let pairs = labels
            .split("\",")
            .filter_map(|p| p.split_once("=\""))
            .map(|(k, v)| {
                (
                    k.trim_start_matches(',').trim().to_owned(),
                    v.trim_end_matches('"').to_owned(),
                )
            })
            .collect();
        out.push((name.to_owned(), pairs, value.to_owned()));
    }
    out
}

const APEX: Option<&str> = Some("netray.info");

#[test]
fn api_v1_domain_route_is_counted_by_its_template() {
    let srv = start("api-count", None);
    let r = get(srv.port, APEX, "/api/v1/domains/example.com");
    assert!(r.status == 404 || r.status == 200, "status {}", r.status);

    let text = metrics_text(&srv);
    let counted: f64 = samples(&text)
        .iter()
        .filter(|(n, l, _)| {
            n == "netray_api_requests_total"
                && l.iter()
                    .any(|(k, v)| k == "endpoint" && v == "/api/v1/domains/{domain}")
        })
        .filter_map(|(_, _, v)| v.parse::<f64>().ok())
        .sum();
    assert!(
        counted >= 1.0,
        "no netray_api_requests_total{{endpoint=\"/api/v1/domains/{{domain}}\"}} >= 1\n--- exposition ---\n{text}"
    );
}

#[test]
fn exposition_labels_never_carry_request_values() {
    let srv = start("api-labels", None);
    let _ = get(srv.port, APEX, "/api/v1/domains/example.com");
    let _ = get(srv.port, APEX, "/api/v1/ip/192.0.2.1");
    let _ = send(srv.port, APEX, "POST", "/inspect/ip/run", "ip=192.0.2.1");

    let text = metrics_text(&srv);
    let banned = [
        "domain", "host", "address", "ip", "client", "target", "url", "query", "name", "id",
    ];
    let mut bad = Vec::new();
    for (name, labels, _) in samples(&text) {
        for (k, v) in &labels {
            if banned.contains(&k.as_str()) {
                bad.push(format!("{name}: label name `{k}`"));
            }
            if v == "example.com" || v == "192.0.2.1" {
                bad.push(format!("{name}: label {k}=\"{v}\""));
            }
            if k == "endpoint"
                && (!v.starts_with('/') || v.contains("example.com") || v.contains("192.0.2.1"))
            {
                bad.push(format!("{name}: endpoint=\"{v}\" is not a route template"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "request values in labels:\n  {}\n--- exposition ---\n{text}",
        bad.join("\n  ")
    );
}
