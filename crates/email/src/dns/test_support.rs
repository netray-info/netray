//! Test-only DNS resolver stub.
//!
//! `TestDnsResolver` is an in-memory fake used by the check tests and, under the
//! `testing` feature, by the integration tests. It implements [`super::DnsLookup`]
//! so `check_*` functions can consume it generically. A name without records is a
//! definite empty answer; a name marked with a `fail_*` builder errors with
//! SERVFAIL.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::{AddressLookup, DnsLookup, LookupError, TlsaRecord};

#[derive(Default)]
pub struct TestDnsResolver {
    txt: HashMap<String, Vec<String>>,
    mx: HashMap<String, Vec<(u16, String)>>,
    ips: HashMap<String, Vec<IpAddr>>,
    cname: HashMap<String, Vec<String>>,
    ptr: HashMap<String, Vec<String>>,
    a: HashMap<String, Vec<Ipv4Addr>>,
    tlsa: HashMap<String, Vec<TlsaRecord>>,
    dnssec_signed: HashSet<String>,
    error_names: HashSet<String>,
    fail_mx: HashSet<String>,
    fail_txt: HashSet<String>,
    fail_a: HashSet<String>,
    fail_ptr: HashSet<String>,
    fail_tlsa: HashSet<String>,
    fail_aaaa: HashSet<String>,
    fail_ips: HashSet<String>,
    fail_dnskey: HashSet<String>,
    delay: Option<Duration>,
    /// Per-name counter: how many times the name was queried (any record type).
    query_counts: std::sync::Mutex<HashMap<String, usize>>,
    total_queries: AtomicUsize,
}

impl TestDnsResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_txt(mut self, name: &str, records: Vec<&str>) -> Self {
        self.txt.insert(
            name.to_string(),
            records.into_iter().map(String::from).collect(),
        );
        self
    }

    pub fn with_mx(mut self, name: &str, records: Vec<(u16, &str)>) -> Self {
        self.mx.insert(
            name.to_string(),
            records
                .into_iter()
                .map(|(p, h)| (p, h.to_string()))
                .collect(),
        );
        self
    }

    pub fn with_ips(mut self, name: &str, ips: Vec<IpAddr>) -> Self {
        self.ips.insert(name.to_string(), ips);
        self
    }

    pub fn with_cname(mut self, name: &str, cnames: Vec<&str>) -> Self {
        self.cname.insert(
            name.to_string(),
            cnames.into_iter().map(String::from).collect(),
        );
        self
    }

    pub fn with_ptr(mut self, ip: IpAddr, names: Vec<&str>) -> Self {
        self.ptr.insert(
            ip.to_string(),
            names.into_iter().map(String::from).collect(),
        );
        self
    }

    pub fn with_a(mut self, name: &str, ips: Vec<Ipv4Addr>) -> Self {
        self.a.insert(name.to_string(), ips);
        self
    }

    pub fn with_tlsa(mut self, name: &str, records: Vec<TlsaRecord>) -> Self {
        self.tlsa.insert(name.to_string(), records);
        self
    }

    pub fn with_dnssec_signed(mut self, name: &str) -> Self {
        self.dnssec_signed.insert(name.to_string());
        self
    }

    pub fn with_delay(mut self, d: Duration) -> Self {
        self.delay = Some(d);
        self
    }

    /// Every lookup of `name`, of any record type, errors.
    pub fn fail_with(mut self, name: &str) -> Self {
        self.error_names.insert(name.to_string());
        self
    }

    /// The MX lookup of `name` errors.
    pub fn fail_mx(mut self, name: &str) -> Self {
        self.fail_mx.insert(name.to_string());
        self
    }

    /// The TXT lookup of `name` errors.
    pub fn fail_txt(mut self, name: &str) -> Self {
        self.fail_txt.insert(name.to_string());
        self
    }

    /// The A lookup of `name` errors, and the A half of `lookup_ips(name)`.
    pub fn fail_a(mut self, name: &str) -> Self {
        self.fail_a.insert(name.to_string());
        self
    }

    /// The PTR lookup of `ip` errors.
    pub fn fail_ptr(mut self, ip: IpAddr) -> Self {
        self.fail_ptr.insert(ip.to_string());
        self
    }

    /// The TLSA lookup of `name` errors.
    pub fn fail_tlsa(mut self, name: &str) -> Self {
        self.fail_tlsa.insert(name.to_string());
        self
    }

    /// The AAAA half of `lookup_ips(name)` errors: it answers the IPv4 addresses with the
    /// AAAA error.
    pub fn fail_aaaa(mut self, name: &str) -> Self {
        self.fail_aaaa.insert(name.to_string());
        self
    }

    /// Both the A and the AAAA half of `lookup_ips(name)` error.
    pub fn fail_ips(mut self, name: &str) -> Self {
        self.fail_ips.insert(name.to_string());
        self
    }

    /// The DNSKEY lookup behind `check_dnssec_signed(name)` errors.
    pub fn fail_dnskey(mut self, name: &str) -> Self {
        self.fail_dnskey.insert(name.to_string());
        self
    }

    /// Number of queries (any record type) made against `name`.
    pub fn query_count(&self, name: &str) -> usize {
        self.query_counts
            .lock()
            .ok()
            .and_then(|m| m.get(name).copied())
            .unwrap_or(0)
    }

    pub fn total_queries(&self) -> usize {
        self.total_queries.load(Ordering::Relaxed)
    }

    fn record_query(&self, name: &str) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut map) = self.query_counts.lock() {
            *map.entry(name.to_string()).or_insert(0) += 1;
        }
    }

    /// The records of `name` in `map`, or SERVFAIL when `name` is marked failing.
    fn answer<T: Clone>(
        &self,
        record_type: &'static str,
        name: &str,
        failing: Option<&HashSet<String>>,
        map: &HashMap<String, Vec<T>>,
    ) -> Result<Vec<T>, LookupError> {
        if self.error_names.contains(name) || failing.is_some_and(|f| f.contains(name)) {
            return Err(LookupError {
                record_type,
                name: name.to_string(),
                cause: "SERVFAIL".to_string(),
            });
        }
        Ok(map.get(name).cloned().unwrap_or_default())
    }

    async fn maybe_sleep(&self) {
        if let Some(d) = self.delay {
            tokio::time::sleep(d).await;
        }
    }
}

