//! The module registry both `netray serve` and the V1 `netray lens` build from their
//! `[modules.*]` tables: the five modules, the in-process enrichment client the ip module hands
//! the other four, and the parts `/api/v1` calls.

use std::collections::BTreeMap;
use std::sync::Arc;

use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, SectionOutcome,
};
use netray_model::{CheckId, Protocol};

/// The `[modules.*]` tables, by module name.
pub type ModuleTables = BTreeMap<String, toml::Table>;

/// A module behind a shared handle: the IP module is shared by the registry and the SIGHUP
/// task, the DNS module by the registry's module and its resolve stage.
struct Shared<T>(Arc<T>);

impl<T: Module> Module for Shared<T> {
    fn protocol(&self) -> Protocol {
        self.0.protocol()
    }

    fn checks(&self) -> &'static [CheckId] {
        self.0.checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        self.0.volatile()
    }

    fn needs_addresses(&self) -> bool {
        self.0.needs_addresses()
    }

    fn wants_facts(&self) -> bool {
        self.0.wants_facts()
    }

    fn needs_address_records(&self) -> bool {
        self.0.needs_address_records()
    }

    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        self.0.run(ctx, facts)
    }
}

impl<T: FactsProvider> FactsProvider for Shared<T> {
    fn resolve<'a>(
        &'a self,
        ctx: &'a RunContext,
        domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        self.0.resolve(ctx, domain)
    }
}

/// The IP module `[modules.ip]` names: `None` when the table is absent (with a warning) or when
/// `startup` is false, where the table is parsed and its keys checked but the data files are
/// neither checked nor read.
async fn ip_module(
    modules: &ModuleTables,
    startup: bool,
) -> Result<Option<Arc<netray_ip::IpModule>>, String> {
    let Some(table) = modules.get("ip").cloned() else {
        tracing::warn!("modules.ip is not configured: the IP section is off");
        return Ok(None);
    };
    let ip_config: netray_ip::ModuleConfig =
        table.try_into().map_err(|e| format!("modules.ip: {e}"))?;
    if ip_config.geoip_city_db.is_none() || ip_config.geoip_asn_db.is_none() {
        return Err(
            "modules.ip: geoip_city_db and geoip_asn_db are required when [modules.ip] is set"
                .into(),
        );
    }
    if !startup {
        return Ok(None);
    }
    for (key, path) in [
        ("geoip_city_db", &ip_config.geoip_city_db),
        ("geoip_asn_db", &ip_config.geoip_asn_db),
    ] {
        if let Some(path) = path
            && !std::path::Path::new(path).is_file()
        {
            return Err(format!("modules.ip: {key}: no file at {path}"));
        }
    }
    let ip = netray_ip::IpModule::new(ip_config)
        .await
        .map_err(|e| format!("modules.ip: {e}"))?;
    Ok(Some(Arc::new(ip)))
}

/// The registry and the modules the API and the data reload share.
pub struct Modules {
    pub registry: Registry,
    pub ip: Option<Arc<netray_ip::IpModule>>,
    tls: Arc<netray_tls::TlsModule>,
    http: Arc<netray_http::HttpModule>,
    email: Arc<netray_email::EmailModule>,
    dns: Arc<netray_dns::DnsModule>,
    dns_band: Arc<netray_dns::DnsModule>,
}

impl Modules {
    /// The registry, the IP module for its data reload, and the parts `/api/v1` calls.
    pub fn split(
        self,
    ) -> (
        Registry,
        Option<Arc<netray_ip::IpModule>>,
        netray_web::api::ApiParts,
    ) {
        let parts = netray_web::api::ApiParts {
            ip: self
                .ip
                .clone()
                .map(|module| module as Arc<dyn netray_ip::IpLookup>),
            tls: Some(self.tls.inspector()),
            http: Some(self.http as Arc<dyn netray_http::HttpInspect>),
            email: Some(Arc::new(self.email.inspector()) as Arc<dyn netray_email::EmailInspect>),
            dns: Some(self.dns.jobs()),
            dns_module: Some(self.dns_band as Arc<dyn netray_engine::Module>),
        };
        (self.registry, self.ip, parts)
    }
}

/// Builds the modules `[modules.*]` names; a table for a module the binary does not know is
/// refused, and an absent `[modules.http]`, `[modules.email]`, `[modules.tls]` or
/// `[modules.dns]` builds the module on its defaults. The IP module needs data: an absent
/// `[modules.ip]` leaves the IP section off (with a warning), a present one must name
/// `geoip_city_db` and `geoip_asn_db`. With `startup` false (`--check-config`) the IP table is
/// parsed and its keys checked, but the data files are neither checked nor read and no IP module
/// is built.
pub async fn module_registry(modules: &ModuleTables, startup: bool) -> Result<Modules, String> {
    if let Some(name) = modules
        .keys()
        .find(|k| !matches!(k.as_str(), "http" | "email" | "ip" | "tls" | "dns"))
    {
        return Err(format!("modules.{name}: unknown module"));
    }
    let ip = ip_module(modules, startup).await?;
    with_ip(modules, ip).await
}

