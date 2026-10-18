//! The two outward steps of an inspection: resolving a hostname and handshaking an address.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use netray_engine::BoxFuture;

use crate::dns::DnsResolver;
use crate::error::AppError;
use crate::routes::resolve_hostname;
use crate::tls::{self, IpInspectionResult};

/// What an inspection asks of the network. `LiveBackend` is the real one; tests install a stub.
pub trait TlsBackend: Send + Sync + 'static {
    /// The addresses of `hostname`; an IP target never reaches this.
    fn resolve<'a>(&'a self, hostname: &'a str) -> BoxFuture<'a, Result<Vec<IpAddr>, AppError>>;

    /// One handshake with `ip:port`; STARTTLS follows from the port.
    fn handshake<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        sni: Option<&'a str>,
        timeout: Duration,
    ) -> BoxFuture<'a, IpInspectionResult>;
}

/// DNS through the service's resolver, handshakes over the network.
pub struct LiveBackend {
    pub resolver: Option<Arc<DnsResolver>>,
}

impl TlsBackend for LiveBackend {
    fn resolve<'a>(&'a self, hostname: &'a str) -> BoxFuture<'a, Result<Vec<IpAddr>, AppError>> {
        Box::pin(async move {
            let resolver = self.resolver.as_ref().ok_or_else(|| {
                AppError::DnsResolutionFailed("DNS resolver not available".to_string())
            })?;
            resolve_hostname(hostname, resolver).await
        })
    }

    fn handshake<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        sni: Option<&'a str>,
        timeout: Duration,
    ) -> BoxFuture<'a, IpInspectionResult> {
        Box::pin(tls::inspect_ip(ip, port, sni, timeout))
    }
}
