// The email resolver retries a lookup whose UDP answer carries the TC bit once over TCP to the
// same address and port, and the TCP answer is the lookup's answer; a failed retry names the TCP
// failure. A loopback responder stands in for the nameserver: a predefined provider cannot point
// at loopback, so the address-with-port entry covers the transport (the entry kinds share it).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hickory_proto::op::{Message, MessageType, OpCode};
use hickory_proto::rr::rdata::TXT;
use hickory_proto::rr::{RData, Record};
use netray_email::dns::resolver::DnsResolver;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

#[derive(Clone, Copy)]
enum Mode {
    /// UDP answers TC with no records; TCP (when listening) answers in full.
    Truncating { tcp: bool },
    /// UDP answers the small record, no TC.
    Small,
}

struct Responder {
    addr: SocketAddr,
    udp: Arc<AtomicUsize>,
    tcp: Arc<AtomicUsize>,
}

const SMALL: &str = "v=spf1 -all";

fn big_spf() -> String {
    let mut s = String::from("v=spf1");
    let mut i = 0;
    while s.len() < 3000 {
        s.push_str(&format!(" ip4:192.0.2.{}", i % 250));
        i += 1;
    }
    s.truncate(3000);
    s
}

fn answer(query: &Message, txt: Option<&str>, truncated: bool) -> Vec<u8> {
    let mut m = Message::response(query.metadata.id, OpCode::Query);
    m.metadata.message_type = MessageType::Response;
    m.metadata.truncation = truncated;
    m.metadata.recursion_available = true;
    for q in &query.queries {
        m.add_query(q.clone());
    }
    if let Some(txt) = txt {
        let chunks: Vec<&[u8]> = txt.as_bytes().chunks(255).collect();
        let name = query.queries[0].name().clone();
        m.add_answer(Record::from_rdata(
            name,
            300,
            RData::TXT(TXT::from_bytes(chunks)),
        ));
    }
    m.to_vec().unwrap()
}

async fn spawn(mode: Mode) -> Responder {
    let (udp, tcp_listener) = loop {
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = udp.local_addr().unwrap().port();
        match mode {
            Mode::Truncating { tcp: true } => {
                if let Ok(l) = TcpListener::bind(("127.0.0.1", port)).await {
                    break (udp, Some(l));
                }
            }
            _ => break (udp, None),
        }
    };
    let addr = udp.local_addr().unwrap();
    let udp_n = Arc::new(AtomicUsize::new(0));
    let tcp_n = Arc::new(AtomicUsize::new(0));
    let big = big_spf();

    let n = udp_n.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        loop {
            let Ok((len, peer)) = udp.recv_from(&mut buf).await else {
                return;
            };
            let Ok(q) = Message::from_vec(&buf[..len]) else {
                continue;
            };
            n.fetch_add(1, Ordering::SeqCst);
            let bytes = match mode {
                Mode::Truncating { .. } => answer(&q, None, true),
                Mode::Small => answer(&q, Some(SMALL), false),
            };
            let _ = udp.send_to(&bytes, peer).await;
        }
    });

    if let Some(listener) = tcp_listener {
        let n = tcp_n.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let n = n.clone();
                let big = big.clone();
                tokio::spawn(async move {
                    while let Ok(len) = s.read_u16().await {
                        let mut buf = vec![0u8; len as usize];
                        if s.read_exact(&mut buf).await.is_err() {
                            return;
                        }
                        let Ok(q) = Message::from_vec(&buf) else {
                            return;
                        };
                        n.fetch_add(1, Ordering::SeqCst);
                        let bytes = answer(&q, Some(&big), false);
                        if s.write_u16(bytes.len() as u16).await.is_err()
                            || s.write_all(&bytes).await.is_err()
                        {
                            return;
                        }
                    }
                });
            }
        });
    }
    Responder {
        addr,
        udp: udp_n,
        tcp: tcp_n,
    }
}

async fn resolver(r: &Responder) -> DnsResolver {
    DnsResolver::new(&[r.addr.to_string()], 2000)
        .await
        .expect("resolver builds against loopback")
}

#[tokio::test]
async fn truncated_udp_answer_is_retried_once_over_tcp_and_returned() {
    let r = spawn(Mode::Truncating { tcp: true }).await;
    let records = resolver(&r)
        .await
        .lookup_txt("example.com")
        .await
        .expect("the TCP answer is the lookup's answer");
    assert_eq!(records, vec![big_spf()]);
    assert_eq!(r.udp.load(Ordering::SeqCst), 1, "one UDP query");
    assert_eq!(r.tcp.load(Ordering::SeqCst), 1, "one TCP query");
}

#[tokio::test]
async fn failed_tcp_retry_names_the_tcp_failure() {
    let r = spawn(Mode::Truncating { tcp: false }).await;
    let err = resolver(&r)
        .await
        .lookup_txt("example.com")
        .await
        .expect_err("no TCP listener: the lookup fails");
    assert!(
        err.cause.contains("TCP retry failed"),
        "cause must name the TCP failure, got: {}",
        err.cause
    );
}

#[tokio::test]
async fn untruncated_udp_answer_makes_no_tcp_query() {
    let r = spawn(Mode::Small).await;
    let records = resolver(&r).await.lookup_txt("example.com").await.unwrap();
    assert_eq!(records, vec![SMALL.to_string()]);
    assert_eq!(r.udp.load(Ordering::SeqCst), 1, "one UDP query");
    assert_eq!(r.tcp.load(Ordering::SeqCst), 0, "no TCP query");
}
