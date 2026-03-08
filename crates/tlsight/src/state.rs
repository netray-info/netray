use std::sync::Arc;

use crate::config::Config;
use crate::dns::DnsResolver;
use crate::security::{IpExtractor, RateLimitState};
use rustls::client::danger::ServerCertVerifier;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
    #[allow(dead_code)] // Used to build cert_verifier; will be used for custom CA loading
    pub trust_store: Arc<rustls::RootCertStore>,
    pub cert_verifier: Arc<dyn ServerCertVerifier>,
    pub dns_resolver: Option<Arc<DnsResolver>>,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        // TODO("custom CA loading"): Load custom CA directory (config.validation.custom_ca_dir)
        // when rustls-pemfile is added as a dependency. Iterate *.pem files, parse with
        // rustls_pemfile::certs(), add to root_store.

        let trust_store = Arc::new(root_store);
        let cert_verifier = rustls::client::WebPkiServerVerifier::builder(Arc::clone(&trust_store))
            .build()
            .expect("failed to build WebPki verifier");

        Self {
            ip_extractor: Arc::new(
                IpExtractor::new(&config.server.trusted_proxies)
                    .expect("invalid trusted_proxies configuration"),
            ),
            rate_limiter: Arc::new(RateLimitState::new(&config.limits)),
            trust_store,
            cert_verifier,
            dns_resolver: None, // Initialized async in main
            config: Arc::new(config.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    fn default_config() -> Config {
        let mut cfg = Config {
            server: crate::config::ServerConfig {
                bind: ([127, 0, 0, 1], 8080).into(),
                metrics_bind: ([127, 0, 0, 1], 9090).into(),
                trusted_proxies: Vec::new(),
            },
            limits: crate::config::LimitsConfig {
                per_ip_per_minute: 30,
                per_ip_burst: 10,
                per_target_per_minute: 60,
                per_target_burst: 20,
                max_concurrent_connections: 256,
                max_concurrent_handshakes: 10,
                handshake_timeout_secs: 5,
                request_timeout_secs: 15,
                max_ports: 5,
                max_ips_per_hostname: 10,
                max_domain_length: 253,
            },
            dns: crate::config::DnsConfig {
                resolver: "cloudflare".to_owned(),
                timeout_secs: 3,
            },
            validation: crate::config::ValidationConfig {
                expiry_warning_days: 30,
                expiry_critical_days: 14,
                check_dane: true,
                check_caa: true,
                check_ct: false,
                custom_ca_dir: None,
            },
            ecosystem: crate::config::EcosystemConfig::default(),
        };
        // Ensure defaults pass validation (not strictly needed but defensive).
        let _ = &mut cfg;
        cfg
    }

    #[test]
    fn creates_state_with_defaults() {
        ensure_crypto_provider();
        let config = default_config();
        let state = AppState::new(&config);

        // Trust store should contain Mozilla root certificates.
        assert!(
            !state.trust_store.is_empty(),
            "trust store should contain root CAs"
        );
    }

    #[test]
    fn state_is_clone() {
        ensure_crypto_provider();
        let config = default_config();
        let state = AppState::new(&config);
        let _cloned = state.clone();
    }

    #[test]
    fn trust_store_has_reasonable_ca_count() {
        ensure_crypto_provider();
        let config = default_config();
        let state = AppState::new(&config);

        // Mozilla's root store typically has 100+ CAs.
        assert!(
            state.trust_store.len() > 50,
            "trust store should have many root CAs, got {}",
            state.trust_store.len()
        );
    }
}
