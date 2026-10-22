use clap::{Parser, Subcommand};

mod config;
mod legacy;
mod registry;
mod serve;

#[derive(Parser)]
#[command(
    name = "netray",
    version,
    about = "netray.info: every host, one process"
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
            match serve::check_config(&path).await {
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
        Command::Serve { config, .. } => serve::run(config).await?,
    }
    Ok(())
}
