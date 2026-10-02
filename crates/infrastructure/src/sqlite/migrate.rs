//! Versioned migrations with downgrade refusal and pre-migration backup.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use sqlx::SqlitePool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use super::{StoreError, restrict_permissions};

/// Migrations embedded at compile time from `crates/infrastructure/migrations`.
pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Latest schema version this build understands.
pub const SCHEMA_VERSION: i64 = 2;

/// What `Database::open` did to the schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// Highest applied version before opening (`None` for a new database).
    pub from_version: Option<i64>,
    pub to_version: i64,
    pub applied: Vec<i64>,
    /// SQLite-aware copy taken before applying migrations to existing data.
    pub backup: Option<PathBuf>,
}

async fn table_exists(pool: &SqlitePool, name: &str) -> Result<bool, StoreError> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(name)
            .fetch_one(pool)
            .await
            .map_err(StoreError::from_sqlx)?;
    Ok(count > 0)
}

async fn user_table_count(pool: &SqlitePool) -> Result<i64, StoreError> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(pool)
    .await
    .map_err(StoreError::from_sqlx)?;
    Ok(count)
}

/// Validates the recorded migration history against `migrator` without changing
/// anything, and returns the applied versions. Refuses: tables without history,
/// partially applied migrations, versions unknown to this build, modified migrations.
async fn inspect(pool: &SqlitePool, migrator: &Migrator) -> Result<BTreeSet<i64>, StoreError> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        if user_table_count(pool).await? > 0 {
            // Never adopt or rewrite a database this application did not create.
            return Err(StoreError::IncompatibleSchema(
                "database has tables but no migration history".to_owned(),
            ));
        }
        return Ok(BTreeSet::new());
    }
    let rows: Vec<(i64, bool, Vec<u8>)> =
        sqlx::query_as("SELECT version, success, checksum FROM _sqlx_migrations")
            .fetch_all(pool)
            .await
            .map_err(StoreError::from_sqlx)?;
    let known: BTreeMap<i64, &[u8]> = migrator
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| (m.version, m.checksum.as_ref()))
        .collect();
    for (version, success, checksum) in &rows {
        if !success {
            return Err(StoreError::MigrationFailed(format!(
                "migration {version} is recorded as partially applied"
            )));
        }
        match known.get(version) {
            None => {
                return Err(StoreError::IncompatibleSchema(format!(
                    "database has migration {version}, which this build does not know (newer app?)"
                )));
            }
            Some(expected) if *expected != checksum.as_slice() => {
                return Err(StoreError::IncompatibleSchema(format!(
                    "applied migration {version} was modified"
                )));
            }
            Some(_) => {}
        }
    }
    Ok(rows.into_iter().map(|(version, ..)| version).collect())
}

/// Read-only inspection of an existing database file before the read-write pool
/// (which enables WAL) touches it.
pub(crate) async fn preflight(path: &Path, migrator: &Migrator) -> Result<(), StoreError> {
    let options = SqliteConnectOptions::from_str("sqlite://")
        .map_err(StoreError::from_sqlx)?
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(StoreError::from_sqlx)?;
    let result = inspect(&pool, migrator).await.map(|_| ());
    pool.close().await;
    result
}

fn backup_path(path: &Path, from: i64, to: i64) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "state.sqlite".to_owned());
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut candidate = dir.join(format!("{name}.pre-migration-v{from}-to-v{to}.bak"));
    let mut counter = 1;
    while candidate.exists() {
        candidate = dir.join(format!(
            "{name}.pre-migration-v{from}-to-v{to}.{counter}.bak"
        ));
        counter += 1;
    }
    candidate
}

/// SQLite-aware online backup of the whole database (including WAL content). A failed
/// backup removes its partial output; `VACUUM INTO` refuses to overwrite a file.
pub(crate) async fn backup_database(pool: &SqlitePool, target: &Path) -> Result<(), StoreError> {
    let target_text = target
        .to_str()
        .ok_or_else(|| StoreError::BackupFailed("backup path is not UTF-8".to_owned()))?;
    if let Err(error) = sqlx::query("VACUUM INTO ?")
        .bind(target_text)
        .execute(pool)
        .await
    {
        if target.exists() {
            // Only our own, just-created partial output is removed.
            let _cleanup = std::fs::remove_file(target);
        }
        return Err(StoreError::BackupFailed(
            StoreError::from_sqlx(error).to_string(),
        ));
    }
    restrict_permissions(target).map_err(|e| StoreError::BackupFailed(e.to_string()))
}

pub(crate) async fn migrate(
    pool: &SqlitePool,
    path: &Path,
    migrator: &Migrator,
) -> Result<MigrationReport, StoreError> {
    let known: BTreeSet<i64> = migrator
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| m.version)
        .collect();
    // All refusal checks happen before the backup, so a refused open leaves no backup.
    let applied = inspect(pool, migrator).await?;
    let pending: Vec<i64> = known.difference(&applied).copied().collect();
    let from_version = applied.iter().next_back().copied();
    let to_version = known.iter().next_back().copied().unwrap_or(0);
    let mut backup = None;
    if let (Some(from), false) = (from_version, pending.is_empty()) {
        let target = backup_path(path, from, to_version);
        backup_database(pool, &target).await?;
        backup = Some(target);
    }
    // Each SQLite migration runs in its own transaction: a failure leaves the database
    // at the last fully applied version (possibly an intermediate one when several are
    // pending; the backup above holds the pre-upgrade state).
    migrator.run(pool).await.map_err(|error| match error {
        MigrateError::VersionMismatch(v) => {
            StoreError::IncompatibleSchema(format!("applied migration {v} was modified"))
        }
        MigrateError::VersionMissing(v) => {
            StoreError::IncompatibleSchema(format!("applied migration {v} is unknown"))
        }
        MigrateError::ExecuteMigration(_, v) => {
            StoreError::MigrationFailed(format!("migration {v} failed and was rolled back"))
        }
        MigrateError::Dirty(v) => {
            StoreError::MigrationFailed(format!("migration {v} is partially applied"))
        }
        other => StoreError::MigrationFailed(other.to_string()),
    })?;
    restrict_permissions(path)?;
    Ok(MigrationReport {
        from_version,
        to_version,
        applied: pending,
        backup,
    })
}
