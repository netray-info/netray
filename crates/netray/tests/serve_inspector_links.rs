//! The inspector hosts' pages are byte-identical to the apex's `/inspect/<tool>`, so their links
//! are host-relative. A GET or HEAD on `dns.`, `tls.`, `http.` or `ip.` for any path but `/` and
//! `/static/*` answers 301 to the same path and query on the apex; other methods stay on the
//! host's inspector router. Helpers copied from `serve_inspector_hosts.rs`.

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
}

impl Resp {
    fn location(&self) -> Option<String> {
        self.headers
            .lines()
            .find_map(|l| l.strip_prefix("location:"))
            .map(|v| v.trim().to_owned())
    }
}

const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101 Firefox/130.0";

fn request(port: u16, method: &str, host: &str, path: &str) -> Option<Resp> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {BROWSER}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    s.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    // Header names lowercased; values keep their case.
    let head: String = String::from_utf8_lossy(&buf[..split])
        .lines()
        .map(|l| match l.split_once(':') {
            Some((k, v)) => format!("{}:{v}\n", k.to_ascii_lowercase()),
            None => format!("{l}\n"),
        })
        .collect();
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
    })
}

fn send(port: u16, method: &str, host: &str, path: &str) -> Resp {
    request(port, method, host, path)
        .unwrap_or_else(|| panic!("no response for {method} {host}{path}"))
}

struct Server {
    port: u16,
    _child: Killed,
}

fn start(tag: &str) -> Server {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("netray-links-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut attempt = 0;
    'attempt: loop {
        attempt += 1;
        let (port, metrics) = (free_port(), free_port());
        let cfg = dir.join("netray.toml");
        std::fs::write(
        &cfg,
        format!(
            "[server]\nbind = \"127.0.0.1:{port}\"\nmetrics_bind = \"127.0.0.1:{metrics}\"\n\n[store]\nenabled = true\ndb_path = \"{}\"\n",
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
                && request(port, "GET", "netray.info", "/health").is_some_and(|r| r.status == 200)
            {
                return Server {
                    port,
                    _child: child,
                };
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("GET /health never answered 200 within 30 s");
    }
}

#[test]
fn inspector_host_links_move_to_the_apex() {
    let srv = start("moved");
    for path in ["/example.com", "/guide/"] {
        let r = send(srv.port, "GET", "dns.netray.info", path);
        assert_eq!(r.status, 301, "GET dns.netray.info{path}");
        assert_eq!(
            r.location().as_deref(),
            Some(format!("https://netray.info{path}").as_str()),
            "GET dns.netray.info{path}"
        );
    }
    let post = send(srv.port, "POST", "dns.netray.info", "/nope");
    assert!(
        !(300..400).contains(&post.status),
        "POST dns.netray.info/nope redirected: {}",
        post.status
    );
}
