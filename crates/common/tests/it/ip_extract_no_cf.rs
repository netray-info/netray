//! `CF-Connecting-IP` must be ignored: there is no Cloudflare in front of the suite.
//! Trusted proxies are honoured only via `X-Real-IP` / `X-Forwarded-For`.

use std::net::{IpAddr, SocketAddr};

use axum::http::{HeaderMap, HeaderValue};
use netray_common::ip_extract::IpExtractor;

const PEER: &str = "172.31.0.5:1234";

fn extractor() -> IpExtractor {
    IpExtractor::new(&["172.31.0.0/24".to_string()])
}

fn extract(headers: &[(&'static str, &'static str)]) -> IpAddr {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        map.insert(*name, HeaderValue::from_static(value));
    }
    extractor().extract(&map, PEER.parse::<SocketAddr>().unwrap())
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn x_real_ip_wins_over_cf_connecting_ip_from_trusted_proxy() {
    let got = extract(&[
        ("cf-connecting-ip", "198.51.100.1"),
        ("x-real-ip", "198.51.100.2"),
    ]);
    assert_eq!(got, ip("198.51.100.2"));
}

#[test]
fn x_forwarded_for_used_when_cf_connecting_ip_also_present() {
    let got = extract(&[
        ("cf-connecting-ip", "198.51.100.1"),
        ("x-forwarded-for", "198.51.100.3"),
    ]);
    assert_ne!(got, ip("198.51.100.1"));
    assert_eq!(got, ip("198.51.100.3"));
}

#[test]
fn cf_connecting_ip_alone_falls_back_to_peer() {
    let got = extract(&[("cf-connecting-ip", "198.51.100.1")]);
    assert_eq!(got, ip("172.31.0.5"));
}
