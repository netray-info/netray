//! v2-cutover Phase 3 (requirements 13 row j, 23): `/r/<id>` answers from the history store only.
//! A stored entry is 200; a well-formed ID that is no entry is 410 (page without a domain, empty
//! body for the card); a malformed ID is 404. No V1 snapshot step exists in config or code.

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
    body: Vec<u8>,
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
        body: buf[split + 4..].to_vec(),
    })
}

fn get(port: u16, host: Option<&str>, path: &str) -> Resp {
    request(port, host, path).unwrap_or_else(|| panic!("no response for {host:?} {path}"))
}

struct Server {
    port: u16,
    _child: Killed,
}

fn start(tag: &str, db: &Path) -> Server {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("netray-gone-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (port, metrics) = (free_port(), free_port());
    let cfg = dir.join("netray.toml");
    std::fs::write(
        &cfg,
        format!(
            "[server]\nbind = \"127.0.0.1:{port}\"\nmetrics_bind = \"127.0.0.1:{metrics}\"\n\n[store]\nenabled = true\ndb_path = \"{}\"\n",
            db.display()
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
                _child: child,
            };
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("GET /health never answered 200 within 30 s");
}

const DOMAIN: &str = "seeded-entry.example.org";

/// Records one complete run for `DOMAIN` into a fresh store at `db` and returns the entry id.
fn seed(db: &Path) -> String {
    use netray_model::diff::diff;
    use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
    use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
    use netray_model::{CheckId, Grade, Protocol, Status};
    use netray_store::{HistoryStore, Recorded};

    let run = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
        }],
        blocks: vec![],
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let store = HistoryStore::open(db).await.unwrap();
        store.migrate().await.unwrap();
        let rec = store
            .record(DOMAIN, &run, &[], 1000, |p, c| diff(p, c, &[], None))
            .await
            .unwrap();
        let Recorded::First { id } = rec else {
            panic!("expected First, got {rec:?}");
        };
        id
    })
}

#[test]
fn entry_ids_answer_from_the_store_only() {
    let dir = std::env::temp_dir().join(format!("netray-gone-seed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("history.db");
    let _ = std::fs::remove_file(&db);
    let id = seed(&db);
    let srv = start("entries", &db);
    let h = Some("netray.info");

    assert_eq!(
        get(srv.port, h, &format!("/r/{id}")).status,
        200,
        "stored entry"
    );

    let gone = get(srv.port, h, "/r/abcd1234");
    assert_eq!(gone.status, 410, "/r/abcd1234");
    let body = String::from_utf8_lossy(&gone.body).to_lowercase();
    assert!(!body.contains(DOMAIN), "{body}");

    let gone_tls = get(srv.port, h, "/r/abcd1234/tls");
    assert_eq!(gone_tls.status, 410, "/r/abcd1234/tls");
    let body = String::from_utf8_lossy(&gone_tls.body).to_lowercase();
    assert!(!body.contains(DOMAIN), "{body}");

    let card = get(srv.port, h, "/r/abcd1234/og.png");
    assert_eq!(card.status, 410, "/r/abcd1234/og.png");
    assert!(
        card.body.is_empty(),
        "og.png 410 body: {} bytes",
        card.body.len()
    );

    assert_eq!(get(srv.port, h, "/r/x").status, 404, "/r/x");
}

#[test]
fn snapshots_cannot_be_enabled_in_netray_config() {
    let dir = std::env::temp_dir().join(format!("netray-gone-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("netray.toml");
    std::fs::write(&cfg, "[snapshots]\nenabled = true\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_netray"))
        .args(["serve", "--check-config", cfg.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
