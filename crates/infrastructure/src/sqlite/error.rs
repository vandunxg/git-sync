use std::fmt;

/// Typed persistence errors. Messages never include SQL parameters or row contents.
#[derive(Debug)]
pub enum StoreError {
    /// A caller-supplied value failed validation (field name only).
    InvalidInput(&'static str),
    NotFound,
    /// Unique constraint, e.g. the same idempotency key and attempt.
    Duplicate,
    /// A referenced row does not exist.
    ForeignKey,
    /// Another job for the repository or destination mapping holds its lock.
    ActiveJobExists,
    /// The queue is paused: nothing new starts.
    QueuePaused,
    /// The global concurrency limit is reached.
    CapacityReached,
    /// The job is not in a state from which the requested transition is allowed.
    InvalidTransition {
        current: String,
    },
    /// Optimistic concurrency check failed (expected revision differs).
    StaleRevision {
        current: i64,
    },
    /// The database was written by a newer build or a migration was modified.
    IncompatibleSchema(String),
    /// A migration failed; the database stays at the previous version.
    MigrationFailed(String),
    /// Pre-migration backup could not be written; no migration was applied.
    BackupFailed(String),
    /// `PRAGMA integrity_check` did not return `ok`.
    Corrupt,
    /// The database stayed locked longer than the busy timeout; safe to retry.
    Busy,
    Database(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(field) => write!(f, "invalid value for {field}"),
            Self::NotFound => f.write_str("record not found"),
            Self::Duplicate => f.write_str("record already exists"),
            Self::ForeignKey => f.write_str("referenced record does not exist"),
            Self::ActiveJobExists => f.write_str("repository already has an active job"),
            Self::QueuePaused => f.write_str("the queue is paused"),
            Self::CapacityReached => f.write_str("the concurrency limit is reached"),
            Self::InvalidTransition { current } => {
                write!(f, "job transition not allowed from {current}")
            }
            Self::StaleRevision { current } => write!(f, "stale revision (current {current})"),
            Self::IncompatibleSchema(why) => write!(f, "incompatible database schema: {why}"),
            Self::MigrationFailed(why) => write!(f, "database migration failed: {why}"),
            Self::BackupFailed(why) => write!(f, "pre-migration backup failed: {why}"),
            Self::Corrupt => f.write_str("database integrity check failed"),
            Self::Busy => f.write_str("database is busy; retry"),
            Self::Database(why) => write!(f, "database error: {why}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl StoreError {
    /// Whether the same operation may be retried unchanged.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Busy)
    }

    pub(crate) fn from_sqlx(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &error {
            // SQLite extended result codes; message text is not exposed.
            match db.code().as_deref() {
                Some("2067" | "1555") => {
                    if db.message().contains("jobs.repository_id")
                        || db.message().contains("jobs.mapping_id")
                    {
                        return Self::ActiveJobExists;
                    }
                    return Self::Duplicate;
                }
                Some("787") => return Self::ForeignKey,
                // SQLITE_BUSY, BUSY_RECOVERY, BUSY_SNAPSHOT, BUSY_TIMEOUT, SQLITE_LOCKED
                Some("5" | "261" | "517" | "773" | "6") => return Self::Busy,
                _ => {}
            }
        }
        if matches!(error, sqlx::Error::RowNotFound) {
            return Self::NotFound;
        }
        Self::Database(sanitized_kind(&error))
    }
}

fn sanitized_kind(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(db) => format!("sqlite code {}", db.code().unwrap_or_default()),
        sqlx::Error::Io(io) => format!("io {}", io.kind()),
        sqlx::Error::PoolTimedOut => "pool timed out".to_owned(),
        sqlx::Error::PoolClosed => "pool closed".to_owned(),
        sqlx::Error::ColumnDecode { index, .. } => format!("decode column {index}"),
        _ => "driver error".to_owned(),
    }
}
