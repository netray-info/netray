use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod config;
mod legacy;
mod registry;
mod serve;
mod site;
mod v1;

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
    /// The netray.info product: every host, one process
    Serve {
        config: Option<String>,
        /// Validate the configuration file at PATH and exit
        #[arg(long, value_name = "PATH")]
        check_config: Option<String>,
    },
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

/// Logs up to `level` to stderr, so `--check-config` keeps stdout to its one line.
fn init_check_logging(level: tracing::Level) {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Serve {
            check_config: Some(path),
            ..
        } => {
            // `serve --check-config` answers with its one line alone: an absent module table is
            // a default, not a finding.
            init_check_logging(tracing::Level::ERROR);
            let loaded = serve::check_config(&path).await;
            check_config(&path, |_| loaded)
        }
        Command::Lens {
            check_config: Some(path),
            ..
        } => {
            init_check_logging(tracing::Level::WARN);
            let loaded = v1::check_config(&path).await;
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
        Command::Serve { config, .. } => serve::run(config).await?,
        Command::Lens { config, .. } => v1::run(config).await?,
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
