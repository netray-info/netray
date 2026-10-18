//! `ApiState`: what the v1 handlers and middleware share.
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{Quota, RateLimiter};
use netray_common::ip_extract::IpExtractor;
use netray_common::rate_limit::KeyedLimiter;
use netray_dns::DnsJobs;
use netray_email::{EmailInspect, InspectSlots};
use netray_engine::profile::{Profile, Profiles};
use netray_http::HttpInspect;
use netray_ip::IpLookup;
use netray_ip::extractors::header_filters;
use netray_store::HistoryStore;
use netray_tls::TlsInspector;
use regex::RegexSet;
use serde::Serialize;

use super::config::{ApiConfig, ApiIpConfig};
use super::runs::DomainRuns;

/// The lens values `/meta` reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AdmissionLimits {
    pub client_budget_per_hour: u32,
    pub domain_run_slots: u32,
    pub rate_limit_per_ip_per_minute: u32,
    pub rate_limit_per_ip_burst: u32,
    pub rate_limit_global_per_minute: u32,
    pub rate_limit_global_burst: u32,
}

/// Module-side parts handed in by `crates/netray`.
#[derive(Clone, Default)]
pub struct ApiParts {
    pub dns: Option<DnsJobs>,
    pub ip: Option<Arc<dyn IpLookup>>,
    pub tls: Option<TlsInspector>,
    pub http: Option<Arc<dyn HttpInspect>>,
    pub email: Option<Arc<dyn EmailInspect>>,
}

/// The three limiters of the ip endpoints, keyed by the caller address (per-target by the
/// looked-up address), built from `[api.ip]`.
pub struct IpLimiters {
    pub per_ip: KeyedLimiter<IpAddr>,
    pub per_target: KeyedLimiter<IpAddr>,
    pub batch_addresses: KeyedLimiter<IpAddr>,
    /// When the three maps were last pruned; the per-target key is an address the caller picks,
    /// so a map that is never pruned grows without bound.
    last_prune: std::sync::Mutex<std::time::Instant>,
}

/// How often the IP limiters drop keys that have refilled, as ifconfig prunes its own.
const PRUNE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

fn keyed(per_minute: u32, burst: u32) -> KeyedLimiter<IpAddr> {
    let per_minute = NonZeroU32::new(per_minute).unwrap_or(NonZeroU32::MIN);
    let burst = NonZeroU32::new(burst).unwrap_or(NonZeroU32::MIN);
    RateLimiter::keyed(Quota::per_minute(per_minute).allow_burst(burst))
}

impl IpLimiters {
    fn new(config: &ApiIpConfig) -> Self {
        Self {
            per_ip: keyed(config.per_ip_per_minute, config.per_ip_burst),
            per_target: keyed(config.per_target_per_minute, config.per_target_burst),
            batch_addresses: keyed(
                config.batch_addresses_per_minute,
                config.batch_addresses_burst,
            ),
            last_prune: std::sync::Mutex::new(std::time::Instant::now()),
        }
    }

    /// Drops the keys of all three limiters that have refilled, at most once per minute.
    pub fn prune_if_due(&self) {
        let Ok(mut last) = self.last_prune.try_lock() else {
            return;
        };
        if last.elapsed() < PRUNE_EVERY {
            return;
        }
        *last = std::time::Instant::now();
        for limiter in [&self.per_ip, &self.per_target, &self.batch_addresses] {
            limiter.retain_recent();
            limiter.shrink_to_fit();
        }
    }
}

#[derive(Clone)]
pub struct ApiState {
    config: ApiConfig,
    runs: Arc<dyn DomainRuns>,
    store: Option<Arc<HistoryStore>>,
    ip_extractor: Arc<IpExtractor>,
    admission: AdmissionLimits,
    parts: ApiParts,
    profile: Option<Arc<Profile>>,
    limiter: Arc<KeyedLimiter<IpAddr>>,
    ip_limiters: Arc<IpLimiters>,
    tls_limiters: Arc<netray_tls::security::RateLimitState>,
    http_limiters: Arc<netray_http::security::RateLimitState>,
    email_limiter: Arc<netray_email::security::RateLimitState>,
    email_slots: InspectSlots,
    dns_limiter: Arc<netray_dns::security::RateLimitState>,
    header_filters: Arc<RegexSet>,
}

