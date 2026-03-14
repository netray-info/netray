use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;

use crate::config::Config;
use crate::dns::DnsResolver;
use crate::enrichment::EnrichmentClient;
use crate::security::{IpExtractor, RateLimitState};
use rustls::client::danger::ServerCertVerifier;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
    #[allow(dead_code)] // Kept alive for cert_verifier which holds an Arc reference to it
    pub trust_store: Arc<rustls::RootCertStore>,
    pub cert_verifier: Arc<dyn ServerCertVerifier>,
    pub handshake_semaphore: Arc<Semaphore>,
    pub hsts_tls_connector: Arc<tokio_rustls::TlsConnector>,
    pub dns_resolver: Option<Arc<DnsResolver>>,
    pub enrichment_client: Option<Arc<EnrichmentClient>>,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        if let Some(ref ca_dir) = config.validation.custom_ca_dir {
            load_custom_cas(&mut root_store, ca_dir);
        }

        let trust_store = Arc::new(root_store);
        let cert_verifier = rustls::client::WebPkiServerVerifier::builder(Arc::clone(&trust_store))
            .build()
            .expect("failed to build WebPki verifier");

        let hsts_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(crate::tls::verifier::AcceptAnyCert))
            .with_no_client_auth();
        let hsts_tls_connector = Arc::new(tokio_rustls::TlsConnector::from(Arc::new(hsts_config)));

        let enrichment_client = config.ecosystem.ip_api_url.as_ref().map(|url| {
            Arc::new(EnrichmentClient::new(
                url,
                Duration::from_millis(config.ecosystem.enrichment_timeout_ms),
            ))
        });

        Self {
            ip_extractor: Arc::new(
                IpExtractor::new(&config.server.trusted_proxies)
                    .expect("invalid trusted_proxies configuration"),
            ),
            rate_limiter: Arc::new(RateLimitState::new(&config.limits)),
            trust_store,
            cert_verifier,
            handshake_semaphore: Arc::new(Semaphore::new(config.limits.max_concurrent_handshakes)),
            hsts_tls_connector,
            dns_resolver: None, // Initialized async in main
            enrichment_client,
            config: Arc::new(ArcSwap::from_pointee(config.clone())),
        }
    }
}

/// Load all `*.pem` and `*.crt` files from a directory into the root store.
/// Fails fast on missing directory, logs warnings for unparseable files.
fn load_custom_cas(root_store: &mut rustls::RootCertStore, ca_dir: &str) {
    let dir = Path::new(ca_dir);
    if !dir.is_dir() {
        panic!("custom_ca_dir does not exist or is not a directory: {ca_dir}");
    }

    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read custom_ca_dir {ca_dir}: {e}"));

    let mut loaded = 0u32;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "failed to read directory entry in {ca_dir}");
                continue;
            }
        };
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "pem" && ext != "crt" {
            continue;
        }

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to read CA file");
                continue;
            }
        };

        let certs: Vec<_> = rustls_pemfile::certs(&mut data.as_slice())
            .filter_map(|r| match r {
                Ok(cert) => Some(cert),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to parse PEM cert");
                    None
                }
            })
            .collect();

        if certs.is_empty() {
            tracing::warn!(path = %path.display(), "no certificates found in file");
            continue;
        }

        for cert in certs {
            match root_store.add(cert) {
                Ok(()) => loaded += 1,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to add cert to trust store");
                }
            }
        }
    }

    tracing::info!(
        dir = ca_dir,
        count = loaded,
        "loaded custom CA certificates"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    fn default_config() -> Config {
        let mut cfg = Config {
            site_name: "tlsight".to_string(),
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
                max_ports: 7,
                max_ips_per_hostname: 10,
                max_domain_length: 253,
                allow_blocked_targets: false,
            },
            dns: crate::config::DnsConfig {
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
            quality: crate::config::QualityConfig::default(),
            telemetry: crate::config::TelemetryConfig::default(),
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

    #[test]
    #[should_panic(expected = "does not exist")]
    fn load_custom_cas_panics_on_missing_dir() {
        let mut root_store = rustls::RootCertStore::empty();
        load_custom_cas(&mut root_store, "/nonexistent/directory/that/cannot/exist");
    }

    #[test]
    fn load_custom_cas_empty_dir_loads_nothing() {
        ensure_crypto_provider();
        let dir = std::env::temp_dir().join(format!(
            "tlsight_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let before = root_store.len();
        load_custom_cas(&mut root_store, dir.to_str().unwrap());
        assert_eq!(root_store.len(), before);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
