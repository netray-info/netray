use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, SectionOutcome,
};
use netray_model::{CheckId, Protocol};

mod site;

#[derive(Parser)]
#[command(
    name = "netray",
    version,
    about = "The netray.info suite as one binary"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Domain health checker (netray.info)
    Lens {
        config: Option<String>,
        /// Validate the configuration file at PATH and exit
        #[arg(long, value_name = "PATH")]
        check_config: Option<String>,
    },
    /// DNS inspector (dns.netray.info)
    Dns {
        config: Option<String>,
        /// Validate the configuration file at PATH and exit
        #[arg(long, value_name = "PATH")]
        check_config: Option<String>,
    },
    /// TLS certificate inspector (tls.netray.info)
    Tls {
        config: Option<String>,
        /// Validate the configuration file at PATH and exit
        #[arg(long, value_name = "PATH")]
        check_config: Option<String>,
    },
    /// HTTP header inspector (http.netray.info)
    Http {
        config: Option<String>,
        /// Validate the configuration file at PATH and exit
        #[arg(long, value_name = "PATH")]
        check_config: Option<String>,
    },
    /// Email security inspector (email.netray.info)
    Email {
        config: Option<String>,
        /// Validate the configuration file at PATH and exit
        #[arg(long, value_name = "PATH")]
        check_config: Option<String>,
    },
    /// IP enrichment API (ip.netray.info)
    Ip {
        config: Option<String>,
        /// Validate the configuration file at PATH and exit
        #[arg(long, value_name = "PATH")]
        check_config: Option<String>,
        /// Validate the configuration and exit
        #[arg(long)]
        check: bool,
        /// Print the effective configuration and exit
        #[arg(long)]
        print_config: bool,
    },
    /// Static site (guide, API docs, tools, compare)
    Site {
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: SocketAddr,
        #[arg(long, default_value = "site")]
        root: PathBuf,
    },
}

fn check_config<T, E: std::fmt::Display>(
    path: &str,
    load: impl FnOnce(Option<&str>) -> Result<T, E>,
) -> ! {
    match load(Some(path)) {
        Ok(_) => {
            println!("config ok: {path}");
            std::process::exit(0)
        }
        Err(err) => {
            eprintln!("config error: {path}: {err}");
            std::process::exit(1)
        }
    }
}

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

