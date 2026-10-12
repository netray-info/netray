// V2 unmeasured, Phase 4: `execute_request` classifies a failed probe into a ProbeErrorKind.
// One loopback listener per row; the Blocked row never connects, so no row needs network.

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use netray_common::fetch::Resolve;
use netray_http::inspect::TaskResult;
use netray_http::inspect::assembler::ProbeErrorKind;
use netray_http::inspect::request::{Outbound, execute_request};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

const LO4: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

struct NoNames;

impl Resolve for NoNames {
    fn resolve(&self, _host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        Box::pin(async { Vec::new() })
    }
}

async fn run(url: String, port: u16) -> TaskResult {
    let outbound = Outbound {
        resolver: Arc::new(NoNames),
        allow: |ip| ip == LO4,
    };
    execute_request(
        &outbound,
        Url::parse(&url).unwrap(),
        SocketAddr::new(LO4, port),
        5,
        Duration::from_secs(1),
        "test-agent",
        None,
    )
    .await
}

async fn bind() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    (l, p)
}

/// Accept connections forever; `answer` gets each one after its request has been read.
fn serve(listener: TcpListener, answer: &'static [u8], hold: bool) {
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let _ =
                    tokio::time::timeout(Duration::from_millis(200), stream.read(&mut buf)).await;
                let _ = stream.write_all(answer).await;
                if hold {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            });
        }
    });
}

#[tokio::test]
async fn probe_error_kinds_table() {
    let mut failures = Vec::new();

    let (l, p) = bind().await;
    drop(l);
    let refused = run(format!("http://127.0.0.1:{p}/"), p).await;

    let (l, p) = bind().await;
    serve(l, b"", false);
    let reset = run(format!("http://127.0.0.1:{p}/"), p).await;

    let (l, p) = bind().await;
    serve(l, b"", true);
    let timeout = run(format!("http://127.0.0.1:{p}/"), p).await;

    let (l, p) = bind().await;
    serve(
        l,
        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        false,
    );
    let tls = run(format!("https://127.0.0.1:{p}/"), p).await;

    let (l, p) = bind().await;
    serve(
        l,
        b"HTTP/1.1 301 Moved Permanently\r\nLocation: https://10.0.0.1/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        false,
    );
    let blocked = run(format!("http://127.0.0.1:{p}/"), p).await;

    for (name, r, kind, text) in [
        ("dropped listener", &refused, ProbeErrorKind::Refused, None),
        ("accept then close", &reset, ProbeErrorKind::Reset, None),
        (
            "accept, never write",
            &timeout,
            ProbeErrorKind::Timeout,
            Some("timed out"),
        ),
        (
            "plain HTTP behind https://",
            &tls,
            ProbeErrorKind::Tls,
            None,
        ),
        (
            "redirect to 10.0.0.1",
            &blocked,
            ProbeErrorKind::Blocked,
            Some("10.0.0.1"),
        ),
    ] {
        let Some(e) = &r.error else {
            failures.push(format!("{name}: no error, status {}", r.status));
            continue;
        };
        if e.kind != kind {
            failures.push(format!("{name}: kind {:?}, want {kind:?} ({e})", e.kind));
        }
        if let Some(text) = text
            && !e.message.contains(text)
        {
            failures.push(format!("{name}: message {:?} lacks {text:?}", e.message));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
