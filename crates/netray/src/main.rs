use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use netray_engine::Registry;

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

/// Builds the modules the lens config names in `[modules.*]`; a table for a module the binary
/// does not know is refused, and an absent `[modules.http]` or `[modules.email]` builds the module
/// on its defaults.
async fn lens_registry(cfg: &lens::config::Config) -> Result<Registry, String> {
    if let Some(name) = cfg
        .modules
        .keys()
        .find(|k| !matches!(k.as_str(), "http" | "email"))
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
    Ok(Registry::new().with(Box::new(module)).with(Box::new(email)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Lens {
            check_config: Some(path),
            ..
        } => {
            let loaded = match lens::config::Config::load(Some(&path)) {
                Ok(cfg) => lens_registry(&cfg).await.map(drop),
                Err(e) => Err(e.to_string()),
            };
            check_config(&path, |_| loaded)
        }
        Command::Dns {
            check_config: Some(path),
            ..
        } => check_config(&path, prism::config::Config::load),
        Command::Tls {
            check_config: Some(path),
            ..
        } => check_config(&path, |p| {
            let cfg = tlsight::config::Config::load(p)?;
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
        } => check_config(&path, ifconfig_rs::config::Config::load),
        Command::Lens { config, .. } => {
            let path = config.or_else(|| std::env::var("LENS_CONFIG").ok());
            let cfg = lens::config::Config::load(path.as_deref())
                .map_err(|e| anyhow::anyhow!("failed to load configuration: {e}"))?;
            let registry = lens_registry(&cfg).await.map_err(|e| anyhow::anyhow!(e))?;
            lens::run_with(path, registry).await
        }
        Command::Dns { config, .. } => prism::run(config).await,
        Command::Tls { config, .. } => tlsight::run(config).await,
        Command::Http { config, .. } => netray_http::run(config).await,
        Command::Email { config, .. } => netray_email::run(config).await?,
        Command::Ip {
            config,
            check,
            print_config,
            ..
        } => ifconfig_rs::run(config, print_config, check).await,
        Command::Site { bind, root } => site::run(bind, root).await?,
    }
    Ok(())
}
