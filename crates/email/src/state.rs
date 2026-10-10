use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::config::Config;
use crate::dns::{DnsResolver, FetchResolver};
use crate::security::{IpExtractor, RateLimitState};
use netray_common::enrichment::{EnrichmentClient, EnrichmentMode};
use netray_common::fetch::{ClientSettings, Resolve};
use netray_common::target_policy::is_allowed_target;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
    pub dns_resolver: Arc<DnsResolver>,
    pub dnsbl_resolver: Arc<DnsResolver>,
    pub fetch: OutboundFetch,
    pub enrichment_client: Option<Arc<EnrichmentClient>>,
    pub inspect_semaphore: Arc<Semaphore>,
}

/// Settings for the MTA-STS and BIMI fetches.
#[derive(Clone)]
pub struct OutboundFetch {
    pub settings: ClientSettings,
    pub resolver: Arc<dyn Resolve>,
    pub allow: fn(IpAddr) -> bool,
    pub timeout: Duration,
}

impl OutboundFetch {
    pub fn new(timeout_ms: u64, resolver: Arc<dyn Resolve>) -> Self {
        Self {
            settings: ClientSettings {
                user_agent: Some(format!(
                    "beacon/{} (netray.info)",
                    env!("CARGO_PKG_VERSION")
                )),
                ..ClientSettings::default()
            },
            resolver,
            allow: is_allowed_target,
            timeout: Duration::from_millis(timeout_ms),
        }
    }
}

impl AppState {
    pub async fn new(config: &Config) -> Result<Self, crate::error::MailError> {
        let dns_resolver = DnsResolver::new(&config.dns.resolvers, config.dns.timeout_ms)
            .await
            .map_err(|e| crate::error::MailError::DnsError(e.to_string()))?;
        let dns_resolver = Arc::new(dns_resolver);

        let dnsbl_resolver = DnsResolver::new(&config.dnsbl.resolvers, config.dnsbl.timeout_ms)
            .await
            .map_err(|e| crate::error::MailError::DnsError(e.to_string()))?;

        let enrichment_client = if config.backends.ip_url.is_empty() {
            None
        } else {
            Some(Arc::new(EnrichmentClient::new(
                &config.backends.ip_url,
                Duration::from_millis(config.backends.timeout_ms),
                "beacon",
                None,
                EnrichmentMode::Backend { cache_ttl_secs: 0 },
            )))
        };

        let fetch = OutboundFetch::new(
            config.http.timeout_ms,
            Arc::new(FetchResolver(dns_resolver.clone())),
        );

        let rate_limiter =
            RateLimitState::new(&config.rate_limit).map_err(crate::error::MailError::Config)?;

        Ok(Self {
            ip_extractor: Arc::new(IpExtractor::new(&config.server.trusted_proxies)),
            rate_limiter: Arc::new(rate_limiter),
            dns_resolver,
            dnsbl_resolver: Arc::new(dnsbl_resolver),
            fetch,
            enrichment_client,
            inspect_semaphore: Arc::new(Semaphore::new(config.server.max_concurrent_inspections)),
            config: Arc::new(config.clone()),
        })
    }

    /// Test-only constructor that accepts pre-built resolvers and skips the
    /// network-side initialisation performed by [`AppState::new`].
    ///
    /// SDD §4 item I3. `AppState` holds a concrete [`DnsResolver`] because
    /// Axum state erasure makes generic state painful; check-level tests that
    /// need to avoid real DNS use [`crate::dns::DnsLookup`] directly via
    /// `run_all_checks::<TestDnsResolver>(...)` rather than going through
    /// [`AppState`].
    #[cfg(test)]
    pub fn with_overrides(
        config: Config,
        dns_resolver: DnsResolver,
        dnsbl_resolver: DnsResolver,
    ) -> Self {
        let rate_limiter = RateLimitState::new(&config.rate_limit)
            .expect("test config must have a valid rate-limit string");
        let dns_resolver = Arc::new(dns_resolver);
        let fetch = OutboundFetch::new(
            config.http.timeout_ms,
            Arc::new(FetchResolver(dns_resolver.clone())),
        );
        Self {
            ip_extractor: Arc::new(IpExtractor::new(&[])),
            rate_limiter: Arc::new(rate_limiter),
            dns_resolver,
            dnsbl_resolver: Arc::new(dnsbl_resolver),
            fetch,
            enrichment_client: None,
            inspect_semaphore: Arc::new(Semaphore::new(16)),
            config: Arc::new(config),
        }
    }
}
