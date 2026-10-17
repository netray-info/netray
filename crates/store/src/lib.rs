//! The V2 history (`history.db`, SDD §4): entries per domain, identical-run detection, the stored
//! diff and the retention sweep.

use std::fmt;
use std::path::Path;

use netray_model::Grade;
use netray_model::diff::Diff;
use netray_model::run::{Run, fingerprint};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteRow};
use sqlx::{Row, SqliteConnection, SqlitePool};

/// 90 days in seconds: how long a domain's latest entry may go unseen before the sweep drops it.
pub const RETENTION_SECS: i64 = 90 * 24 * 3600;

const ID_LEN: usize = 8;
const ID_ATTEMPTS: usize = 4;
const POOL_MAX_CONNECTIONS: u32 = 5;
const ID_ALPHABET: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I',
    'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'a', 'b',
    'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u',
    'v', 'w', 'x', 'y', 'z',
];

const ENTRY_COLUMNS: &str = "e.id, e.domain, e.created_at, e.last_seen_at, e.predecessor_id, \
     e.profile_version, e.grade, e.fingerprint, e.run, e.diff, e.engine_version";

/// The chain head first (the entry no other entry names as its predecessor), then by
/// `created_at`, newest first, then by insertion order, newest first. `created_at` never decreases
/// along a chain and writes are serialised, so this is newest first even within one second.
const NEWEST_FIRST: &str = "ORDER BY \
     CASE WHEN EXISTS (SELECT 1 FROM entries n WHERE n.predecessor_id = e.id) THEN 1 ELSE 0 END ASC, \
     e.created_at DESC, e.rowid DESC";

/// Why a store operation failed.
#[derive(Debug)]
pub struct StoreError(Kind);

#[derive(Debug)]
enum Kind {
    Sql(sqlx::Error),
    Migrate(sqlx::migrate::MigrateError),
    Json(serde_json::Error),
    Corrupt(String),
    IdCollision,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Kind::Sql(e) => write!(f, "history store: {e}"),
            Kind::Migrate(e) => write!(f, "history store migration: {e}"),
            Kind::Json(e) => write!(f, "history store payload: {e}"),
            Kind::Corrupt(what) => write!(f, "history store row: {what}"),
            Kind::IdCollision => write!(f, "history store: id collision, all attempts exhausted"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            Kind::Sql(e) => Some(e),
            Kind::Migrate(e) => Some(e),
            Kind::Json(e) => Some(e),
            Kind::Corrupt(_) | Kind::IdCollision => None,
        }
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        StoreError(Kind::Sql(e))
    }
}

impl From<sqlx::migrate::MigrateError> for StoreError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        StoreError(Kind::Migrate(e))
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError(Kind::Json(e))
    }
}

/// What `HistoryStore::record` did with a run.
#[derive(Debug, Clone, PartialEq)]
pub enum Recorded {
    /// No entry existed; a new one was stored without a diff.
    First { id: String },
    /// Fingerprints equal; the latest entry's `last_seen_at` moved to `now`.
    Identical { id: String },
    /// A new entry with `predecessor_id` = the previous latest, carrying `diff`.
    Changed { id: String, diff: Diff },
    /// `run.complete` was false: nothing written; the diff against the latest entry, if any.
    NotStored { diff: Option<Diff> },
}

/// One stored run of a domain.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub domain: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub predecessor_id: Option<String>,
    pub profile_version: u32,
    pub grade: Grade,
    pub fingerprint: String,
    pub run: Run,
    pub diff: Option<Diff>,
    pub engine_version: String,
}

/// The head of a domain's chain, without its payload columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub id: String,
    pub last_seen_at: i64,
    pub grade: Grade,
    pub profile_version: u32,
}

/// The history database. Every time value is Unix seconds, passed in by the caller.
pub struct HistoryStore {
    pool: SqlitePool,
    write: tokio::sync::Mutex<()>,
}

