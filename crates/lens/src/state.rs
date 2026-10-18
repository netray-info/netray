use std::future::Future;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use fontdb::Database;

use governor::{Quota, RateLimiter};
use moka::future::Cache;
use netray_common::ip_extract::IpExtractor;
use netray_common::rate_limit::KeyedLimiter;
use netray_engine::Registry;
use netray_engine::profile::{Profile, Profiles};
use netray_model::Protocol;

use crate::cache::CachedResult;
use crate::check::CheckOutput;
use crate::config::Config;
use crate::modules::ModuleSection;
use crate::security::rate_limit::{GlobalRateLimiter, PerIpRateLimiter};

/// Per-client GCRA budget of fresh runs, keyed by client address.
pub type ClientBudgetLimiter = KeyedLimiter<IpAddr>;

/// Per-domain GCRA rate limiter for badge recomputes.
pub type BadgeRecomputeLimiter = KeyedLimiter<String>;

/// Injectable check function for badge handler. When `Some`, replaces the real `run_check`.
/// Used in tests to inject a mock without backends.
pub type BadgeCheckFn =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = CheckOutput> + Send>> + Send + Sync>;

/// Hook awaited by `/api/check` after its head and cache step and before `admit`.
pub type BeforeAdmit = Arc<dyn Fn() -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

/// Shared application state passed to every axum handler via `axum::extract::State`.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub per_ip_limiter: Arc<PerIpRateLimiter>,
    pub global_limiter: Arc<GlobalRateLimiter>,
    /// Resolves the client IP from the TCP peer and, when the peer is a
    /// trusted proxy, from the forwarding headers.
    pub ip_extractor: Arc<IpExtractor>,
    pub badge_recompute_limiter: Arc<BadgeRecomputeLimiter>,
    /// The V1 holder of the history head's V1 rendering, keyed by entry id; its TTL is the
    /// cache window. Goes with the V1 wire.
    pub cache: Option<Arc<Cache<String, Arc<CachedResult>>>>,
    pub scoring_profile: Arc<Profile>,
    /// The modules and the resolve stage a check runs through.
    pub registry: Arc<Registry>,
    /// The configured sections, one per registered module.
    pub backends: Arc<Vec<ModuleSection>>,
    /// SPA HTML shell with `[site]` placeholders substituted once at startup.
    /// `None` when the embedded `frontend/dist` has no `index.html` (test
    /// builds with a `.gitkeep`-only dist).
    pub rendered_html: Option<Arc<String>>,
    /// Embedded Inter font database for OG card PNG rasterization.
    pub font_db: Arc<Database>,
    /// When `Some`, badge_handler calls this instead of the real `run_check`.
    pub badge_check_fn: Option<BadgeCheckFn>,
    /// Snapshot storage. `None` when snapshots are disabled.
    pub snapshot_store: Option<Arc<crate::snapshot::SnapshotStore>>,
    /// History storage. `None` when history is disabled.
    pub history_store: Option<Arc<netray_store::HistoryStore>>,
    /// Fresh runs per client for the current hour; flushed hourly into a histogram.
    pub client_runs: Arc<crate::metrics::ClientRunCounter>,
    /// The runs in flight, one per domain.
    pub runs: Arc<crate::admission::Runs>,
    /// When each domain's last run started.
    pub throttle: Arc<crate::admission::Throttle>,
    /// One permit per domain run in flight.
    pub slots: Arc<tokio::sync::Semaphore>,
    /// `None` when `client_budget_per_hour` is 0: no client may start a budgeted run.
    pub client_budget: Option<Arc<ClientBudgetLimiter>>,
    /// Module-side parts of API v1, handed in by `crates/netray`.
    pub api_parts: netray_web::api::ApiParts,
    /// Awaited by `/api/check` after its head and cache step and before `admit`.
    pub before_admit: Option<BeforeAdmit>,
}

