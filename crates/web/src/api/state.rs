//! `ApiState`: what the v1 handlers and middleware share.
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{Quota, RateLimiter};
use netray_common::ip_extract::IpExtractor;
use netray_common::rate_limit::KeyedLimiter;
use netray_engine::profile::{Profile, Profiles};
use netray_store::HistoryStore;
use serde::Serialize;

use super::config::ApiConfig;
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
pub struct ApiParts {}

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
}

impl ApiState {
    pub fn new(config: ApiConfig, runs: Arc<dyn DomainRuns>) -> Self {
        let per_minute = NonZeroU32::new(config.per_ip_per_minute).unwrap_or(NonZeroU32::MIN);
        let burst = NonZeroU32::new(config.per_ip_burst).unwrap_or(NonZeroU32::MIN);
        let limiter = RateLimiter::keyed(Quota::per_minute(per_minute).allow_burst(burst));
        Self {
            config,
            runs,
            store: None,
            ip_extractor: Arc::new(IpExtractor::new(&[])),
            admission: AdmissionLimits::default(),
            parts: ApiParts::default(),
            profile: None,
            limiter: Arc::new(limiter),
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