impl HistoryStore {
    /// Opens (creating the file if missing) a SQLite database in WAL mode. Fails when the
    /// directory does not exist or the file cannot be opened.
    pub async fn open(path: &Path) -> Result<HistoryStore, StoreError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = sqlx::pool::PoolOptions::<sqlx::Sqlite>::new()
            .max_connections(POOL_MAX_CONNECTIONS)
            .connect_with(options)
            .await?;
        Ok(HistoryStore {
            pool,
            write: tokio::sync::Mutex::new(()),
        })
    }

    /// Runs the migrations; idempotent.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Records `run` for `domain` (A-label) at `now`; a `now` before the chain head's times is
    /// clamped to them, so the chain never forks and `last_seen_at` never moves back. `diff` computes the diff of (predecessor,
    /// current); it is called only when a latest entry exists and the run is not identical, or
    /// when the run is incomplete and a latest entry exists.
    pub async fn record<F>(
        &self,
        domain: &str,
        run: &Run,
        volatile: &[&str],
        now: i64,
        diff: F,
    ) -> Result<Recorded, StoreError>
    where
        F: FnOnce(&Run, &Run) -> Diff,
    {
        let _guard = self.write.lock().await;
        let mut tx = self.pool.begin().await?;
        let latest = latest_in(&mut tx, domain).await?;

        if !run.complete {
            let diff = latest.map(|e| diff(&e.run, run));
            return Ok(Recorded::NotStored { diff });
        }

        let print = fingerprint(run, volatile);
        let recorded = match latest {
            Some(latest) if latest.fingerprint == print => {
                sqlx::query("UPDATE entries SET last_seen_at = ? WHERE id = ?")
                    .bind(now.max(latest.last_seen_at))
                    .bind(&latest.id)
                    .execute(&mut *tx)
                    .await?;
                Recorded::Identical { id: latest.id }
            }
            Some(latest) => {
                let diff = diff(&latest.run, run);
                let diff_json = serde_json::to_string(&diff)?;
                let id = insert(
                    &mut tx,
                    domain,
                    run,
                    &print,
                    now.max(latest.created_at),
                    Some(&latest.id),
                    Some(&diff_json),
                )
                .await?;
                Recorded::Changed { id, diff }
            }
            None => {
                let id = insert(&mut tx, domain, run, &print, now, None, None).await?;
                Recorded::First { id }
            }
        };
        tx.commit().await?;
        Ok(recorded)
    }

    /// The domain's latest entry, the head of its chain, if any.
    pub async fn latest(&self, domain: &str) -> Result<Option<Entry>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        latest_in(&mut conn, domain).await
    }

    /// The head of the domain's chain without its payload columns, if any.
    pub async fn head(&self, domain: &str) -> Result<Option<Head>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        Ok(head_in(&mut conn, domain).await?.map(|(head, _)| head))
    }

    /// One entry by id.
    pub async fn get(&self, id: &str) -> Result<Option<Entry>, StoreError> {
        let sql = format!("SELECT {ENTRY_COLUMNS} FROM entries e WHERE e.id = ?");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(entry_from_row).transpose()
    }

    /// Every entry of a domain, newest first.
    pub async fn entries(&self, domain: &str) -> Result<Vec<Entry>, StoreError> {
        let sql =
            format!("SELECT {ENTRY_COLUMNS} FROM entries e WHERE e.domain = ? {NEWEST_FIRST}");
        let rows = sqlx::query(&sql).bind(domain).fetch_all(&self.pool).await?;
        rows.iter().map(entry_from_row).collect()
    }

    /// The retention sweep at `now`; returns the number of deleted entries. A domain whose latest
    /// entry was last seen before `now - RETENTION_SECS` goes entirely; of the other domains,
    /// entries last seen before it go too, except the latest and its predecessor.
    pub async fn sweep(&self, now: i64) -> Result<u64, StoreError> {
        let _guard = self.write.lock().await;
        let cutoff = now - RETENTION_SECS;
        let mut tx = self.pool.begin().await?;

        let domains: Vec<String> = sqlx::query_scalar("SELECT DISTINCT domain FROM entries")
            .fetch_all(&mut *tx)
            .await?;

        let mut deleted = 0;
        for domain in domains {
            let Some((head, predecessor_id)) = head_in(&mut tx, &domain).await? else {
                continue;
            };
            let (latest_id, last_seen_at) = (head.id, head.last_seen_at);
            let result = if last_seen_at < cutoff {
                sqlx::query("DELETE FROM entries WHERE domain = ?")
                    .bind(&domain)
                    .execute(&mut *tx)
                    .await?
            } else {
                let keep_predecessor = predecessor_id.unwrap_or_default();
                sqlx::query(
                    "DELETE FROM entries WHERE domain = ? AND last_seen_at < ? AND id <> ? AND id <> ?",
                )
                .bind(&domain)
                .bind(cutoff)
                .bind(&latest_id)
                .bind(keep_predecessor)
                .execute(&mut *tx)
                .await?
            };
            deleted += result.rows_affected();
        }
        tx.commit().await?;
        Ok(deleted)
    }
}

