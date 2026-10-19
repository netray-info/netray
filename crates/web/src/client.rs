//! The caller address of a request, shared by the API, the inspector pages and the result page.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::http::HeaderMap;
use netray_common::ip_extract::IpExtractor;

/// The caller address: `CF-Connecting-IP` dropped, then the configured extractor.
pub(crate) fn client_ip(
    extractor: &IpExtractor,
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
) -> IpAddr {
    let mut headers = headers.clone();
    headers.remove("cf-connecting-ip");
    let peer = peer.unwrap_or_else(|| SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0));
    extractor.extract(&headers, peer)
}