impl ApiState {
    pub fn new(config: ApiConfig, runs: Arc<dyn DomainRuns>) -> Self {
        let per_minute = NonZeroU32::new(config.per_ip_per_minute).unwrap_or(NonZeroU32::MIN);
        let burst = NonZeroU32::new(config.per_ip_burst).unwrap_or(NonZeroU32::MIN);
        let limiter = RateLimiter::keyed(Quota::per_minute(per_minute).allow_burst(burst));
        let ip_limiters = Arc::new(IpLimiters::new(&config.ip));
        let tls_limiters = Arc::new(netray_tls::security::RateLimitState::scoped(
            &netray_tls::security::rate_limit::Quotas {
                per_ip_per_minute: config.tls.per_ip_per_minute,
                per_ip_burst: config.tls.per_ip_burst,
                per_target_per_minute: config.tls.per_target_per_minute,
                per_target_burst: config.tls.per_target_burst,
            },
            netray_tls::security::rate_limit::Scopes {
                metrics_prefix: "lens",
                per_ip: "api_v1_tls",
                per_target: "api_v1_tls_target",
            },
        ));
        let http_limiters = Arc::new(netray_http::security::RateLimitState::scoped(
            &netray_http::security::rate_limit::Quotas {
                per_ip_per_minute: config.http.per_ip_per_minute,
                per_ip_burst: config.http.per_ip_burst,
                per_target_per_minute: config.http.per_target_per_minute,
                per_target_burst: config.http.per_target_burst,
            },
            netray_http::security::rate_limit::Scopes {
                metrics_prefix: "lens",
                per_ip: "api_v1_http",
                per_target: "api_v1_http_target",
            },
        ));
        let email_limiter = Arc::new(netray_email::security::RateLimitState::scoped(
            &netray_email::security::rate_limit::Quotas {
                per_ip_per_minute: config.email.per_ip_per_minute,
                per_ip_burst: config.email.per_ip_burst,
            },
            netray_email::security::rate_limit::Scopes {
                metrics_prefix: "lens",
                per_ip: "api_v1_email",
            },
        ));
        let email_slots = InspectSlots::new(config.email.max_concurrent_inspections as usize);
        let dns_limiter = Arc::new(netray_dns::security::RateLimitState::scoped(
            &netray_dns::security::rate_limit::Quotas {
                per_ip_per_minute: config.dns.per_ip_per_minute,
                per_ip_burst: config.dns.per_ip_burst,
                per_target_per_minute: config.dns.per_target_per_minute,
                per_target_burst: config.dns.per_target_burst,
                global_per_minute: config.dns.global_per_minute,
                global_burst: config.dns.global_burst,
                per_ip_max_streams: config.dns.per_ip_max_streams,
            },
            netray_dns::security::rate_limit::Scopes {
                metrics_prefix: "lens",
                per_ip: "api_v1_dns",
                per_target: "api_v1_dns_target",
                global: "api_v1_dns_global",
                max_streams: "api_v1_dns_streams",
            },
        ));
        let header_filters = Arc::new(header_filters(&config.ip.filtered_headers));
        Self {
            config,
            runs,
            store: None,
            ip_extractor: Arc::new(IpExtractor::new(&[])),
            admission: AdmissionLimits::default(),
            parts: ApiParts::default(),
            profile: None,
            limiter: Arc::new(limiter),
            ip_limiters,
            tls_limiters,
            http_limiters,
            email_limiter,
            email_slots,
            dns_limiter,
            header_filters,
        }
    }

    pub fn with_store(mut self, store: Option<Arc<HistoryStore>>) -> Self {
        self.store = store;
        self
    }

    pub fn with_ip_extractor(mut self, extractor: Arc<IpExtractor>) -> Self {
        self.ip_extractor = extractor;
        self
    }

    pub fn with_admission_limits(mut self, limits: AdmissionLimits) -> Self {
        self.admission = limits;
        self
    }

    pub fn with_parts(mut self, parts: ApiParts) -> Self {
        self.parts = parts;
        self
    }

    /// The profile lens grades with; `/meta` and `/profiles/{n}` report it.
    pub fn with_profile(mut self, profile: Arc<Profile>) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn config(&self) -> &ApiConfig {
        &self.config
    }

    pub fn runs(&self) -> &Arc<dyn DomainRuns> {
        &self.runs
    }

    pub fn store(&self) -> Option<&Arc<HistoryStore>> {
        self.store.as_ref()
    }

    pub fn ip_extractor(&self) -> &IpExtractor {
        &self.ip_extractor
    }

    pub fn admission(&self) -> &AdmissionLimits {
        &self.admission
    }

    pub fn parts(&self) -> &ApiParts {
        &self.parts
    }

    pub fn ip_limiters(&self) -> &Arc<IpLimiters> {
        &self.ip_limiters
    }

    pub fn ip_lookup(&self) -> Option<&Arc<dyn IpLookup>> {
        self.parts.ip.as_ref()
    }

    pub fn tls_limiters(&self) -> &Arc<netray_tls::security::RateLimitState> {
        &self.tls_limiters
    }

    pub fn http_limiters(&self) -> &Arc<netray_http::security::RateLimitState> {
        &self.http_limiters
    }

    pub fn email_limiter(&self) -> &Arc<netray_email::security::RateLimitState> {
        &self.email_limiter
    }

    pub fn email_slots(&self) -> &InspectSlots {
        &self.email_slots
    }

    pub fn dns_limiter(&self) -> &Arc<netray_dns::security::RateLimitState> {
        &self.dns_limiter
    }

    pub fn dns_jobs(&self) -> Option<&DnsJobs> {
        self.parts.dns.as_ref()
    }

    pub fn tls_inspector(&self) -> Option<&TlsInspector> {
        self.parts.tls.as_ref()
    }

    pub fn http_inspect(&self) -> Option<&Arc<dyn HttpInspect>> {
        self.parts.http.as_ref()
    }

    pub fn email_inspect(&self) -> Option<&Arc<dyn EmailInspect>> {
        self.parts.email.as_ref()
    }

    pub(super) fn header_filters(&self) -> &RegexSet {
        &self.header_filters
    }

    pub(super) fn current_profile(&self) -> u32 {
        self.profile
            .as_ref()
            .map_or_else(|| Profiles::current().version, |p| p.version)
    }

    pub(super) fn profile_versions(&self) -> Vec<u32> {
        let mut versions = Profiles::versions();
        versions.extend(self.profile.as_ref().map(|p| p.version));
        versions.sort_unstable();
        versions.dedup();
        versions
    }

    pub(super) fn profile(&self, version: u32) -> Option<&Profile> {
        self.profile
            .as_deref()
            .filter(|p| p.version == version)
            .or_else(|| Profiles::get(version))
    }

    pub(super) fn limiter(&self) -> &KeyedLimiter<IpAddr> {
        &self.limiter
    }
}