impl AppState {
    /// Build `AppState` from a validated `Config` and no protocol modules.
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_registry(config, Registry::new())
    }

    /// Build `AppState` from a validated `Config`; the HTTP and email sections run the
    /// registry's modules when `[backends.http]` and `[backends.email]` are configured, the DNS,
    /// IP and TLS sections when the registry has those modules.
    pub fn with_registry(
        config: Config,
        registry: Registry,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let registry = Arc::new(registry);
        let per_ip_limiter = Arc::new(PerIpRateLimiter::new(&config.rate_limit));
        let global_limiter = Arc::new(GlobalRateLimiter::new(&config.rate_limit));
        let ip_extractor = Arc::new(IpExtractor::new(&config.server.trusted_proxies));

        let badge_quota = Quota::with_period(Duration::from_secs(config.badges.ttl_seconds))
            .expect("badge ttl_seconds must be non-zero")
            .allow_burst(NonZeroU32::new(1).unwrap());
        let badge_recompute_limiter = Arc::new(RateLimiter::keyed(badge_quota));

        let slots = Arc::new(tokio::sync::Semaphore::new(
            config.admission.domain_run_slots as usize,
        ));
        let client_budget = NonZeroU32::new(config.admission.client_budget_per_hour)
            .map(|n| Arc::new(RateLimiter::keyed(Quota::per_hour(n).allow_burst(n))));

        let cache = if config.cache.enabled {
            let ttl = Duration::from_secs(config.cache.ttl_seconds);
            let c: Cache<String, Arc<CachedResult>> = Cache::builder().time_to_live(ttl).build();
            Some(Arc::new(c))
        } else {
            None
        };

        let scoring_profile = Arc::new(load_scoring_profile(
            config.scoring.profile_path.as_deref(),
        )?);

        let eco = &config.ecosystem;

        let mut backends: Vec<ModuleSection> = Vec::new();
        if registry.module(Protocol::Dns).is_some() {
            backends.push(ModuleSection {
                registry: registry.clone(),
                protocol: Protocol::Dns,
                timeout: Duration::from_millis(config.backends.dns.timeout_ms),
                public_url: eco.dns_base_url.clone().unwrap_or_default(),
            });
        }
        if registry.module(Protocol::Tls).is_some() {
            backends.push(ModuleSection {
                registry: registry.clone(),
                protocol: Protocol::Tls,
                timeout: Duration::from_millis(config.backends.tls.timeout_ms),
                public_url: eco.tls_base_url.clone().unwrap_or_default(),
            });
        }
        if let Some(ref http_cfg) = config.backends.http
            && registry.module(Protocol::Http).is_some()
        {
            backends.push(ModuleSection {
                registry: registry.clone(),
                protocol: Protocol::Http,
                timeout: Duration::from_millis(http_cfg.timeout_ms),
                public_url: eco.http_base_url.clone().unwrap_or_default(),
            });
        }
        if let Some(ref email_cfg) = config.backends.email
            && registry.module(Protocol::Email).is_some()
        {
            backends.push(ModuleSection {
                registry: registry.clone(),
                protocol: Protocol::Email,
                timeout: Duration::from_millis(email_cfg.timeout_ms),
                public_url: eco.email_base_url.clone().unwrap_or_default(),
            });
        }
        if registry.module(Protocol::Ip).is_some() {
            backends.push(ModuleSection {
                registry: registry.clone(),
                protocol: Protocol::Ip,
                timeout: Duration::from_millis(config.backends.ip.timeout_ms),
                public_url: eco.ip_base_url.clone().unwrap_or_default(),
            });
        }

        let rendered_html = crate::spa::render_shell(&config.site);

        let font_db = crate::og::fonts::init_font_db();

        Ok(Self {
            config: Arc::new(config),
            per_ip_limiter,
            global_limiter,
            ip_extractor,
            badge_recompute_limiter,
            cache,
            scoring_profile,
            registry,
            backends: Arc::new(backends),
            rendered_html,
            font_db,
            badge_check_fn: None,
            snapshot_store: None,
            history_store: None,
            client_runs: Arc::new(crate::metrics::ClientRunCounter::new()),
            runs: Arc::new(crate::admission::Runs::default()),
            throttle: Arc::new(crate::admission::Throttle::default()),
            slots,
            client_budget,
            api_parts: netray_web::api::ApiParts::default(),
            before_admit: None,
        })
    }
}

