use std::sync::Arc;

use tracing::{info, warn};

use super::store::{SnapshotStore, SNAPSHOT_SWEEP_INTERVAL, SNAPSHOT_TTL_SECS};

pub async fn run_sweep_loop(store: Arc<SnapshotStore>) {
    loop {
        tokio::time::sleep(SNAPSHOT_SWEEP_INTERVAL).await;
        match store.sweep_expired(SNAPSHOT_TTL_SECS).await {
            Ok(n) => info!("snapshot sweep deleted {n} expired snapshots"),
            Err(e) => warn!("snapshot sweep error: {e}"),
        }
    }
}