impl DnsLookup for TestDnsResolver {
    fn lookup_txt(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            self.answer("TXT", &name, Some(&self.fail_txt), &self.txt)
        }
    }

    fn lookup_mx(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<(u16, String)>, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            self.answer("MX", &name, Some(&self.fail_mx), &self.mx)
        }
    }

    fn lookup_ips(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<AddressLookup, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            let a_failed = self.fail_ips.contains(&name) || self.fail_a.contains(&name);
            let aaaa_failed = self.fail_ips.contains(&name) || self.fail_aaaa.contains(&name);
            let failed = |record_type| LookupError {
                record_type,
                name: name.clone(),
                cause: "SERVFAIL".to_string(),
            };
            if a_failed && aaaa_failed {
                return Err(failed("address"));
            }
            let ips = self.answer("address", &name, None, &self.ips)?;
            if a_failed {
                return Ok(AddressLookup {
                    ips: ips.into_iter().filter(IpAddr::is_ipv6).collect(),
                    error: Some(failed("A")),
                });
            }
            if aaaa_failed {
                return Ok(AddressLookup {
                    ips: ips.into_iter().filter(IpAddr::is_ipv4).collect(),
                    error: Some(failed("AAAA")),
                });
            }
            Ok(AddressLookup { ips, error: None })
        }
    }

    fn lookup_cname(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            self.answer("CNAME", &name, None, &self.cname)
        }
    }

    fn lookup_ptr(
        &self,
        ip: IpAddr,
    ) -> impl Future<Output = Result<Vec<String>, LookupError>> + Send {
        let key = ip.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&key);
            self.answer("PTR", &key, Some(&self.fail_ptr), &self.ptr)
        }
    }

    fn lookup_a(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<Ipv4Addr>, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            self.answer("A", &name, Some(&self.fail_a), &self.a)
        }
    }

    fn lookup_tlsa(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<TlsaRecord>, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            self.answer("TLSA", &name, Some(&self.fail_tlsa), &self.tlsa)
        }
    }

    fn lookup_exists(&self, name: &str) -> impl Future<Output = Result<bool, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name)
                || self.fail_a.contains(&name)
                || self.fail_ips.contains(&name)
            {
                return Err(LookupError {
                    record_type: "A",
                    name,
                    cause: "SERVFAIL".to_string(),
                });
            }
            Ok(!self.a.get(&name).cloned().unwrap_or_default().is_empty()
                || !self.ips.get(&name).cloned().unwrap_or_default().is_empty())
        }
    }

    fn check_dnssec_signed(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<bool, LookupError>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) || self.fail_dnskey.contains(&name) {
                return Err(LookupError {
                    record_type: "DNSKEY",
                    name,
                    cause: "SERVFAIL".to_string(),
                });
            }
            Ok(self.dnssec_signed.contains(&name))
        }
    }
}
