pub mod resolver;

#[cfg(test)]
pub mod test_support;

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};

pub use resolver::{DnsResolver, TlsaRecord};

/// Minimal DNS lookup surface required by the category `check_*` functions.
///
/// Both the production [`DnsResolver`] and the in-memory
/// [`test_support::TestDnsResolver`] implement this trait, which lets tests
/// inject deterministic responses without going over the network. Lookup
/// failures are swallowed into empty results (mirroring the production
/// resolver's behaviour, which logs and returns `Vec::new()` on error).
pub trait DnsLookup: Send + Sync {
    fn lookup_txt(&self, name: &str) -> impl Future<Output = Vec<String>> + Send;
    fn lookup_mx(&self, name: &str) -> impl Future<Output = Vec<(u16, String)>> + Send;
    fn lookup_ips(&self, name: &str) -> impl Future<Output = Vec<IpAddr>> + Send;
    fn lookup_cname(&self, name: &str) -> impl Future<Output = Vec<String>> + Send;
    fn lookup_ptr(&self, ip: IpAddr) -> impl Future<Output = Vec<String>> + Send;
    fn lookup_a(&self, name: &str) -> impl Future<Output = Vec<Ipv4Addr>> + Send;
    fn lookup_tlsa(&self, name: &str) -> impl Future<Output = Vec<TlsaRecord>> + Send;
    fn lookup_exists(&self, name: &str) -> impl Future<Output = bool> + Send;
    fn check_dnssec_signed(&self, name: &str) -> impl Future<Output = bool> + Send;
}

impl DnsLookup for DnsResolver {
    fn lookup_txt(&self, name: &str) -> impl Future<Output = Vec<String>> + Send {
        DnsResolver::lookup_txt(self, name)
    }
    fn lookup_mx(&self, name: &str) -> impl Future<Output = Vec<(u16, String)>> + Send {
        DnsResolver::lookup_mx(self, name)
    }
    fn lookup_ips(&self, name: &str) -> impl Future<Output = Vec<IpAddr>> + Send {
        DnsResolver::lookup_ips(self, name)
    }
    fn lookup_cname(&self, name: &str) -> impl Future<Output = Vec<String>> + Send {
        DnsResolver::lookup_cname(self, name)
    }
    fn lookup_ptr(&self, ip: IpAddr) -> impl Future<Output = Vec<String>> + Send {
        DnsResolver::lookup_ptr(self, ip)
    }
    fn lookup_a(&self, name: &str) -> impl Future<Output = Vec<Ipv4Addr>> + Send {
        DnsResolver::lookup_a(self, name)
    }
    fn lookup_tlsa(&self, name: &str) -> impl Future<Output = Vec<TlsaRecord>> + Send {
        DnsResolver::lookup_tlsa(self, name)
    }
    fn lookup_exists(&self, name: &str) -> impl Future<Output = bool> + Send {
        DnsResolver::lookup_exists(self, name)
    }
    fn check_dnssec_signed(&self, name: &str) -> impl Future<Output = bool> + Send {
        DnsResolver::check_dnssec_signed(self, name)
    }
}
