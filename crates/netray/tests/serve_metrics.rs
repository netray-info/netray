//! v2-cutover Phase 4 (requirements 16, 17; SDD 9.2): `netray serve` exports exactly the `netray_*`
//! series of the metric table, registered with HELP and TYPE at startup, and no per-service
//! `lens_*`-style names.

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
        std::env::temp_dir().join(format!("netray-metrics-{tag}-{}", std::process::id()));
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

fn metrics_text(srv: &Server) -> String {
    let m = get(srv.metrics, Some("netray.info"), "/metrics");
    assert_eq!(m.status, 200);
    String::from_utf8_lossy(&m.body).into_owned()
}

fn sample_name(line: &str) -> &str {
    line.split(|c: char| c == '{' || c.is_whitespace())
        .next()
        .unwrap_or("")
}

/// Metric names the exposition mentions, in `# HELP`, `# TYPE` and sample lines.
fn exposed_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let name = if let Some(rest) = line
            .strip_prefix("# TYPE ")
            .or_else(|| line.strip_prefix("# HELP "))
        {
            sample_name(rest)
        } else if line.starts_with('#') {
            continue;
        } else {
            sample_name(line)
        };
        if !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
    }
    names
}

#[test]
fn every_metric_of_the_table_is_registered_at_startup() {
    let srv = start("table", None);
    let text = metrics_text(&srv);

    let table: [(&str, &str); 13] = [
        ("netray_runs_total", "counter"),
        ("netray_runs_with_predecessor_total", "counter"),
        ("netray_section_outcomes_total", "counter"),
        ("netray_time_to_first_section_seconds", "histogram"),
        ("netray_time_to_result_seconds", "histogram"),
        ("netray_run_interval_seconds", "histogram"),
        ("netray_fixes_observed_total", "counter"),
        ("netray_badge_requests_total", "counter"),
        ("netray_entry_views_total", "counter"),
        ("netray_api_requests_total", "counter"),
        ("netray_admission_rejections_total", "counter"),
        ("netray_history_entries", "gauge"),
        ("netray_history_domains", "gauge"),
    ];
    let mut missing = Vec::new();
    for (name, kind) in table {
        if !text.contains(&format!("# HELP {name} ")) && !text.contains(&format!("# HELP {name}\n"))
        {
            missing.push(format!("{name}: no # HELP line"));
        }
        if !text
            .lines()
            .any(|l| l.trim() == format!("# TYPE {name} {kind}"))
        {
            missing.push(format!("{name}: no `# TYPE {name} {kind}` line"));
        }
    }

    // Labelled counters appear at startup with their label names on a zero series.
    let labelled: [(&str, &[&str]); 5] = [
        ("netray_runs_total", &["outcome", "trigger"]),
        ("netray_section_outcomes_total", &["section", "outcome"]),
        ("netray_fixes_observed_total", &["section"]),
        ("netray_badge_requests_total", &["cache"]),
        ("netray_admission_rejections_total", &["reason"]),
    ];
    for (name, labels) in labelled {
        let ok = text.lines().any(|l| {
            let l = l.trim();
            l.starts_with(&format!("{name}{{"))
                && labels.iter().all(|k| l.contains(&format!("{k}=\"")))
        });
        if !ok {
            missing.push(format!("{name}: no zero series carrying labels {labels:?}"));
        }
    }
    assert!(
        missing.is_empty(),
        "metric table gaps:\n  {}\n--- exposition ---\n{text}",
        missing.join("\n  ")
    );
}

#[test]
fn only_netray_names_are_exported() {
    let srv = start("names", None);
    let apex = Some("netray.info");
    assert_eq!(get(srv.port, apex, "/").status, 200);
    assert_eq!(get(srv.port, apex, "/guide/").status, 200);
    assert_eq!(get(srv.port, apex, "/static/favicon.svg").status, 200);
    assert_eq!(
        get(srv.port, Some("dns.netray.info"), "/api/query").status,
        410
    );
    assert_eq!(get(srv.port, Some("lens.netray.info"), "/x").status, 301);
    let _ = get(srv.port, apex, "/r/abcd1234");

    let text = metrics_text(&srv);
    let names = exposed_names(&text);
    let old = [
        "lens_",
        "prism_",
        "tlsight_",
        "spectra_",
        "beacon_",
        "ifconfig_",
    ];
    let offenders: Vec<&String> = names
        .iter()
        .filter(|n| !n.starts_with("netray_") || old.iter().any(|p| n.starts_with(p)))
        .collect();
    assert!(
        offenders.is_empty(),
        "names outside netray_* or with an old service prefix: {offenders:?}"
    );
}
