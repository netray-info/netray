use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::SqlitePool;
use thiserror::Error;

use crate::snapshot::shortid;
use crate::snapshot::types::Snapshot;

pub const SNAPSHOT_TTL_SECS: i64 = 2_592_000; // 30 days
pub const SNAPSHOT_SWEEP_INTERVAL: Duration = Duration::from_secs(3_600);
pub const SHORTID_LEN: usize = 8;
pub const SHORTID_ALPHABET: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H',
    'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
    's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];
pub const MAX_FINDINGS_PER_SECTION: usize = 50;
pub const SNAPSHOT_POOL_MAX_CONNECTIONS: u32 = 5;

pub struct SnapshotStore {
    pub(crate) pool: SqlitePool,
}

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("shortid collision — all 4 attempts exhausted")]
    Collision,
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

impl SnapshotStore {
    /// Run embedded migrations on the store's database connection pool.
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }

    pub async fn new(db_path: &Path) -> Result<Self, SnapshotError> {
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = sqlx::pool::PoolOptions::<sqlx::Sqlite>::new()
            .max_connections(SNAPSHOT_POOL_MAX_CONNECTIONS)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    pub async fn insert(&self, mut snap: Snapshot) -> Result<String, SnapshotError> {
        let domain = snap.domain.clone();
        let grade = snap.grade.clone();
        let created_at = snap.created_at.timestamp();
        let lens_version = snap.lens_version.clone();

        for _ in 0..4 {
            let id = shortid::generate();
            // Embed the generated shortid into the payload so round-tripping restores it.
            snap.shortid = id.clone();
            let payload = serde_json::to_string(&snap)
                .map_err(|e| SnapshotError::Sql(sqlx::Error::Protocol(e.to_string())))?;
            let result = sqlx::query(
                "INSERT OR ABORT INTO snapshots (shortid, domain, grade, created_at, lens_version, payload) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&domain)
            .bind(&grade)
            .bind(created_at)
            .bind(&lens_version)
            .bind(&payload)
            .execute(&self.pool)
            .await;

            match result {
                Ok(_) => return Ok(id),
                Err(sqlx::Error::Database(ref e)) if e.is_unique_violation() => continue,
                Err(e) => return Err(SnapshotError::Sql(e)),
            }
        }
        Err(SnapshotError::Collision)
    }

    pub async fn get(&self, shortid: &str) -> Result<Option<Snapshot>, SnapshotError> {
        let threshold = chrono::Utc::now().timestamp() - SNAPSHOT_TTL_SECS;
        let row = sqlx::query(
            "SELECT payload FROM snapshots WHERE shortid = ? AND created_at > ?",
        )
        .bind(shortid)
        .bind(threshold)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(None),
            Some(r) => {
                use sqlx::Row;
                let payload: String = r.try_get("payload")?;
                let snap: Snapshot = serde_json::from_str(&payload)
                    .map_err(|e| SnapshotError::Sql(sqlx::Error::Protocol(e.to_string())))?;
                Ok(Some(snap))
            }
        }
    }

    pub async fn sweep_expired(&self, ttl_secs: i64) -> Result<u64, SnapshotError> {
        let threshold = chrono::Utc::now().timestamp() - ttl_secs;
        let result = sqlx::query("DELETE FROM snapshots WHERE created_at < ?")
            .bind(threshold)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
