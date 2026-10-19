//! v2-cutover requirement 2 (C8, C9): `dns.`, `tls.`, `http.` and `ip.` serve their inspector at
//! `/` through the apex's `/inspect/<tool>` handler; V1 query strings keep working, `r` is
//! ignored, and `ip.` answers a curl user agent with the caller's address as text.

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
    request_ua(port, host, path, BROWSER)
}

const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101 Firefox/130.0";

fn request_ua(port: u16, host: Option<&str>, path: &str, ua: &str) -> Option<Resp> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    let req = match host {
        Some(h) => format!("GET {path} HTTP/1.1\r\nHost: {h}\r\nUser-Agent: {ua}\r\nConnection: close\r\n\r\n"),
        None => format!("GET {path} HTTP/1.0\r\n\r\n"),
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

fn get_ua(port: u16, host: &str, path: &str, ua: &str) -> Resp {
    request_ua(port, Some(host), path, ua).unwrap_or_else(|| panic!("no response for {host} {path}"))
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
        std::env::temp_dir().join(format!("netray-hosts-{tag}-{}", std::process::id()));
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

const TOOLS: [(&str, &str); 4] = [("dns", "q"), ("tls", "h"), ("http", "url"), ("ip", "ip")];

fn text(r: &Resp) -> String {
    String::from_utf8_lossy(&r.body).into_owned()
}

fn marker(tool: &str) -> String {
    format!(r#"data-stream-url="/inspect/{tool}/run""#)
}

#[test]
fn c8_inspector_hosts_serve_their_inspector_at_root() {
    let srv = start("c8", None);
    for (tool, field) in TOOLS {
        let host = format!("{tool}.netray.info");
        let r = get(srv.port, Some(&host), "/");
        assert_eq!(r.status, 200, "{host} /");
        assert!(r.content_type().starts_with("text/html"), "{host}: {}", r.content_type());
        let body = text(&r);
        assert!(body.contains(&marker(tool)), "{host}: not the {tool} inspector");
        assert!(body.contains(&format!(r#"name="{field}""#)), "{host}: no input {field}");
        assert!(!body.contains("data-stream-autostart"), "{host}: autostart without a query");
    }

    let apex = get(srv.port, Some("netray.info"), "/inspect/dns?q=example.com");
    assert_eq!(apex.status, 200);
    let via_host = get(srv.port, Some("dns.netray.info"), "/?q=example.com");
    assert_eq!(via_host.status, 200);
    assert_eq!(text(&via_host), text(&apex), "same handler, same body");
}

#[test]
fn c8_configured_apex_moves_the_inspector_hosts() {
    let srv = start("c8-localhost", Some("localhost"));
    let r = get(srv.port, Some("dns.localhost"), "/");
    assert_eq!(r.status, 200);
    assert!(text(&r).contains(&marker("dns")));
    assert_eq!(get(srv.port, Some("dns.netray.info"), "/").status, 404);
}

#[test]
fn c9_v1_query_strings_prefill_the_inspector() {
    let srv = start("c9", None);
    let rows = [
        ("dns", "/?q=example.com", "example.com"),
        ("tls", "/?h=example.com", "example.com"),
        ("http", "/?url=https://example.com/", "https://example.com/"),
        ("ip", "/?ip=192.0.2.1", "192.0.2.1"),
    ];
    for (tool, path, value) in rows {
        let host = format!("{tool}.netray.info");
        let r = get(srv.port, Some(&host), path);
        assert_eq!(r.status, 200, "{host}{path}");
        assert!(!r.headers.contains("\nlocation:"), "{host}{path}: redirect");
        let body = text(&r);
        assert!(body.contains(&marker(tool)), "{host}{path}");
        assert!(body.contains(&format!(r#"value="{value}""#)), "{host}{path}: not prefilled");
    }

    let plain = get(srv.port, Some("dns.netray.info"), "/?q=example.com");
    let with_r = get(srv.port, Some("dns.netray.info"), "/?q=example.com&r=k1");
    assert_eq!(with_r.status, 200);
    assert_eq!(text(&with_r), text(&plain), "r is ignored");
}

#[test]
fn c9_ip_root_answers_curl_with_the_caller_and_browsers_with_the_page() {
    let srv = start("c9-ip", None);
    for ua in ["curl/8.0", "Wget/1.21", "HTTPie/3.2.1"] {
        let r = get_ua(srv.port, "ip.netray.info", "/", ua);
        assert_eq!(r.status, 200, "{ua}");
        assert!(r.content_type().starts_with("text/plain"), "{ua}: {}", r.content_type());
        assert_eq!(text(&r).trim(), "127.0.0.1", "{ua}");
    }
    let r = get_ua(srv.port, "ip.netray.info", "/", BROWSER);
    assert_eq!(r.status, 200);
    assert!(r.content_type().starts_with("text/html"), "{}", r.content_type());
    assert!(text(&r).contains(&marker("ip")));
    let _ = srv.metrics;
}
