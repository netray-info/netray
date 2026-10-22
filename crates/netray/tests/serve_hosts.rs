//! v2-cutover Phase 2: `netray serve` routes by Host. The apex serves the pages, `mta-sts.<apex>`
//! only the policy, every other name (and no Host) a 404 on `/`; `/health` and `/ready` answer
//! for any Host; `/metrics` lives on the metrics listener only.

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
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    let req = match host {
        Some(h) => format!("GET {path} HTTP/1.1\r\nHost: {h}\r\nConnection: close\r\n\r\n"),
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

fn site(rel: &str) -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../web/tests/fixtures/v1-site")
        .join(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn health_and_ready_answer_for_every_host() {
    let srv = start("probes", None);
    let loopback = format!("127.0.0.1:{}", srv.port);
    let hosts: Vec<Option<&str>> = vec![
        Some("netray.info"),
        Some("dns.netray.info"),
        Some("tls.netray.info"),
        Some("http.netray.info"),
        Some("ip.netray.info"),
        Some("email.netray.info"),
        Some("lens.netray.info"),
        Some("mta-sts.netray.info"),
        Some("www.example.com"),
        Some(&loopback),
        None,
    ];
    for host in hosts {
        for path in ["/health", "/ready"] {
            let r = get(srv.port, host, path);
            assert_eq!(r.status, 200, "{host:?} {path}");
        }
    }
}

#[test]
fn apex_serves_pages_and_assets() {
    let srv = start("apex", None);
    let p = srv.port;

    let root = get(p, Some("netray.info"), "/");
    assert_eq!(root.status, 200, "apex /");
    assert!(
        root.content_type().starts_with("text/html"),
        "{}",
        root.content_type()
    );
    let shouty = get(p, Some("NETRAY.INFO:443"), "/");
    assert_eq!(shouty.status, 200, "NETRAY.INFO:443 /");
    assert_eq!(
        shouty.body, root.body,
        "host match is case- and port-insensitive"
    );

    let fav = get(p, Some("netray.info"), "/favicon.svg");
    assert_eq!(fav.status, 200);
    assert!(
        fav.content_type().starts_with("image/svg+xml"),
        "{}",
        fav.content_type()
    );
    let static_fav = get(p, Some("netray.info"), "/static/favicon.svg");
    assert_eq!(static_fav.status, 200);
    assert_eq!(fav.body, static_fav.body);

    let bimi = get(p, Some("netray.info"), "/bimi-logo.svg");
    assert_eq!(bimi.status, 200);
    assert_eq!(bimi.body, site("bimi-logo.svg"));

    for path in [
        "/favicon.ico",
        "/.well-known/mta-sts.txt",
        "/.well-known/security.txt",
    ] {
        assert_eq!(get(p, Some("netray.info"), path).status, 404, "apex {path}");
    }
}

#[test]
fn mta_sts_host_serves_only_the_policy() {
    let srv = start("mtasts", None);
    let h = Some("mta-sts.netray.info");
    let r = get(srv.port, h, "/.well-known/mta-sts.txt");
    assert_eq!(r.status, 200);
    let body = String::from_utf8(r.body.clone()).unwrap();
    assert!(body.starts_with("version: STSv1"), "{body}");
    assert!(body.contains("mode:"), "{body}");
    assert_eq!(r.body, site(".well-known/mta-sts.txt"));
    assert_eq!(get(srv.port, h, "/").status, 404);
    assert_eq!(get(srv.port, h, "/favicon.svg").status, 404);
}

#[test]
fn foreign_and_missing_hosts_get_404_on_root() {
    let srv = start("foreign", None);
    for host in [
        Some("www.example.com"),
        Some("www.netray.info"),
        Some("example.com"),
        None,
    ] {
        assert_eq!(get(srv.port, host, "/").status, 404, "{host:?} /");
    }
    assert_eq!(
        get(srv.port, Some("www.example.com"), "/guide/").status,
        404,
        "no apex content under a foreign name"
    );
}

#[test]
fn inspector_hosts_never_serve_apex_pages() {
    let srv = start("inspector", None);
    // An apex path moves to the apex; the inspector host never serves its page.
    let r = get(srv.port, Some("dns.netray.info"), "/guide/");
    assert_eq!(r.status, 301);
    assert!(
        r.headers
            .lines()
            .any(|l| l == "location: https://netray.info/guide/"),
        "{}",
        r.headers
    );
    assert!(!String::from_utf8_lossy(&r.body).contains("<html"));
}

#[test]
fn configured_apex_replaces_the_default() {
    let srv = start("localhost", Some("localhost"));
    assert_eq!(get(srv.port, Some("localhost"), "/").status, 200);
    assert_eq!(get(srv.port, Some("netray.info"), "/").status, 404);
}

#[test]
fn metrics_only_on_the_metrics_listener() {
    let srv = start("metrics", None);
    assert_eq!(get(srv.port, Some("netray.info"), "/metrics").status, 404);
    let m = get(srv.metrics, Some("netray.info"), "/metrics");
    assert_eq!(m.status, 200);
    assert!(
        m.content_type().starts_with("text/plain"),
        "{}",
        m.content_type()
    );
    assert!(
        String::from_utf8_lossy(&m.body).contains("# TYPE"),
        "no # TYPE line"
    );
}
