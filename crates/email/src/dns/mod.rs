pub mod resolver;

#[cfg(any(test, feature = "testing"))]
pub mod test_support;

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::Arc;

pub use resolver::{DnsResolver, TlsaRecord};

/// [`netray_common::fetch::Resolve`] through a [`DnsLookup`], so outbound
/// fetches resolve with the same resolver as the checks.
pub struct FetchResolver<R>(pub Arc<R>);

impl<R: DnsLookup + 'static> netray_common::fetch::Resolve for FetchResolver<R> {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let r = self.0.clone();
        let h = host.to_owned();
        Box::pin(async move { r.lookup_ips(&h).await.unwrap_or_default() })
    }
}

/// A lookup that got no definite answer: an error rcode (SERVFAIL, REFUSED), a timeout, a
/// referral or no response at all. A definite empty answer (NXDOMAIN, NODATA) is not an error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{record_type} lookup of {name} failed: {cause}")]
pub struct LookupError {
    pub record_type: &'static str,
    pub name: String,
    pub cause: String,
}

/// Minimal DNS lookup surface required by the category `check_*` functions.
///
/// Both the production [`DnsResolver`] and the in-memory
/// [`test_support::TestDnsResolver`] implement this trait, which lets tests
/// inject deterministic responses without going over the network. A record
/// lookup returns `Ok` with the records of a definite answer (empty for
/// NXDOMAIN or NODATA) and [`LookupError`] when no nameserver answered
/// definitively. `lookup_ips` is an error when either the A or the AAAA half
/// failed. `check_dnssec_signed` is an error when the DNSKEY lookup failed
/// (unknown, not unsigned). `lookup_exists` folds errors into `false`.
pub trait DnsLookup: Send + Sync {
    fn lookup_txt(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send;
    fn lookup_mx(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<(u16, String)>, LookupError>> + Send;
    fn lookup_ips(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<IpAddr>, LookupError>> + Send;
    fn lookup_cname(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send;
    fn lookup_ptr(
        &self,
        ip: IpAddr,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send;
    fn lookup_a(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<Ipv4Addr>, LookupError>> + Send;
    fn lookup_tlsa(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<TlsaRecord>, LookupError>> + Send;
    fn lookup_exists(&self, name: &str) -> impl Future<Output = bool> + Send;
    fn check_dnssec_signed(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<bool, LookupError>> + Send;
}

impl DnsLookup for DnsResolver {
    fn lookup_txt(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send {
        DnsResolver::lookup_txt(self, name)
    }
    fn lookup_mx(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<(u16, String)>, LookupError>> + Send {
        DnsResolver::lookup_mx(self, name)
    }
    fn lookup_ips(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<IpAddr>, LookupError>> + Send {
        DnsResolver::lookup_ips(self, name)
    }
    fn lookup_cname(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send {
        DnsResolver::lookup_cname(self, name)
    }
    fn lookup_ptr(
        &self,
        ip: IpAddr,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send {
        DnsResolver::lookup_ptr(self, ip)
    }
    fn lookup_a(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<Ipv4Addr>, LookupError>> + Send {
        DnsResolver::lookup_a(self, name)
    }
    fn lookup_tlsa(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<TlsaRecord>, LookupError>> + Send {
        DnsResolver::lookup_tlsa(self, name)
    }
    fn lookup_exists(&self, name: &str) -> impl Future<Output = bool> + Send {
        DnsResolver::lookup_exists(self, name)
    }
    fn check_dnssec_signed(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<bool, LookupError>> + Send {
        DnsResolver::check_dnssec_signed(self, name)
    }
}