/// Load the scoring profile from a file path, or fall back to the engine's current profile.
fn load_scoring_profile(path: Option<&str>) -> Result<Profile, Box<dyn std::error::Error>> {
    match path {
        Some(p) => {
            let contents = std::fs::read_to_string(p)?;
            let profile = Profile::from_toml(&contents)?;
            let catalogue = netray_engine::catalogue();
            if let Some(unknown) = profile
                .sections
                .values()
                .flat_map(|s| s.checks.keys())
                .find(|id| catalogue.get(id).is_none())
            {
                return Err(format!(
                    "[scoring] profile_path {p}: check {unknown} is not a catalogue ID"
                )
                .into());
            }
            Ok(profile)
        }
        None => Ok(Profiles::current().clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BackendsConfig, BadgesConfig, CacheConfig, EcosystemConfig, RateLimitConfig, ScoringConfig,
        ServerConfig, SiteConfig,
    };
    use crate::modules::Backend;

    fn test_config() -> Config {
        Config {
            server: ServerConfig {
                bind: ([0, 0, 0, 0], 8082).into(),
                metrics_bind: ([127, 0, 0, 1], 8090).into(),
                trusted_proxies: Vec::new(),
            },
            backends: BackendsConfig {
                resolve_timeout_ms: 2000,
                dns: crate::config::BackendConfig::default(),
                tls: crate::config::BackendConfig::default(),
                ip: crate::config::BackendConfig::default(),
                http: None,
                email: None,
            },
            ecosystem: EcosystemConfig::default(),
            telemetry: Default::default(),
            cache: CacheConfig {
                enabled: false,
                ttl_seconds: 300,
            },
            rate_limit: RateLimitConfig {
                per_ip_per_minute: 10,
                per_ip_burst: 3,
                global_per_minute: 100,
                global_burst: 20,
            },
            scoring: ScoringConfig::default(),
            site: SiteConfig::default(),
            badges: BadgesConfig::default(),
            admission: crate::config::AdmissionConfig::default(),
            inspect: Default::default(),
            api: Default::default(),
            og_cards: crate::config::OgCardsConfig::default(),
            snapshots: crate::config::SnapshotsConfig::default(),
            history: crate::config::HistoryConfig::default(),
            modules: Default::default(),
        }
    }

    #[test]
    fn builds_state_from_config() {
        let config = test_config();
        let state = AppState::new(config).unwrap();
        assert!(
            state.cache.is_none(),
            "cache should be disabled in test config"
        );
    }

    #[test]
    fn state_is_clone() {
        let config = test_config();
        let state = AppState::new(config).unwrap();
        let _cloned = state.clone();
    }

    #[test]
    fn v1_holder_built_with_the_window_as_ttl() {
        // Follows v2-admission Phase 3: moka only holds the V1 rendering, for the window.
        let mut config = test_config();
        config.cache.enabled = true;
        config.cache.ttl_seconds = 60;
        let state = AppState::new(config).unwrap();
        assert_eq!(
            state.cache.as_ref().unwrap().policy().time_to_live(),
            Some(std::time::Duration::from_secs(60))
        );
    }

    #[test]
    fn embedded_default_profile_loads() {
        let profile = load_scoring_profile(None);
        assert!(
            profile.is_ok(),
            "embedded default profile must parse: {:?}",
            profile.err()
        );
    }

    #[test]
    fn profile_with_unknown_check_id_is_refused() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(
            &mut file,
            br#"
[meta]
name = "custom"
version = 99

[sections.dns]
weight = 10

[sections.dns.checks]
dnssec = 5
no_such_check = 3

[thresholds]
A = 90
"#,
        )
        .unwrap();
        let mut config = test_config();
        config.scoring.profile_path = Some(file.path().to_string_lossy().into_owned());
        let err = AppState::new(config)
            .err()
            .expect("unknown ID must be refused");
        let msg = err.to_string();
        assert!(msg.contains("[scoring] profile_path"), "{msg}");
        assert!(msg.contains("dns.no_such_check"), "{msg}");
    }

    #[test]
    fn no_section_registered_without_modules() {
        let config = test_config();
        let state = AppState::new(config).unwrap();
        assert!(state.backends.is_empty());
    }

    #[test]
    fn dns_section_registered_when_module_present() {
        let registry = Registry::new().with(netray_dns::testing::golden_module(include_str!(
            "../../../tests/fixtures/contracts/prism.sse"
        )));
        let state = AppState::with_registry(test_config(), registry).unwrap();
        assert_eq!(state.backends.len(), 1);
        assert_eq!(state.backends[0].section(), "dns");
    }

    #[test]
    fn tls_section_registered_when_module_present() {
        let registry = Registry::new().with(netray_tls::testing::golden_module(include_str!(
            "../../../tests/fixtures/contracts/tlsight-inspect.json"
        )));
        let state = AppState::with_registry(test_config(), registry).unwrap();
        assert_eq!(state.backends.len(), 1);
        assert_eq!(state.backends[0].section(), "tls");
    }

    #[test]
    fn ip_section_registered_when_module_present() {
        let registry = Registry::new().with(netray_ip::testing::golden_module(include_str!(
            "../../../tests/fixtures/contracts/ifconfig-json.json"
        )));
        let state = AppState::with_registry(test_config(), registry).unwrap();
        assert_eq!(state.backends.len(), 1);
        assert_eq!(state.backends[0].section(), "ip");
    }

    #[test]
    fn http_section_registered_when_configured_and_module_present() {
        let mut config = test_config();
        config.backends.http = Some(crate::config::BackendConfig::default());
        let registry = Registry::new().with(netray_http::testing::golden_module(include_str!(
            "../../../tests/fixtures/contracts/spectra-inspect.json"
        )));
        let state = AppState::with_registry(config, registry).unwrap();
        assert_eq!(state.backends.len(), 1);
        assert_eq!(state.backends[0].section(), "http");
    }
}
