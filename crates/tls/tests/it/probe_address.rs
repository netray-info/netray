//! v2-tls-versions-coop, C3/C16/C17/C18/C19: `probe_address` fans out the version and
//! suite-group probes concurrently, one TCP connection each, and reads TLS 1.2 / 1.3 from
//! the handshake where it can. An in-process responder on 127.0.0.1 answers canned bytes.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict, probe_address};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Copy)]
enum Mode {
    /// Accepts TLS 1.0 and 1.1, answers alert 70 to everything else.
    LegacyOnly,
    /// Accepts and never answers.
    Silent,
    /// Reads the ClientHello, then resets the connection.
    Reset,
    /// Reads the ClientHello, then closes the connection (FIN) without a byte.
    Close,
}

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// Reads one ClientHello record; returns (client_version, offered suites).
async fn read_hello(s: &mut TcpStream) -> Option<(u16, Vec<u16>)> {
    let mut head = [0u8; 5];
    s.read_exact(&mut head).await.ok()?;
    let len = u16::from_be_bytes([head[3], head[4]]) as usize;
    let mut body = vec![0u8; len];
    s.read_exact(&mut body).await.ok()?;
    // body: 4-byte handshake header, version(2), random(32), session id, suites
    let version = u16::from_be_bytes([body[4], body[5]]);
    let sid = body[38] as usize;
    let at = 39 + sid;
    let n = u16::from_be_bytes([body[at], body[at + 1]]) as usize;
    let suites = body[at + 2..at + 2 + n]
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    Some((version, suites))
}

fn server_hello(version: u16, suite: u16) -> Vec<u8> {
    let v = version.to_be_bytes();
    let mut hs = vec![0x02, 0, 0, 38];
    hs.extend_from_slice(&v);
    hs.extend_from_slice(&[0u8; 32]);
    hs.push(0); // session id length
    hs.extend_from_slice(&suite.to_be_bytes());
    hs.push(0); // compression
    let mut rec = vec![0x16, v[0], v[1]];
    rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    rec.extend_from_slice(&hs);
    rec
}

const ALERT_70: [u8; 7] = [0x15, 0x03, 0x03, 0x00, 0x02, 0x02, 70];

async fn responder(mode: Mode) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                return;
            };
            seen.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                match mode {
                    Mode::Silent => {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        drop(s);
                    }
                    Mode::Reset => {
                        let _ = read_hello(&mut s).await;
                        // SO_LINGER 0 is the only way to force an RST; the blocking-on-drop
                        // caveat is irrelevant for a zero timeout.
                        #[allow(deprecated)]
                        s.set_linger(Some(Duration::ZERO)).unwrap();
                        drop(s);
                    }
                    Mode::Close => {
                        let _ = read_hello(&mut s).await;
                        let _ = s.shutdown().await;
                        // Keep the read half open briefly so the FIN, not an RST, ends the probe.
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        drop(s);
                    }
                    Mode::LegacyOnly => {
                        let Some((version, suites)) = read_hello(&mut s).await else {
                            return;
                        };
                        let reply = if version == 0x0301 || version == 0x0302 {
                            server_hello(version, suites[0])
                        } else {
                            ALERT_70.to_vec()
                        };
                        let _ = s.write_all(&reply).await;
                        let _ = s.shutdown().await;
                    }
                }
            });
        }
    });
    (port, count)
}

fn probed(p: &ProtocolProbes) -> [(&'static str, &ProbeOutcome); 6] {
    [
        ("ssl3", &p.ssl3),
        ("tls1_0", &p.tls1_0),
        ("tls1_1", &p.tls1_1),
        ("suites_insecure", &p.suites_insecure),
        ("suites_cbc", &p.suites_cbc),
        ("suites_static_rsa", &p.suites_static_rsa),
    ]
}

#[tokio::test]
async fn negotiated_tls13_probes_tls12_and_makes_seven_connections() {
    let (port, count) = responder(Mode::LegacyOnly).await;
    let p = probe_address(LOCALHOST, port, None, "TLSv1.3", Duration::from_secs(5)).await;

    assert_eq!(p.ssl3.verdict, Verdict::Refused);
    assert_eq!(p.tls1_0.verdict, Verdict::Accepted);
    assert_eq!(p.tls1_1.verdict, Verdict::Accepted);
    assert_eq!(p.tls1_2.verdict, Verdict::Refused);
    assert_eq!(p.tls1_3.verdict, Verdict::Accepted);
    assert_eq!(p.suites_insecure.verdict, Verdict::Refused);
    assert_eq!(p.suites_cbc.verdict, Verdict::Refused);
    assert_eq!(p.suites_static_rsa.verdict, Verdict::Refused);
    assert_eq!(count.load(Ordering::SeqCst), 7);
}

#[tokio::test]
async fn negotiated_tls12_comes_from_the_handshake_with_six_connections() {
    let (port, count) = responder(Mode::LegacyOnly).await;
    let p = probe_address(LOCALHOST, port, None, "TLSv1.2", Duration::from_secs(5)).await;

    assert_eq!(p.tls1_2.verdict, Verdict::Accepted);
    assert!(
        p.tls1_2.detail.contains("handshake"),
        "detail: {}",
        p.tls1_2.detail
    );
    assert_eq!(p.tls1_3.verdict, Verdict::Refused);
    assert_eq!(count.load(Ordering::SeqCst), 6);
}

#[tokio::test]
async fn silent_server_times_out_every_probe_concurrently() {
    let (port, _) = responder(Mode::Silent).await;
    let start = Instant::now();
    let p = probe_address(LOCALHOST, port, None, "TLSv1.3", Duration::from_millis(300)).await;
    let elapsed = start.elapsed();

    for (name, o) in probed(&p).into_iter().chain([("tls1_2", &p.tls1_2)]) {
        assert_eq!(o.verdict, Verdict::Unanswered, "{name}");
        assert!(o.detail.contains("timed out"), "{name}: {}", o.detail);
    }
    assert!(
        elapsed < Duration::from_millis(1000),
        "took {elapsed:?}: probes are not concurrent"
    );
}

// v2-tls-probe-close, C12: a reset after the ClientHello is the server's refusal (was
// unanswered under v2-tls-versions-coop).
#[tokio::test]
async fn reset_after_the_hello_is_refused_closed_reset() {
    let (port, _) = responder(Mode::Reset).await;
    let p = probe_address(LOCALHOST, port, None, "TLSv1.3", Duration::from_secs(5)).await;

    for (name, o) in probed(&p).into_iter().chain([("tls1_2", &p.tls1_2)]) {
        assert_eq!(o.verdict, Verdict::Refused, "{name}: {o:?}");
        assert_eq!(o.detail, "closed (reset)", "{name}: {o:?}");
    }
}

// v2-tls-probe-close, C11: a close (FIN) after the ClientHello is the server's refusal.
#[tokio::test]
async fn close_after_the_hello_is_refused_closed() {
    let (port, count) = responder(Mode::Close).await;
    let p = probe_address(LOCALHOST, port, None, "TLSv1.3", Duration::from_secs(5)).await;

    for (name, o) in probed(&p).into_iter().chain([("tls1_2", &p.tls1_2)]) {
        assert_eq!(o.verdict, Verdict::Refused, "{name}: {o:?}");
        assert_eq!(o.detail, "closed", "{name}: {o:?}");
    }
    assert_eq!(p.tls1_3.verdict, Verdict::Accepted);
    assert_eq!(count.load(Ordering::SeqCst), 7);
}