/// The modules of `modules` around `ip`: each of dns, tls, http and email gets the in-process
/// enrichment client over `ip`, none without it.
async fn with_ip(
    modules: &ModuleTables,
    ip: Option<Arc<netray_ip::IpModule>>,
) -> Result<Modules, String> {
    let enrichment = netray_ip::enrichment_client(ip.as_ref());
    let table = modules.get("http").cloned().unwrap_or_default();
    let module_config: netray_http::ModuleConfig =
        table.try_into().map_err(|e| format!("modules.http: {e}"))?;
    let http = Arc::new(
        netray_http::HttpModule::new(module_config, enrichment.clone())
            .map_err(|e| format!("modules.http: {e}"))?,
    );
    let table = modules.get("email").cloned().unwrap_or_default();
    let email_config: netray_email::ModuleConfig = table
        .try_into()
        .map_err(|e| format!("modules.email: {e}"))?;
    let email = Arc::new(
        netray_email::EmailModule::new(email_config, enrichment.clone())
            .await
            .map_err(|e| format!("modules.email: {e}"))?,
    );
    let table = match modules.get("tls") {
        Some(table) => table.clone(),
        None => {
            tracing::warn!(
                "modules.tls is not configured: TLS runs on tlsight's defaults (certificate transparency off)"
            );
            Default::default()
        }
    };
    let tls_config: netray_tls::ModuleConfig =
        table.try_into().map_err(|e| format!("modules.tls: {e}"))?;
    let tls = Arc::new(
        netray_tls::TlsModule::new(tls_config, enrichment.clone())
            .await
            .map_err(|e| format!("modules.tls: {e}"))?,
    );
    let table = match modules.get("dns") {
        Some(table) => table.clone(),
        None => {
            tracing::warn!("modules.dns is not configured: DNS runs on prism's defaults");
            Default::default()
        }
    };
    let dns_config: netray_dns::ModuleConfig =
        table.try_into().map_err(|e| format!("modules.dns: {e}"))?;
    let dns_band = Arc::new(
        netray_dns::DnsModule::new(dns_config.clone(), enrichment.clone())
            .await
            .map_err(|e| format!("modules.dns: {e}"))?
            .admitted_by_caller(),
    );
    let dns = Arc::new(
        netray_dns::DnsModule::new(dns_config, enrichment)
            .await
            .map_err(|e| format!("modules.dns: {e}"))?,
    );
    let mut registry = Registry::new()
        .with(Box::new(Shared(dns.clone())))
        .with_facts(Box::new(Shared(dns.clone())))
        .with(Box::new(Shared(http.clone())))
        .with(Box::new(Shared(email.clone())))
        .with(Box::new(Shared(tls.clone())));
    if let Some(ip) = &ip {
        registry = registry.with(Box::new(Shared(ip.clone())));
    }
    Ok(Modules {
        registry,
        ip,
        tls,
        http,
        email,
        dns,
        dns_band,
    })
}

/// Reloads the IP module's data on every SIGHUP.
pub fn reload_ip_on_sighup(ip: Arc<netray_ip::IpModule>) {
    tokio::spawn(async move {
        let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("failed to register the SIGHUP handler");
        while sig.recv().await.is_some() {
            ip.reload().await;
        }
    });
}

/// v2-cutover C7: the composition root's registry hands the in-process enrichment client to
/// dns, tls, http and email when the ip module is registered, and to none without it.
#[cfg(test)]
mod registry_wiring {
    use super::*;

    const IFCONFIG_JSON: &str =
        include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");

    fn enriched(m: &Modules) -> [(&'static str, bool); 4] {
        [
            ("dns", m.dns.has_enrichment()),
            ("dns band", m.dns_band.has_enrichment()),
            ("tls", m.tls.has_enrichment()),
            ("http", m.http.has_enrichment()),
        ]
    }

    #[tokio::test]
    async fn the_ip_module_hands_every_module_the_enrichment_client() {
        let ip = netray_ip::testing::stub_ip_module(IFCONFIG_JSON);
        let m = with_ip(&ModuleTables::new(), Some(ip)).await.unwrap();
        for (module, has) in enriched(&m) {
            assert!(
                has,
                "{module} holds no enrichment client with the ip module registered"
            );
        }
        assert!(m.email.has_enrichment(), "email holds no enrichment client");
        assert!(
            m.registry.module(Protocol::Ip).is_some(),
            "the ip module is registered"
        );
    }

    #[tokio::test]
    async fn without_the_ip_module_no_module_holds_an_enrichment_client() {
        let m = module_registry(&ModuleTables::new(), true).await.unwrap();
        for (module, has) in enriched(&m) {
            assert!(
                !has,
                "{module} holds an enrichment client with no ip module"
            );
        }
        assert!(
            !m.email.has_enrichment(),
            "email holds an enrichment client"
        );
        assert!(m.registry.module(Protocol::Ip).is_none());
    }
}
