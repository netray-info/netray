//! Outbound fetch policy: HTTP requests to user-supplied URLs.
//!
//! [`fetch`] builds one reqwest client per call. Its DNS resolver checks every address with
//! [`FetchOptions::allow`] and hands hyper only the checked addresses, so every connection to
//! a host name, on every redirect hop, goes to a checked address. IP-literal hosts never reach
//! a resolver; they are checked before the first request and in the redirect policy.
//!
//! A caller configures the client only through [`ClientSettings`]. It cannot hand in a
//! builder, so it cannot add `resolve` overrides, a Unix socket or a proxy: each would route a
//! connection around the checking resolver. Environment proxies are ignored for the same
//! reason.

use std::error::Error as StdError;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use reqwest::header::HeaderMap;
use reqwest::redirect::Policy;
use reqwest::{Method, StatusCode};
use url::{Host, Url};

use crate::target_policy::is_allowed_target;

/// Resolves a host name to its addresses.
pub trait Resolve: Send + Sync + 'static {
    /// Returns every address of `host`, or an empty set if it does not resolve.
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>>;
}

/// [`Resolve`] through the system resolver (`tokio::net::lookup_host`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemResolver;

impl Resolve for SystemResolver {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let host = host.to_owned();
        Box::pin(async move {
            match tokio::net::lookup_host((host.as_str(), 0)).await {
                Ok(addrs) => addrs.map(|a| a.ip()).collect(),
                Err(_) => Vec::new(),
            }
        })
    }
}

/// Client settings a caller may choose; [`fetch`] applies them to a fresh builder.
#[derive(Debug, Clone, Default)]
pub struct ClientSettings {
    /// `User-Agent` header. Default: none.
    pub user_agent: Option<String>,
    /// Accept invalid TLS certificates. Default: `false`.
    pub accept_invalid_certs: bool,
    /// Extra trusted root certificates.
    pub root_certificates: Vec<reqwest::Certificate>,
    /// Headers sent on every hop.
    pub default_headers: HeaderMap,
}

/// What [`fetch`] does with a redirect that arrives after `max_redirects` hops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtLimit {
    /// Fail with [`FetchError::TooManyRedirects`].
    Fail,
    /// Return the unfollowed redirect with [`FetchResponse::limit_reached`] set.
    ReturnLast,
}

/// Options for one [`fetch`] call.
#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// Request method of the first hop.
    pub method: Method,
    /// Request body of the first hop.
    pub body: Option<Bytes>,
    /// Request headers of the first hop. Default: empty.
    pub headers: HeaderMap,
    /// Redirects followed before `at_limit` applies. Default: 0.
    pub max_redirects: usize,
    /// Behaviour at the redirect limit. Default: [`AtLimit::Fail`].
    pub at_limit: AtLimit,
    /// Refuse plain `http` on every hop. Default: `false`.
    pub https_only: bool,
    /// Largest accepted response body in bytes. Default: 64 KiB.
    pub body_cap: usize,
    /// Read the final response's body. Default: `true`.
    pub read_body: bool,
    /// Cut a body over `body_cap` at the cap instead of failing. Default: `false`.
    pub truncate_body: bool,
    /// Deadline for the whole chain, name resolution to end of body. Default: 10 s.
    pub timeout: Duration,
    /// Address predicate; every resolved address must pass. Default: [`is_allowed_target`].
    pub allow: fn(IpAddr) -> bool,
}

impl FetchOptions {
    /// Options with the defaults listed on each field.
    pub fn new(method: Method) -> Self {
        Self {
            method,
            body: None,
            headers: HeaderMap::new(),
            max_redirects: 0,
            at_limit: AtLimit::Fail,
            https_only: false,
            body_cap: 64 * 1024,
            read_body: true,
            truncate_body: false,
            timeout: Duration::from_secs(10),
            allow: is_allowed_target,
        }
    }
}

/// A followed redirect.
#[derive(Debug, Clone)]
pub struct Hop {
    /// URL that answered with the redirect, without userinfo.
    pub url: Url,
    /// Redirect status.
    pub status: StatusCode,
    /// `Location` joined against `url`, without userinfo.
    pub location: String,
}

