//! spec v2-api, requirement 1: `LensRuns` implements `DomainRuns`; `run_with` takes `ApiParts`.
use lens::api_v1::LensRuns;
use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_web::api::{ApiParts, DomainRuns};

fn base_config() -> Config {
    Config {
        modules: Default::default(),
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            resolve_timeout_ms: 2000,
            dns: lens::config::BackendConfig {
                timeout_ms: 100,
                ..Default::default()
            },
            tls: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            ip: lens::config::BackendConfig {
                timeout_ms: 100,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: false,
            ttl_seconds: 300,
        },
        telemetry: Default::default(),
        rate_limit: RateLimitConfig {
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            global_per_minute: 1000,
            global_burst: 100,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig::default(),
        og_cards: OgCardsConfig::default(),
        snapshots: SnapshotsConfig::default(),
        history: lens::config::HistoryConfig::default(),
        admission: lens::config::AdmissionConfig::default(),
        api: Default::default(),
    }
}

#[test]
fn lens_runs_validates_domains() {
    let runs = LensRuns::new(AppState::new(base_config()).unwrap());
    let runs: &dyn DomainRuns = &runs;
    assert_eq!(runs.validate("Example.COM."), Ok("example.com".to_string()));
    let refusal = runs.validate("*.example.com").unwrap_err();
    assert_eq!(refusal.code, "DOMAIN_INVALID");
}

#[test]
fn run_with_takes_api_parts() {
    // Compile-time only: the closure is never called.
    let _never = |registry: netray_engine::Registry| {
        let _fut = lens::run_with(None, registry, ApiParts::default());
    };
}