async fn latest_in(conn: &mut SqliteConnection, domain: &str) -> Result<Option<Entry>, StoreError> {
    let sql =
        format!("SELECT {ENTRY_COLUMNS} FROM entries e WHERE e.domain = ? {NEWEST_FIRST} LIMIT 1");
    let row = sqlx::query(&sql)
        .bind(domain)
        .fetch_optional(&mut *conn)
        .await?;
    row.as_ref().map(entry_from_row).transpose()
}

/// The head's id, `last_seen_at`, `predecessor_id`, grade and profile version: no payload column
/// is read.
async fn head_in(
    conn: &mut SqliteConnection,
    domain: &str,
) -> Result<Option<(Head, Option<String>)>, StoreError> {
    let sql = format!(
        "SELECT e.id, e.last_seen_at, e.predecessor_id, e.grade, e.profile_version \
         FROM entries e WHERE e.domain = ? {NEWEST_FIRST} LIMIT 1"
    );
    let row = sqlx::query(&sql)
        .bind(domain)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(|r| {
        let grade: String = r.try_get("grade")?;
        let profile_version: i64 = r.try_get("profile_version")?;
        let head = Head {
            id: r.try_get("id")?,
            last_seen_at: r.try_get("last_seen_at")?,
            grade: serde_json::from_value(serde_json::Value::String(grade))?,
            profile_version: u32::try_from(profile_version)
                .map_err(|_| StoreError(Kind::Corrupt("profile_version out of range".into())))?,
        };
        Ok((head, r.try_get("predecessor_id")?))
    })
    .transpose()
}

async fn insert(
    conn: &mut SqliteConnection,
    domain: &str,
    run: &Run,
    print: &str,
    now: i64,
    predecessor_id: Option<&str>,
    diff_json: Option<&str>,
) -> Result<String, StoreError> {
    let run_json = serde_json::to_string(run)?;
    let grade = serde_json::to_value(run.grade)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| StoreError(Kind::Corrupt("grade is not a string".into())))?;

    for _ in 0..ID_ATTEMPTS {
        let id = nanoid::nanoid!(ID_LEN, ID_ALPHABET);
        let result = sqlx::query(
            "INSERT INTO entries (id, domain, created_at, last_seen_at, predecessor_id, \
             profile_version, grade, fingerprint, run, diff, engine_version) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(domain)
        .bind(now)
        .bind(now)
        .bind(predecessor_id)
        .bind(i64::from(run.profile_version))
        .bind(&grade)
        .bind(print)
        .bind(&run_json)
        .bind(diff_json)
        .bind(&run.engine_version)
        .execute(&mut *conn)
        .await;
        match result {
            Ok(_) => return Ok(id),
            Err(sqlx::Error::Database(ref e)) if e.is_unique_violation() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(StoreError(Kind::IdCollision))
}

fn entry_from_row(row: &SqliteRow) -> Result<Entry, StoreError> {
    let grade: String = row.try_get("grade")?;
    let grade = serde_json::from_value(serde_json::Value::String(grade))?;
    let run: String = row.try_get("run")?;
    let diff: Option<String> = row.try_get("diff")?;
    let profile_version: i64 = row.try_get("profile_version")?;
    Ok(Entry {
        id: row.try_get("id")?,
        domain: row.try_get("domain")?,
        created_at: row.try_get("created_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
        predecessor_id: row.try_get("predecessor_id")?,
        profile_version: u32::try_from(profile_version)
            .map_err(|_| StoreError(Kind::Corrupt("profile_version out of range".into())))?,
        grade,
        fingerprint: row.try_get("fingerprint")?,
        run: serde_json::from_str(&run)?,
        diff: diff.as_deref().map(serde_json::from_str).transpose()?,
        engine_version: row.try_get("engine_version")?,
    })
}
