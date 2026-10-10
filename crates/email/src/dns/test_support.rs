//! Test-only DNS resolver stub.
//!
//! `TestDnsResolver` is an in-memory fake used by Wave-2 check tests (Track E).
//! It implements [`super::DnsLookup`] so `check_*` functions can consume it
//! generically.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::{DnsLookup, TlsaRecord};

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

    pub fn fail_with(mut self, name: &str) -> Self {
        self.error_names.insert(name.to_string());
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

    async fn maybe_sleep(&self) {
        if let Some(d) = self.delay {
            tokio::time::sleep(d).await;
        }
    }
}

impl DnsLookup for TestDnsResolver {
    fn lookup_txt(&self, name: &str) -> impl Future<Output = Vec<String>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) {
                return Vec::new();
            }
            self.txt.get(&name).cloned().unwrap_or_default()
        }
    }

    fn lookup_mx(&self, name: &str) -> impl Future<Output = Vec<(u16, String)>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) {
                return Vec::new();
            }
            self.mx.get(&name).cloned().unwrap_or_default()
        }
    }

    fn lookup_ips(&self, name: &str) -> impl Future<Output = Vec<IpAddr>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) {
                return Vec::new();
            }
            self.ips.get(&name).cloned().unwrap_or_default()
        }
    }

    fn lookup_cname(&self, name: &str) -> impl Future<Output = Vec<String>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) {
                return Vec::new();
            }
            self.cname.get(&name).cloned().unwrap_or_default()
        }
    }

    fn lookup_ptr(&self, ip: IpAddr) -> impl Future<Output = Vec<String>> + Send {
        let key = ip.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&key);
            if self.error_names.contains(&key) {
                return Vec::new();
            }
            self.ptr.get(&key).cloned().unwrap_or_default()
        }
    }

    fn lookup_a(&self, name: &str) -> impl Future<Output = Vec<Ipv4Addr>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) {
                return Vec::new();
            }
            self.a.get(&name).cloned().unwrap_or_default()
        }
    }

    fn lookup_tlsa(&self, name: &str) -> impl Future<Output = Vec<TlsaRecord>> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) {
                return Vec::new();
            }
            self.tlsa.get(&name).cloned().unwrap_or_default()
        }
    }

    fn lookup_exists(&self, name: &str) -> impl Future<Output = bool> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            if self.error_names.contains(&name) {
                return false;
            }
            !self.a.get(&name).cloned().unwrap_or_default().is_empty()
                || !self.ips.get(&name).cloned().unwrap_or_default().is_empty()
        }
    }

    fn check_dnssec_signed(&self, name: &str) -> impl Future<Output = bool> + Send {
        let name = name.to_string();
        async move {
            self.maybe_sleep().await;
            self.record_query(&name);
            self.dnssec_signed.contains(&name)
        }
    }
}
