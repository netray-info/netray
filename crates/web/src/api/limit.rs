//! The `[api]` per-IP limiter, keyed by the caller address.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use netray_common::rate_limit::check_keyed_cost;

use super::error::V1Error;
use super::state::ApiState;

/// The caller address: `CF-Connecting-IP` dropped, then the configured extractor.
pub(super) fn client(state: &ApiState, headers: &HeaderMap, peer: Option<SocketAddr>) -> IpAddr {
    let mut headers = headers.clone();
    headers.remove("cf-connecting-ip");
    let peer = peer.unwrap_or_else(|| SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0));
    state.ip_extractor().extract(&headers, peer)
}

fn client_ip(state: &ApiState, req: &Request) -> IpAddr {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    client(state, req.headers(), peer)
}

pub(super) async fn per_ip(State(state): State<ApiState>, req: Request, next: Next) -> Response {
    let ip = client_ip(&state, &req);
    match check_keyed_cost(state.limiter(), &ip, NonZeroU32::MIN, "api_v1", "lens") {
        Ok(()) => next.run(req).await,
        Err(rejection) => V1Error::RateLimited {
            retry_after_secs: rejection.retry_after_secs,
        }
        .into_response(),
    }
}
