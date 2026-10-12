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
        Box::pin(async move { r.lookup_ips(&h).await.map(|a| a.ips).unwrap_or_default() })
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

/// The addresses of an A + AAAA lookup in which at least one half answered definitively.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddressLookup {
    pub ips: Vec<IpAddr>,
    /// The half that failed, if one did; its `record_type` is `"A"` or `"AAAA"`.
    pub error: Option<LookupError>,
}

impl AddressLookup {
    /// The error of the failed half when it is `ip`'s family, so `ip`'s absence is not measured.
    pub fn missed(&self, ip: &IpAddr) -> Option<&LookupError> {
        let family = if ip.is_ipv4() { "A" } else { "AAAA" };
        self.error.as_ref().filter(|e| e.record_type == family)
    }
}

/// Minimal DNS lookup surface required by the category `check_*` functions.
///
/// Both the production [`DnsResolver`] and the in-memory
/// [`test_support::TestDnsResolver`] implement this trait, which lets tests
/// inject deterministic responses without going over the network. A record
/// lookup returns `Ok` with the records of a definite answer (empty for
/// NXDOMAIN or NODATA) and [`LookupError`] when no nameserver answered
/// definitively. `lookup_ips` is an error only when both the A and the AAAA
/// half failed; when one did, [`AddressLookup::error`] carries it. `check_dnssec_signed` is an error when the DNSKEY lookup failed
/// (unknown, not unsigned), as is `lookup_exists` when its A lookup failed.
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
    ) -> impl Future<Output = Result<AddressLookup, LookupError>> + Send;
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
    fn lookup_exists(&self, name: &str) -> impl Future<Output = Result<bool, LookupError>> + Send;
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
    ) -> impl Future<Output = Result<AddressLookup, LookupError>> + Send {
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
    fn lookup_exists(&self, name: &str) -> impl Future<Output = Result<bool, LookupError>> + Send {
        DnsResolver::lookup_exists(self, name)
    }
    fn check_dnssec_signed(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<bool, LookupError>> + Send {
        DnsResolver::check_dnssec_signed(self, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missed_is_the_failed_family_only() {
        let v4: IpAddr = "192.0.2.10".parse().unwrap();
        let v6: IpAddr = "2001:db8::25".parse().unwrap();
        let failed = |record_type| AddressLookup {
            ips: vec![v4],
            error: Some(LookupError {
                record_type,
                name: "mail.example.com".to_string(),
                cause: "SERVFAIL".to_string(),
            }),
        };

        let aaaa = failed("AAAA");
        assert!(aaaa.missed(&v4).is_none());
        assert_eq!(aaaa.missed(&v6), aaaa.error.as_ref());

        let a = failed("A");
        assert_eq!(a.missed(&v4), a.error.as_ref());
        assert!(a.missed(&v6).is_none());

        let answered = AddressLookup {
            ips: vec![v4],
            error: None,
        };
        assert!(answered.missed(&v4).is_none());
        assert!(answered.missed(&v6).is_none());
    }
}
