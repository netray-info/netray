use std::path::Path;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::backend::{LiveBackend, TlsBackend};
use crate::config::Config;
use crate::dns::DnsResolver;
use netray_common::enrichment::EnrichmentClient;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ArcSwap<Config>>,
    pub trust_store: Arc<ArcSwap<rustls::RootCertStore>>,
    pub handshake_semaphore: Arc<Semaphore>,
    /// Addresses probing at once (each up to seven connections), apart from the handshake
    /// permits so a hanging probe never holds one.
    pub probe_semaphore: Arc<Semaphore>,
    pub hsts_tls_connector: Arc<tokio_rustls::TlsConnector>,
    pub dns_resolver: Option<Arc<DnsResolver>>,
    pub backend: Arc<dyn TlsBackend>,
    pub enrichment_client: Option<Arc<EnrichmentClient>>,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        if let Some(ref ca_dir) = config.validation.custom_ca_dir {
            load_custom_cas(&mut root_store, ca_dir);
        }

        let trust_store_inner = Arc::new(root_store);
        let trust_store = Arc::new(ArcSwap::from(trust_store_inner));

        let hsts_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(crate::tls::verifier::AcceptAnyCert))
            .with_no_client_auth();
        let hsts_tls_connector = Arc::new(tokio_rustls::TlsConnector::from(Arc::new(hsts_config)));

        Self {
            trust_store,
            handshake_semaphore: Arc::new(Semaphore::new(config.limits.max_concurrent_handshakes)),
            probe_semaphore: Arc::new(Semaphore::new(config.limits.max_concurrent_handshakes)),
            hsts_tls_connector,
            dns_resolver: None, // Initialized by `init_dns_resolver`
            backend: Arc::new(LiveBackend { resolver: None }),
            enrichment_client: None,
            config: Arc::new(ArcSwap::from_pointee(config.clone())),
        }
    }

    /// Initializes the DNS resolver, used for A/AAAA resolution and for CAA/DANE lookups; a
    /// failure leaves it unset and hostname inspection failing.
    pub async fn init_dns_resolver(&mut self, config: &Config) {
        match DnsResolver::new(config.dns.timeout_secs).await {
            Ok(resolver) => {
                let resolver = Arc::new(resolver);
                self.backend = Arc::new(LiveBackend {
                    resolver: Some(Arc::clone(&resolver)),
                });
                self.dns_resolver = Some(resolver);
                tracing::info!(
                    check_caa = config.validation.check_caa,
                    check_dane = config.validation.check_dane,
                    "DNS resolver initialized"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to initialize DNS resolver; hostname inspection will fail");
            }
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

        let certs: Vec<_> = CertificateDer::pem_slice_iter(&data)
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
        serde_json::from_str::<crate::config::ModuleConfig>("{}")
            .unwrap()
            .into_config()
            .unwrap()
    }

    #[test]
    fn creates_state_with_defaults() {
        ensure_crypto_provider();
        let config = default_config();
        let state = AppState::new(&config);

        // Trust store should contain Mozilla root certificates.
        assert!(
            !state.trust_store.load().is_empty(),
            "trust store should contain root CAs"
        );
    }

    #[test]
    fn state_is_clone() {
        ensure_crypto_provider();
        let config = default_config();
        let state = AppState::new(&config);
        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.trust_store, &cloned.trust_store));
    }

    #[test]
    fn trust_store_has_reasonable_ca_count() {
        ensure_crypto_provider();
        let config = default_config();
        let state = AppState::new(&config);

        // Mozilla's root store typically has 100+ CAs.
        assert!(
            state.trust_store.load().len() > 50,
            "trust store should have many root CAs, got {}",
            state.trust_store.load().len()
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

    fn ca_pem(name: &str) -> String {
        let key = rcgen::KeyPair::generate().unwrap();
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, name);
        params.self_signed(&key).unwrap().pem()
    }

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tlsight_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// C11: one PEM file holding two distinct CA certificates loads both.
    #[test]
    fn load_custom_cas_multi_cert_pem_loads_every_cert() {
        ensure_crypto_provider();
        let dir = unique_dir("c11");
        let bundle = format!("{}{}", ca_pem("Test CA One"), ca_pem("Test CA Two"));
        std::fs::write(dir.join("bundle.pem"), bundle).unwrap();

        let mut root_store = rustls::RootCertStore::empty();
        load_custom_cas(&mut root_store, dir.to_str().unwrap());
        let loaded = root_store.len();
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(loaded, 2);
    }

    /// C12: a file without any PEM certificate is refused without panic and
    /// does not stop a neighbouring valid file from loading.
    #[test]
    fn load_custom_cas_file_without_cert_is_refused_neighbour_still_loads() {
        ensure_crypto_provider();
        let key_only = rcgen::KeyPair::generate().unwrap().serialize_pem();
        let cases = [
            ("plain text", "this is not a certificate\n".to_string()),
            ("private key only", key_only),
        ];
        for (label, content) in cases {
            let dir = unique_dir("c12");
            std::fs::write(dir.join("bad.pem"), &content).unwrap();

            let mut root_store = rustls::RootCertStore::empty();
            load_custom_cas(&mut root_store, dir.to_str().unwrap());
            assert_eq!(root_store.len(), 0, "{label}: nothing must load");

            std::fs::write(dir.join("good.crt"), ca_pem("Test CA Good")).unwrap();
            let mut root_store = rustls::RootCertStore::empty();
            load_custom_cas(&mut root_store, dir.to_str().unwrap());
            let loaded = root_store.len();
            std::fs::remove_dir_all(&dir).unwrap();
            assert_eq!(loaded, 1, "{label}: neighbouring valid file must load");
        }
    }
}