/// Builds the modules the lens config names in `[modules.*]`; a table for a module the binary
/// does not know is refused, and an absent `[modules.http]`, `[modules.email]`, `[modules.tls]` or
/// `[modules.dns]` builds the module on its defaults. The IP module needs data: an absent `[modules.ip]` leaves the IP section off
/// (with a warning), a present one must name `geoip_city_db` and `geoip_asn_db`. The IP module is
/// returned as well, for its data reload. With `startup` false (`--check-config`) the IP table is
/// parsed and its keys checked, but the data files are neither checked nor read and no IP module
/// is built.
async fn lens_registry(
    cfg: &lens::config::Config,
    startup: bool,
) -> Result<(Registry, Option<Arc<netray_ip::IpModule>>), String> {
    if let Some(name) = cfg
        .modules
        .keys()
        .find(|k| !matches!(k.as_str(), "http" | "email" | "ip" | "tls" | "dns"))
    {
        return Err(format!("modules.{name}: unknown module"));
    }
    let table = cfg.modules.get("http").cloned().unwrap_or_default();
    let module_config: netray_http::ModuleConfig =
        table.try_into().map_err(|e| format!("modules.http: {e}"))?;
    if cfg.backends.http.is_some() && module_config.enrichment.ip_url.is_none() {
        tracing::warn!(
            "modules.http.enrichment.ip_url is not set: server org and network type stay empty"
        );
    }
    let module =
        netray_http::HttpModule::new(module_config).map_err(|e| format!("modules.http: {e}"))?;
    let table = cfg.modules.get("email").cloned().unwrap_or_default();
    let email_config: netray_email::ModuleConfig = table
        .try_into()
        .map_err(|e| format!("modules.email: {e}"))?;
    let email = netray_email::EmailModule::new(email_config)
        .await
        .map_err(|e| format!("modules.email: {e}"))?;
    let table = match cfg.modules.get("tls") {
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
    let tls = netray_tls::TlsModule::new(tls_config)
        .await
        .map_err(|e| format!("modules.tls: {e}"))?;
    let table = match cfg.modules.get("dns") {
        Some(table) => table.clone(),
        None => {
            tracing::warn!("modules.dns is not configured: DNS runs on prism's defaults");
            Default::default()
        }
    };
    let dns_config: netray_dns::ModuleConfig =
        table.try_into().map_err(|e| format!("modules.dns: {e}"))?;
    if cfg.modules.contains_key("dns") && dns_config.backends.ip.is_none() {
        tracing::warn!("modules.dns.backends.ip is not set: the infrastructure check is absent");
    }
    let dns = Arc::new(
        netray_dns::DnsModule::new(dns_config)
            .await
            .map_err(|e| format!("modules.dns: {e}"))?,
    );
    let mut registry = Registry::new()
        .with(Box::new(Shared(dns.clone())))
        .with_facts(Box::new(Shared(dns)))
        .with(Box::new(module))
        .with(Box::new(email))
        .with(Box::new(tls));
    let Some(table) = cfg.modules.get("ip").cloned() else {
        tracing::warn!("modules.ip is not configured: the IP section is off");
        return Ok((registry, None));
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
        return Ok((registry, None));
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
    let ip = Arc::new(
        netray_ip::IpModule::new(ip_config)
            .await
            .map_err(|e| format!("modules.ip: {e}"))?,
    );
    registry = registry.with(Box::new(Shared(ip.clone())));
    Ok((registry, Some(ip)))
}

/// Reloads the IP module's data on every SIGHUP.
fn reload_ip_on_sighup(ip: Arc<netray_ip::IpModule>) {
    tokio::spawn(async move {
        let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("failed to register the SIGHUP handler");
        while sig.recv().await.is_some() {
            ip.reload().await;
        }
    });
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Lens {
            check_config: Some(path),
            ..
        } => {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::WARN)
                .with_writer(std::io::stderr)
                .init();
            let loaded = match lens::config::Config::load(Some(&path)) {
                Ok(cfg) => lens_registry(&cfg, false).await.map(drop),
                Err(e) => Err(e.to_string()),
            };
            check_config(&path, |_| loaded)
        }
        Command::Dns {
            check_config: Some(path),
            ..
        } => check_config(&path, netray_dns::config::Config::load),
        Command::Tls {
            check_config: Some(path),
            ..
        } => check_config(&path, |p| {
            let cfg = netray_tls::config::Config::load(p)?;
            cfg.check_startup()
        }),
        Command::Http {
            check_config: Some(path),
            ..
        } => check_config(&path, netray_http::config::Config::load),
        Command::Email {
            check_config: Some(path),
            ..
        } => check_config(&path, netray_email::config::Config::load),
        Command::Ip {
            check_config: Some(path),
            ..
        } => check_config(&path, netray_ip::config::Config::load),
        Command::Lens { config, .. } => {
            let path = config.or_else(|| std::env::var("LENS_CONFIG").ok());
            let cfg = lens::config::Config::load(path.as_deref())
                .map_err(|e| anyhow::anyhow!("failed to load configuration: {e}"))?;
            lens::init_telemetry(&cfg);
            let (registry, ip) = lens_registry(&cfg, true)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            if let Some(ip) = ip {
                reload_ip_on_sighup(ip);
            }
            lens::run_with(path, registry).await
        }
        Command::Dns { config, .. } => netray_dns::run(config).await,
        Command::Tls { config, .. } => netray_tls::run(config).await,
        Command::Http { config, .. } => netray_http::run(config).await,
        Command::Email { config, .. } => netray_email::run(config).await?,
        Command::Ip {
            config,
            check,
            print_config,
            ..
        } => netray_ip::run(config, print_config, check).await,
        Command::Site { bind, root } => site::run(bind, root).await?,
    }
    Ok(())
}
