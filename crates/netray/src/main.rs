use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Lens {
            check_config: Some(path),
            ..
        } => check_config(&path, lens::config::Config::load),
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
        } => check_config(&path, beacon::config::Config::load),
        Command::Ip {
            check_config: Some(path),
            ..
        } => check_config(&path, ifconfig_rs::config::Config::load),
        Command::Lens { config, .. } => lens::run(config).await,
        Command::Dns { config, .. } => prism::run(config).await,
        Command::Tls { config, .. } => tlsight::run(config).await,
        Command::Http { config, .. } => netray_http::run(config).await,
        Command::Email { config, .. } => beacon::run(config).await?,
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
