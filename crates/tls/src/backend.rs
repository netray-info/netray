//! The two outward steps of an inspection: resolving a hostname and handshaking an address.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use netray_engine::BoxFuture;

use crate::dns::DnsResolver;
use crate::error::AppError;
use crate::routes::resolve_hostname;
use crate::tls::probe::ProtocolProbes;
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

    /// The connections the probes of one address cost on one port, charged to the rate limiters
    /// beside its handshake; 0 for a backend that does not probe.
    fn probe_connections(&self) -> u32;

    /// The protocol-version and suite-group probes of `ip:port`, after a handshake that
    /// negotiated `negotiated`; `None` when the backend does not probe.
    fn probe<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        sni: Option<&'a str>,
        negotiated: &'a str,
        timeout: Duration,
    ) -> BoxFuture<'a, Option<ProtocolProbes>>;
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

    fn probe_connections(&self) -> u32 {
        tls::probe::ProbeKind::ALL.len() as u32
    }

    fn probe<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        sni: Option<&'a str>,
        negotiated: &'a str,
        timeout: Duration,
    ) -> BoxFuture<'a, Option<ProtocolProbes>> {
        Box::pin(async move {
            Some(tls::probe::probe_address(ip, port, sni, negotiated, timeout).await)
        })
    }
}
