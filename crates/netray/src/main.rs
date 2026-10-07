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
    Lens { config: Option<String> },
    /// DNS inspector (dns.netray.info)
    Dns { config: Option<String> },
    /// TLS certificate inspector (tls.netray.info)
    Tls { config: Option<String> },
    /// HTTP header inspector (http.netray.info)
    Http { config: Option<String> },
    /// Email security inspector (email.netray.info)
    Email { config: Option<String> },
    /// IP enrichment API (ip.netray.info)
    Ip {
        config: Option<String>,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Lens { config } => lens::run(config).await,
        Command::Dns { config } => prism::run(config).await,
        Command::Tls { config } => tlsight::run(config).await,
        Command::Http { config } => spectra::run(config).await,
        Command::Email { config } => beacon::run(config).await?,
        Command::Ip {
            config,
            check,
            print_config,
        } => ifconfig_rs::run(config, print_config, check).await,
        Command::Site { bind, root } => site::run(bind, root).await?,
    }
    Ok(())
}