/// The final response of a [`fetch`].
#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub status: StatusCode,
    pub version: reqwest::Version,
    pub headers: HeaderMap,
    /// URL of the final response, without userinfo.
    pub url: Url,
    /// Followed redirects, in order.
    pub hops: Vec<Hop>,
    /// The response is a redirect left unfollowed under [`AtLimit::ReturnLast`].
    pub limit_reached: bool,
    /// Empty unless [`FetchOptions::read_body`] and the response is no redirect.
    pub body: Bytes,
    /// `body` was cut at [`FetchOptions::body_cap`].
    pub body_truncated: bool,
}

/// Why a target was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// The host resolved to no address.
    NoAddress,
    /// An address failed [`FetchOptions::allow`].
    Disallowed,
}

/// Errors returned by [`fetch`].
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// `hop` is 0 for the initial URL and n for the n-th redirect target; `hops` are the
    /// redirects followed up to the refused target.
    #[error("target not allowed")]
    Blocked {
        url: Url,
        reason: BlockReason,
        hop: usize,
        hops: Vec<Hop>,
    },
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("scheme not allowed: {}", .0.scheme())]
    Scheme(Url),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("response body too large")]
    BodyTooLarge,
    #[error("timed out")]
    Timeout,
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("target not allowed")]
struct Refused(BlockReason);

#[derive(Debug, thiserror::Error)]
#[error("too many redirects")]
struct LimitExceeded;

#[derive(Debug, thiserror::Error)]
#[error("scheme not allowed")]
struct SchemeRefused(Url);

/// Redirect state shared between the policy and [`fetch`].
struct State {
    hops: Vec<Hop>,
    /// URL of the current request, without userinfo.
    current: Url,
    limit_reached: bool,
}

/// reqwest resolver that returns only addresses passing `allow`.
struct Checked {
    inner: Arc<dyn Resolve>,
    allow: fn(IpAddr) -> bool,
}

impl reqwest::dns::Resolve for Checked {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let lookup = self.inner.resolve(name.as_str());
        let allow = self.allow;
        Box::pin(async move {
            let addrs = lookup.await;
            let reason = if addrs.is_empty() {
                Some(BlockReason::NoAddress)
            } else if !addrs.iter().all(|ip| allow(*ip)) {
                Some(BlockReason::Disallowed)
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(Box::new(Refused(reason)) as _);
            }
            let addrs: reqwest::dns::Addrs =
                Box::new(addrs.into_iter().map(|ip| SocketAddr::new(ip, 0)));
            Ok(addrs)
        })
    }
}

/// Fetches `url` under the outbound fetch policy.
///
/// The client is built from `settings` alone, with no proxy (environment proxies included),
/// `opts.timeout` as total timeout, the checking resolver and the redirect policy; redirects
/// themselves (Location joining, method rules, header stripping, Referer) are reqwest's. The
/// initial URL's fragment is dropped before the request.
pub async fn fetch(
    settings: &ClientSettings,
    resolver: Arc<dyn Resolve>,
    url: &str,
    opts: &FetchOptions,
) -> Result<FetchResponse, FetchError> {
    fetch_traced(settings, resolver, url, opts).await.0
}

/// [`fetch`], also returning the redirects followed before the outcome, whether it is a
/// response or an error.
pub async fn fetch_traced(
    settings: &ClientSettings,
    resolver: Arc<dyn Resolve>,
    url: &str,
    opts: &FetchOptions,
) -> (Result<FetchResponse, FetchError>, Vec<Hop>) {
    let mut url = match Url::parse(url) {
        Ok(url) => url,
        Err(e) => return (Err(FetchError::InvalidUrl(e.to_string())), Vec::new()),
    };
    url.set_fragment(None);
    let state = Arc::new(Mutex::new(State {
        hops: Vec::new(),
        current: without_userinfo(&url),
        limit_reached: false,
    }));
    let result = fetch_with(settings, resolver, url, opts, &state).await;
    let hops = lock(&state).hops.clone();
    (result, hops)
}

