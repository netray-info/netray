use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

use crate::backend::TlsBackend;
use crate::config::{Config, ModuleConfig};
use crate::error::AppError;
use crate::module::{tls_checks, translate};
use crate::routes::InspectResponse;
use crate::state::AppState;
use crate::tls::probe::ProtocolProbes;
use crate::tls::{InspectionError, IpInspectionResult};

const INSPECT_FIXTURE: &str =
    include_str!("../../../tests/fixtures/contracts/tlsight-inspect.json");

struct GoldenModule(SectionOutcome);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }

    fn checks(&self) -> &'static [CheckId] {
        tls_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.0.clone() })
    }
}

/// A TLS module that answers every run with the translation of `contract_json`, a tlsight
/// inspect response.
pub fn golden_module(contract_json: &str) -> Box<dyn Module> {
    let resp: InspectResponse = serde_json::from_str(contract_json)
        .expect("golden contract decodes as an inspect response");
    Box::new(GoldenModule(translate(&resp)))
}

/// A backend with no network: every name resolves to `93.184.215.14` unless `with_host` says
/// otherwise, and every handshake answers the first address of the tlsight contract fixture.
/// Each handshake is recorded.
pub struct StubTlsBackend {
    hosts: HashMap<String, Vec<IpAddr>>,
    calls: Mutex<Vec<(IpAddr, u16, Option<String>)>>,
    probes: Option<ProtocolProbes>,
    failed: Vec<IpAddr>,
    probe_calls: Mutex<Vec<(IpAddr, u16, String)>>,
}

impl Default for StubTlsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StubTlsBackend {
    pub fn new() -> Self {
        Self {
            hosts: HashMap::new(),
            calls: Mutex::new(Vec::new()),
            probes: None,
            failed: Vec::new(),
            probe_calls: Mutex::new(Vec::new()),
        }
    }

    /// `name` resolves to `ips`; an empty list fails the lookup.
    pub fn with_host(mut self, name: &str, ips: Vec<IpAddr>) -> Self {
        self.hosts.insert(name.to_string(), ips);
        self
    }

    /// Every probe answers `probes`.
    pub fn with_probes(mut self, probes: ProtocolProbes) -> Self {
        self.probes = Some(probes);
        self
    }

    /// The handshake with `ip` fails with `HANDSHAKE_FAILED`.
    pub fn with_failed_handshake(mut self, ip: IpAddr) -> Self {
        self.failed.push(ip);
        self
    }

    /// The probes so far: address, port, negotiated version.
    pub fn probe_calls(&self) -> Vec<(IpAddr, u16, String)> {
        self.probe_calls.lock().expect("probe calls lock").clone()
    }

    /// The handshakes so far: address, port, SNI.
    pub fn calls(&self) -> Vec<(IpAddr, u16, Option<String>)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl TlsBackend for StubTlsBackend {
    fn resolve<'a>(&'a self, hostname: &'a str) -> BoxFuture<'a, Result<Vec<IpAddr>, AppError>> {
        Box::pin(async move {
            match self.hosts.get(hostname) {
                Some(ips) if ips.is_empty() => Err(AppError::DnsResolutionFailed(format!(
                    "{hostname}: no stub answer"
                ))),
                Some(ips) => Ok(ips.clone()),
                None => Ok(vec![IpAddr::from([93, 184, 215, 14])]),
            }
        })
    }

    fn handshake<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        sni: Option<&'a str>,
        _timeout: Duration,
    ) -> BoxFuture<'a, IpInspectionResult> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls lock")
                .push((ip, port, sni.map(str::to_string)));
            if self.failed.contains(&ip) {
                return IpInspectionResult {
                    ip: ip.to_string(),
                    ip_version: if ip.is_ipv4() { "v4" } else { "v6" }.to_string(),
                    tls: None,
                    chain: None,
                    validation: None,
                    ct: None,
                    enrichment: None,
                    raw_certs: None,
                    probes: None,
                    error: Some(InspectionError {
                        code: "HANDSHAKE_FAILED".into(),
                        message: "connection refused".into(),
                    }),
                };
            }
            let doc: serde_json::Value =
                serde_json::from_str(INSPECT_FIXTURE).expect("fixture is JSON");
            let mut result: IpInspectionResult =
                serde_json::from_value(doc["ports"][0]["ips"][0].clone())
                    .expect("fixture decodes as an address result");
            result.ip = ip.to_string();
            result.ip_version = if ip.is_ipv4() { "v4" } else { "v6" }.to_string();
            if let Some(tls) = result.tls.as_mut() {
                tls.sni = sni.map(str::to_string);
                tls.starttls = None;
            }
            result.validation = None;
            result.ct = None;
            result.enrichment = None;
            result.raw_certs = None;
            result
        })
    }

    fn probe<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        _sni: Option<&'a str>,
        negotiated: &'a str,
        _timeout: Duration,
    ) -> BoxFuture<'a, Option<ProtocolProbes>> {
        Box::pin(async move {
            self.probe_calls.lock().expect("probe calls lock").push((
                ip,
                port,
                negotiated.to_string(),
            ));
            self.probes.clone()
        })
    }
}

/// The default module sections as a service config, without the HTTP probes.
pub fn stub_config() -> Config {
    let mut module: ModuleConfig =
        serde_json::from_str("{}").expect("empty sections take their defaults");
    module.quality.skip_http_checks = true;
    module.into_config().expect("default sections are valid")
}

/// An `AppState` over `backend`, without a DNS resolver.
pub fn stub_state(backend: Arc<StubTlsBackend>) -> AppState {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut state = AppState::new(&stub_config());
    state.backend = backend;
    state
}
