//! SQLite persistence (DATA_MODEL.md, ADR-004).
//!
//! Rules enforced here:
//! - versioned migrations from `crates/infrastructure/migrations`, never edited after
//!   release (sqlx checksums); a database newer than this build is refused;
//! - a SQLite-aware backup (`VACUUM INTO`) before applying pending migrations to a
//!   database that already holds data;
//! - every connection: foreign keys ON, WAL, `synchronous=FULL`, bounded busy timeout,
//!   bounded pool;
//! - parameterized SQL only; every store method opens its own short `BEGIN IMMEDIATE`
//!   write transaction (so concurrent writers wait on `busy_timeout` instead of
//!   failing a deferred read-to-write upgrade), commits or explicitly rolls back, and
//!   takes no async callbacks, so no network or Git await can run inside it;
//! - database and backup files are owner-only (0600) on Unix;
//! - each mutation increments the global read-model revision in the same transaction.

mod accounts;
mod activity;
mod error;
mod jobs;
mod migrate;
mod roots;
mod settings;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{Sqlite, Transaction};

pub use accounts::{AccountRecord, NewAccount};
pub use activity::{ActivityRecord, NewActivity, Severity};
pub use error::StoreError;
pub use jobs::{
    ActiveRow, Enqueued, JobKind, JobRecord, JobState, NewJob, QueuedRow, allowed_transition,
};
pub use migrate::{MigrationReport, SCHEMA_VERSION};
pub use roots::RootRecord;
pub use settings::SettingsRecord;

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONNECTIONS: u32 = 4;

/// Handle to the application database.
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
    path: PathBuf,
}

impl Database {
    /// Opens (creating if missing) the database at `path`, backs it up if migrations
    /// are pending on existing data, applies migrations and verifies integrity.
    pub async fn open(path: &Path) -> Result<(Self, MigrationReport), StoreError> {
        // Inspect an existing file read-only first, so a foreign or newer database is
        // refused before any connection switches it to WAL or creates tables.
        if path.exists() {
            migrate::preflight(path, &migrate::MIGRATOR).await?;
        }
        let options = connect_options(path)?;
        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(options)
            .await
            .map_err(StoreError::from_sqlx)?;
        restrict_permissions(path)?;
        let report = migrate::migrate(&pool, path, &migrate::MIGRATOR).await?;
        let database = Self {
            pool,
            path: path.to_owned(),
        };
        database.check_integrity().await?;
        Ok((database, report))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `PRAGMA integrity_check` must report exactly `ok`.
    pub async fn check_integrity(&self) -> Result<(), StoreError> {
        let rows: Vec<(String,)> = sqlx::query_as("PRAGMA integrity_check")
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::from_sqlx)?;
        match rows.as_slice() {
            [(status,)] if status == "ok" => Ok(()),
            _ => Err(StoreError::Corrupt),
        }
    }

    /// Current global read-model revision.
    pub async fn revision(&self) -> Result<i64, StoreError> {
        let (revision,): (i64,) =
            sqlx::query_as("SELECT current_revision FROM revisions WHERE singleton = 1")
                .fetch_one(&self.pool)
                .await
                .map_err(StoreError::from_sqlx)?;
        Ok(revision)
    }

    pub async fn close(self) {
        self.pool.close().await;
    }

    /// Starts a write transaction holding SQLite's RESERVED lock from the first
    /// statement; competing writers wait up to `busy_timeout`.
    async fn begin(&self) -> Result<Transaction<'static, Sqlite>, StoreError> {
        self.pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(StoreError::from_sqlx)
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

pub(crate) fn connect_options(path: &Path) -> Result<SqliteConnectOptions, StoreError> {
    // `from_str("sqlite://")` + `filename` avoids interpreting the path as a URL.
    Ok(SqliteConnectOptions::from_str("sqlite://")
        .map_err(StoreError::from_sqlx)?
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(BUSY_TIMEOUT))
}

/// Commits on success; on error rolls back explicitly before returning the error, so
/// the connection goes back to the pool clean.
pub(crate) async fn finish<T>(
    tx: Transaction<'static, Sqlite>,
    result: Result<T, StoreError>,
) -> Result<T, StoreError> {
    match result {
        Ok(value) => {
            tx.commit().await.map_err(StoreError::from_sqlx)?;
            Ok(value)
        }
        Err(error) => {
            // The original error is what matters; if rollback itself fails, dropping
            // the transaction still rolls it back.
            let _rollback = tx.rollback().await;
            Err(error)
        }
    }
}

/// Owner-only permissions for the database and its WAL/SHM side files (Unix).
pub(crate) fn restrict_permissions(path: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut files = vec![path.to_owned()];
        for suffix in ["-wal", "-shm"] {
            let mut side = path.as_os_str().to_owned();
            side.push(suffix);
            files.push(PathBuf::from(side));
        }
        for file in files.iter().filter(|f| f.exists()) {
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| StoreError::Database(format!("io {}", e.kind())))?;
        }
    }
    Ok(())
}

/// Increments the global revision inside `tx` and returns the new value.
pub(crate) async fn bump_revision(tx: &mut Transaction<'_, Sqlite>) -> Result<i64, StoreError> {
    let (revision,): (i64,) = sqlx::query_as(
        "UPDATE revisions SET current_revision = current_revision + 1 \
         WHERE singleton = 1 RETURNING current_revision",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::from_sqlx)?;
    Ok(revision)
}

pub(crate) fn require_id(field: &'static str, value: &str) -> Result<(), StoreError> {
    if gbm_domain::contract::schema::is_id(value) {
        Ok(())
    } else {
        Err(StoreError::InvalidInput(field))
    }
}

pub(crate) fn require_timestamp(field: &'static str, value: &str) -> Result<(), StoreError> {
    if gbm_domain::contract::schema::is_rfc3339(value) {
        Ok(())
    } else {
        Err(StoreError::InvalidInput(field))
    }
}

#[cfg(test)]
mod tests;