async fn fetch_with(
    settings: &ClientSettings,
    resolver: Arc<dyn Resolve>,
    url: Url,
    opts: &FetchOptions,
    state: &Arc<Mutex<State>>,
) -> Result<FetchResponse, FetchError> {
    if !scheme_allowed(&url, opts.https_only) {
        return Err(FetchError::Scheme(url));
    }
    if !literal_allowed(&url, opts.allow) {
        return Err(blocked(state, BlockReason::Disallowed));
    }
    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(settings.accept_invalid_certs)
        .default_headers(settings.default_headers.clone());
    if let Some(user_agent) = &settings.user_agent {
        builder = builder.user_agent(user_agent);
    }
    for cert in &settings.root_certificates {
        builder = builder.add_root_certificate(cert.clone());
    }
    let client = builder
        .no_proxy()
        .timeout(opts.timeout)
        .dns_resolver(Arc::new(Checked {
            inner: resolver,
            allow: opts.allow,
        }))
        .redirect(policy(Arc::clone(state), opts))
        .build()?;
    let mut request = client
        .request(opts.method.clone(), url)
        .headers(opts.headers.clone());
    if let Some(body) = &opts.body {
        request = request.body(body.clone());
    }
    let mut response = request.send().await.map_err(|e| map_error(e, state))?;
    let status = response.status();
    let version = response.version();
    let headers = response.headers().clone();
    let url = without_userinfo(response.url());
    let mut body = BytesMut::new();
    let mut body_truncated = false;
    if opts.read_body && !status.is_redirection() {
        while let Some(chunk) = response.chunk().await.map_err(|e| map_error(e, state))? {
            let room = opts.body_cap - body.len();
            if chunk.len() > room {
                if !opts.truncate_body {
                    return Err(FetchError::BodyTooLarge);
                }
                body.extend_from_slice(&chunk[..room]);
                body_truncated = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }
    }
    let state = lock(state);
    Ok(FetchResponse {
        status,
        version,
        headers,
        url,
        hops: state.hops.clone(),
        limit_reached: state.limit_reached,
        body: body.freeze(),
        body_truncated,
    })
}

/// Records each followed hop and refuses a hop over the limit, with a refused scheme, or with
/// a refused IP-literal host.
fn policy(state: Arc<Mutex<State>>, opts: &FetchOptions) -> Policy {
    let (max, at_limit, https_only, allow) = (
        opts.max_redirects,
        opts.at_limit,
        opts.https_only,
        opts.allow,
    );
    Policy::custom(move |attempt| {
        let mut s = lock(&state);
        if s.hops.len() >= max {
            return match at_limit {
                AtLimit::Fail => attempt.error(LimitExceeded),
                AtLimit::ReturnLast => {
                    s.limit_reached = true;
                    attempt.stop()
                }
            };
        }
        let next = without_userinfo(attempt.url());
        // Record before any refusal, so a caller keeps the redirect it was shown.
        let from = std::mem::replace(&mut s.current, next.clone());
        s.hops.push(Hop {
            url: from,
            status: attempt.status(),
            location: next.to_string(),
        });
        if !scheme_allowed(&next, https_only) {
            return attempt.error(SchemeRefused(next));
        }
        if !literal_allowed(&next, allow) {
            return attempt.error(Refused(BlockReason::Disallowed));
        }
        attempt.follow()
    })
}

fn map_error(e: reqwest::Error, state: &Mutex<State>) -> FetchError {
    let mut source: Option<&(dyn StdError + 'static)> = Some(&e);
    while let Some(err) = source {
        if let Some(Refused(reason)) = err.downcast_ref() {
            return blocked(state, *reason);
        }
        if err.is::<LimitExceeded>() {
            return FetchError::TooManyRedirects;
        }
        if let Some(SchemeRefused(url)) = err.downcast_ref() {
            return FetchError::Scheme(url.clone());
        }
        source = err.source();
    }
    if e.is_timeout() {
        FetchError::Timeout
    } else {
        FetchError::Http(e)
    }
}

fn blocked(state: &Mutex<State>, reason: BlockReason) -> FetchError {
    let s = lock(state);
    FetchError::Blocked {
        url: s.current.clone(),
        reason,
        hop: s.hops.len(),
        hops: s.hops.clone(),
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn scheme_allowed(url: &Url, https_only: bool) -> bool {
    matches!((url.scheme(), https_only), ("https", _) | ("http", false))
}

/// `false` for an IP-literal host failing `allow`; host names are checked by the resolver.
fn literal_allowed(url: &Url, allow: fn(IpAddr) -> bool) -> bool {
    match url.host() {
        Some(Host::Ipv4(ip)) => allow(IpAddr::V4(ip)),
        Some(Host::Ipv6(ip)) => allow(IpAddr::V6(ip)),
        _ => true,
    }
}

fn without_userinfo(url: &Url) -> Url {
    let mut url = url.clone();
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url
}
