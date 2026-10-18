//! `cargo run -p netray-store --example seed -- <db path> <fixture dir>`: seeds a history store.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [db, dir] = args.as_slice() else {
        eprintln!("usage: seed <db path> <fixture dir>");
        return ExitCode::FAILURE;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    match netray_store::seed::seed(&PathBuf::from(db), &PathBuf::from(dir), now).await {
        Ok(n) => {
            println!("seeded {n} fixture(s) into {db}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
