//! What the apex's run machinery and its badge, OG and result routes share. Copied from lens's
//! `state.rs` without the V1 cache, the SPA shell and the snapshot store.

use std::future::Future;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use fontdb::Database;
use governor::{Quota, RateLimiter};
use netray_common::ip_extract::IpExtractor;
use netray_common::rate_limit::KeyedLimiter;
use netray_engine::Registry;
use netray_engine::domain_run::CheckOutput;
use netray_engine::profile::{Profile, Profiles};
use netray_model::Protocol;
use netray_store::HistoryStore;

use super::rate_limit::{GlobalRateLimiter, PerIpRateLimiter};
use super::{ImageBudget, Runs, Throttle};
use crate::api::ApiParts;
use crate::config::ApexConfig;
use crate::metrics::ClientRunCounter;

/// Per-client GCRA budget of fresh runs, keyed by client address.
pub type ClientBudgetLimiter = KeyedLimiter<IpAddr>;

/// Per-domain GCRA rate limiter for badge and OG card recomputes.
pub type BadgeRecomputeLimiter = KeyedLimiter<String>;

/// When set, a badge or OG run calls this instead of the registry: the seam tests inject a
/// result through.
pub type BadgeCheckFn =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = CheckOutput> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct ApexState {
    pub config: Arc<ApexConfig>,
    pub per_ip_limiter: Arc<PerIpRateLimiter>,
    pub global_limiter: Arc<GlobalRateLimiter>,
    /// Resolves the client address from the TCP peer and, when the peer is a trusted proxy,
    /// from the forwarding headers.
    pub ip_extractor: Arc<IpExtractor>,
    pub badge_recompute_limiter: Arc<BadgeRecomputeLimiter>,
    /// The site-wide budget of badge and OG fresh runs.
    pub image_budget: Arc<ImageBudget>,
    pub scoring_profile: Arc<Profile>,
    /// The modules and the resolve stage a run goes through.
    pub registry: Arc<Registry>,
    /// The sections a run starts and their windows, in plan order.
    pub plan: Arc<Vec<(Protocol, Duration)>>,
    /// The budget of the resolve stage, counted from the start of a run.
    pub resolve_budget: Duration,
    /// Inter and JetBrains Mono for the OG card rasterization.
    pub font_db: Arc<Database>,
    pub badge_check_fn: Option<BadgeCheckFn>,
    /// `None` when the history is disabled.
    pub history_store: Option<Arc<HistoryStore>>,
    /// Fresh runs per client for the current hour; flushed hourly into a histogram.
    pub client_runs: Arc<ClientRunCounter>,
    /// The runs in flight, one per domain.
    pub runs: Arc<Runs>,
    /// When each domain's last run started.
    pub throttle: Arc<Throttle>,
    /// One permit per domain run in flight.
    pub slots: Arc<tokio::sync::Semaphore>,
    /// `None` when `client_budget_per_hour` is 0: no client may start a budgeted run.
    pub client_budget: Option<Arc<ClientBudgetLimiter>>,
    /// Module-side parts of API v1, handed in by `crates/netray`.
    pub api_parts: ApiParts,
}

impl ApexState {
    /// The state of `config` over `registry`, running the sections of `plan` with the resolve
    /// stage under `resolve_budget`, scored with `profile`; no history store, no API parts.
    /// Panics on a zero badge TTL or a zero limiter rate; the configuration refuses both.
    pub fn new(
        config: ApexConfig,
        registry: Registry,
        plan: Vec<(Protocol, Duration)>,
        resolve_budget: Duration,
        profile: Profile,
    ) -> Self {
        let limits = config.limits;
        let per_ip_limiter = Arc::new(PerIpRateLimiter::new(
            limits.rate_limit_per_ip_per_minute,
            limits.rate_limit_per_ip_burst,
        ));
        let global_limiter = Arc::new(GlobalRateLimiter::new(
            limits.rate_limit_global_per_minute,
            limits.rate_limit_global_burst,
        ));
        let ip_extractor = Arc::new(IpExtractor::new(&config.trusted_proxies));

        let badge_quota = Quota::with_period(Duration::from_secs(config.badges.ttl_seconds))
            .expect("badge ttl_seconds must be non-zero")
            .allow_burst(NonZeroU32::new(1).unwrap());
        let badge_recompute_limiter = Arc::new(RateLimiter::keyed(badge_quota));

        let image_budget = Arc::new(ImageBudget::new(limits.image_runs_per_minute));
        let slots = Arc::new(tokio::sync::Semaphore::new(
            limits.domain_run_slots as usize,
        ));
        let client_budget = NonZeroU32::new(limits.client_budget_per_hour)
            .map(|n| Arc::new(RateLimiter::keyed(Quota::per_hour(n).allow_burst(n))));

        Self {
            config: Arc::new(config),
            per_ip_limiter,
            global_limiter,
            ip_extractor,
            badge_recompute_limiter,
            image_budget,
            scoring_profile: Arc::new(profile),
            registry: Arc::new(registry),
            plan: Arc::new(plan),
            resolve_budget,
            font_db: crate::og::fonts::init_font_db(),
            badge_check_fn: None,
            history_store: None,
            client_runs: Arc::new(ClientRunCounter::new()),
            runs: Arc::new(Runs::default()),
            throttle: Arc::new(Throttle::default()),
            slots,
            client_budget,
            api_parts: ApiParts::default(),
        }
    }
}

/// The scoring profile at `path`, or the engine's current profile; a profile naming a check
/// that is not in the catalogue is refused.
pub fn load_scoring_profile(path: Option<&str>) -> Result<Profile, Box<dyn std::error::Error>> {
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
                    "[engine] profile_path {p}: check {unknown} is not a catalogue ID"
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

    fn state() -> ApexState {
        ApexState::new(
            ApexConfig::default(),
            Registry::new(),
            Vec::new(),
            Duration::from_millis(1500),
            Profiles::current().clone(),
        )
    }

    #[test]
    fn state_is_clone() {
        let _cloned = state().clone();
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
        let err = load_scoring_profile(Some(&file.path().to_string_lossy()))
            .expect_err("unknown ID must be refused");
        let msg = err.to_string();
        assert!(msg.contains("[engine] profile_path"), "{msg}");
        assert!(msg.contains("dns.no_such_check"), "{msg}");
    }
}
