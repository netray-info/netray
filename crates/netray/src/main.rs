use clap::{Parser, Subcommand};

mod config;
mod legacy;
mod registry;
mod serve;

#[derive(Parser)]
#[command(
    name = "netray",
    version,
    about = "netray.info: every host, one process",
    disable_help_subcommand = true
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

#[cfg(test)]
mod tests {
    // specs/features/v2-correctness-followups Phase 2, requirement 3 (C1, C3, C4): a domain run
    // charges no per-target limit of the DNS module; lens admission bounds domain runs (S44).

    use std::time::{Duration, Instant};

    use netray_engine::{
        Domain, Facts, FactsProvider, Module, RunContext, RunOptions, SectionOutcome,
    };

    use super::lens_registry;

    const CONFIG: &str = "[modules.dns]\nservers = [\"9.9.9.9\"]\n\n[modules.dns.dns]\nallow_arbitrary_servers = true\n\n[modules.dns.limits]\nper_target_burst = 1\n";

    fn ctx() -> RunContext {
        RunContext {
            deadline: Instant::now() + Duration::from_millis(300),
            domain: Domain::new("example.com"),
            options: RunOptions::default(),
        }
    }

    /// The registry's DNS module goes on to the network and outlives its deadline in blocking
    /// tasks, so it runs on its own runtime, shut down without waiting for them.
    #[test]
    fn a_domain_run_charges_no_dns_per_target_limit() {
        let dir = std::env::temp_dir().join(format!("netray-lens-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("lens.toml");
        std::fs::write(&path, CONFIG).expect("config written");
        let cfg = lens::config::Config::load(Some(path.to_str().unwrap())).expect("config loads");

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime builds");
        let (runs, facts) = rt.block_on(async {
            let modules = lens_registry(&cfg, false).await.expect("modules build");
            let mut runs: Vec<SectionOutcome> = Vec::new();
            let mut facts: Vec<String> = Vec::new();
            for _ in 0..2 {
                runs.push(modules.dns.run(&ctx(), &Facts::default()).await);
                let c = ctx();
                if let Err(e) = modules.dns.resolve(&c, &c.domain).await {
                    facts.push(e.to_string());
                }
            }
            (runs, facts)
        });
        rt.shutdown_background();

        for outcome in &runs {
            assert!(
                !matches!(outcome, SectionOutcome::Incomplete { reason } if reason.contains("rate limit")),
                "a domain run refused by the DNS per-target limit: {outcome:?}"
            );
        }
        for error in &facts {
            assert!(
                !error.contains("rate limit"),
                "the facts stage refused by the DNS per-target limit: {error}"
            );
        }
    }
}
