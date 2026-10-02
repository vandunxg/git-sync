//! TEST-DB: schema upgrade with sample data, interrupted migration, downgrade and
//! foreign-database refusal, foreign keys, revisions, active-job uniqueness, restart
//! recovery and absence of secret values.

use std::fs;
use std::path::{Path, PathBuf};

use gbm_test_fixtures::Sandbox;
use serde_json::json;
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqlitePoolOptions;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NOW: &str = "2026-01-01T00:00:00Z";
const LATER: &str = "2026-01-01T00:05:00Z";
const COVERAGE: &str = r#"{"gitRefs":"not_checked","localObjects":"not_checked","lfs":"not_checked","submodules":"none"}"#;
const MIGRATION_0001: &str = include_str!("../../migrations/0001_initial.sql");
const MIGRATION_0002: &str = include_str!("../../migrations/0002_scheduler.sql");
const MIGRATION_0003: &str = include_str!("../../migrations/0003_checkpoints.sql");

fn db_path(sandbox: &Sandbox) -> PathBuf {
    sandbox.path().join("state.sqlite")
}

/// Writes a migration directory and loads it as a runtime migrator.
async fn migrator(
    dir: &Path,
    files: &[(&str, &str)],
) -> Result<Migrator, Box<dyn std::error::Error>> {
    fs::create_dir_all(dir)?;
    for (name, sql) in files {
        fs::write(dir.join(name), sql)?;
    }
    Ok(Migrator::new(dir).await?)
}

async fn open_with(path: &Path, migrator: &Migrator) -> Result<MigrationReport, StoreError> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(connect_options(path)?)
        .await
        .map_err(StoreError::from_sqlx)?;
    let result = migrate::migrate(&pool, path, migrator).await;
    pool.close().await;
    result
}

async fn count(db: &Database, sql: &'static str) -> Result<i64, Box<dyn std::error::Error>> {
    let (n,): (i64,) = sqlx::query_as(sql).fetch_one(db.pool()).await?;
    Ok(n)
}

fn account<'a>(id: &'a str, user: &'a str) -> NewAccount<'a> {
    NewAccount {
        id,
        provider: "github",
        provider_user_id: user,
        username: "octo",
        credential_ref: Some("cred:k1"),
        auth_state: "connected",
        storage_mode: "keyring",
        verified_at: Some(NOW),
    }
}

async fn seed_repository(db: &Database, repo: &str) -> TestResult {
    db.upsert_account(&account("acc-1", "42")).await?;
    sqlx::query(
        "INSERT INTO repositories (id, account_id, provider_repository_id, full_name, \
         is_private, is_fork, is_archived, selected) VALUES (?, 'acc-1', ?, 'octo/x', 1, 0, 0, 1)",
    )
    .bind(repo)
    .bind(format!("9{}", repo.len()))
    .execute(db.pool())
    .await?;
    Ok(())
}

fn job<'a>(id: &'a str, repo: Option<&'a str>, key: &'a str) -> NewJob<'a> {
    NewJob {
        id,
        repository_id: repo,
        account_id: None,
        plan_id: None,
        mapping_id: None,
        not_before: None,
        kind: JobKind::Sync,
        stage: "preflight",
        attempt: 1,
        idempotency_key: key,
        now: NOW,
        coverage_json: COVERAGE,
    }
}

#[tokio::test]
async fn fresh_database_is_migrated_and_configured() -> TestResult {
    let sandbox = Sandbox::new("db-fresh")?;
    let (db, report) = Database::open(&db_path(&sandbox)).await?;
    assert_eq!(
        report,
        MigrationReport {
            from_version: None,
            to_version: 3,
            applied: vec![1, 2, 3],
            backup: None
        }
    );
    assert_eq!(SCHEMA_VERSION, 3);
    let (fk,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
        .fetch_one(db.pool())
        .await?;
    let (journal,): (String,) = sqlx::query_as("PRAGMA journal_mode")
        .fetch_one(db.pool())
        .await?;
    let (sync,): (i64,) = sqlx::query_as("PRAGMA synchronous")
        .fetch_one(db.pool())
        .await?;
    let (busy,): (i64,) = sqlx::query_as("PRAGMA busy_timeout")
        .fetch_one(db.pool())
        .await?;
    assert_eq!((fk, journal.as_str(), sync, busy), (1, "wal", 2, 5000));
    assert_eq!(db.revision().await?, 0);
    db.close().await;

    let (db, report) = Database::open(&db_path(&sandbox)).await?;
    assert_eq!(report.applied, Vec::<i64>::new());
    assert_eq!(report.backup, None);
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn upgrade_with_sample_data_takes_a_backup_and_preserves_rows() -> TestResult {
    let sandbox = Sandbox::new("db-upgrade")?;
    let path = db_path(&sandbox);
    let (db, _) = Database::open(&path).await?;
    seed_repository(&db, "repo-1").await?;
    db.enqueue_job(&job("job-1", Some("repo-1"), "key-1"))
        .await?;
    db.close().await;

    let v2 = "ALTER TABLE repositories ADD COLUMN description TEXT;";
    let m = migrator(
        &sandbox.path().join("m-v2"),
        &[
            ("0001_initial.sql", MIGRATION_0001),
            ("0002_scheduler.sql", MIGRATION_0002),
            ("0003_checkpoints.sql", MIGRATION_0003),
            ("0004_description.sql", v2),
        ],
    )
    .await?;
    let report = open_with(&path, &m).await?;
    assert_eq!(
        (
            report.from_version,
            report.to_version,
            report.applied.clone()
        ),
        (Some(3), 4, vec![4])
    );
    let backup = report.backup.ok_or("backup missing")?;
    assert!(backup.exists());

    // The backup is a complete, openable v3 database with the sample data.
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options(&backup)?)
        .await?;
    let (repos,): (i64,) = sqlx::query_as("SELECT count(*) FROM repositories")
        .fetch_one(&pool)
        .await?;
    let (max,): (i64,) = sqlx::query_as("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    assert_eq!((repos, max), (1, 3));
    pool.close().await;

    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options(&path)?)
        .await?;
    let (jobs,): (i64,) = sqlx::query_as("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await?;
    let (described,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM repositories WHERE description IS NULL")
            .fetch_one(&pool)
            .await?;
    assert_eq!((jobs, described), (1, 1));
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn interrupted_migration_rolls_back_and_can_be_retried() -> TestResult {
    let sandbox = Sandbox::new("db-interrupted")?;
    let path = db_path(&sandbox);
    let (db, _) = Database::open(&path).await?;
    seed_repository(&db, "repo-1").await?;
    db.close().await;

    // Fails half-way: the first statement succeeds, the second references a missing table.
    let broken = "CREATE TABLE partial_table (id TEXT); INSERT INTO no_such_table VALUES (1);";
    let m = migrator(
        &sandbox.path().join("m-broken"),
        &[
            ("0001_initial.sql", MIGRATION_0001),
            ("0002_scheduler.sql", MIGRATION_0002),
            ("0003_checkpoints.sql", MIGRATION_0003),
            ("0004_x.sql", broken),
        ],
    )
    .await?;
    let error = open_with(&path, &m)
        .await
        .err()
        .ok_or("broken migration must fail")?;
    assert!(matches!(error, StoreError::MigrationFailed(_)), "{error}");

    let (db, report) = Database::open(&path).await?;
    assert_eq!(report.from_version, Some(3));
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM sqlite_master WHERE name = 'partial_table'"
        )
        .await?,
        0
    );
    assert_eq!(count(&db, "SELECT count(*) FROM repositories").await?, 1);
    assert_eq!(
        count(&db, "SELECT max(version) FROM _sqlx_migrations").await?,
        3
    );
    db.close().await;

    let fixed = "CREATE TABLE partial_table (id TEXT);";
    let m = migrator(
        &sandbox.path().join("m-fixed"),
        &[
            ("0001_initial.sql", MIGRATION_0001),
            ("0002_scheduler.sql", MIGRATION_0002),
            ("0003_checkpoints.sql", MIGRATION_0003),
            ("0004_x.sql", fixed),
        ],
    )
    .await?;
    assert_eq!(open_with(&path, &m).await?.applied, vec![4]);
    Ok(())
}

#[tokio::test]
async fn newer_modified_or_foreign_databases_are_refused() -> TestResult {
    let sandbox = Sandbox::new("db-refuse")?;
    let path = db_path(&sandbox);
    let m = migrator(
        &sandbox.path().join("m-newer"),
        &[
            ("0001_initial.sql", MIGRATION_0001),
            ("0002_scheduler.sql", MIGRATION_0002),
            ("0003_checkpoints.sql", MIGRATION_0003),
            ("0004_future.sql", "CREATE TABLE future (id TEXT);"),
        ],
    )
    .await?;
    open_with(&path, &m).await?;
    let newer = Database::open(&path)
        .await
        .err()
        .ok_or("newer schema must be refused")?;
    assert!(
        matches!(newer, StoreError::IncompatibleSchema(_)),
        "{newer}"
    );

    let other = sandbox.path().join("modified.sqlite");
    Database::open(&other).await?.0.close().await;
    let m = migrator(
        &sandbox.path().join("m-mod"),
        &[("0001_initial.sql", "CREATE TABLE accounts (id TEXT);")],
    )
    .await?;
    let modified = open_with(&other, &m)
        .await
        .err()
        .ok_or("modified migration must be refused")?;
    assert!(
        matches!(modified, StoreError::IncompatibleSchema(_)),
        "{modified}"
    );

    let foreign = sandbox.path().join("foreign.sqlite");
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options(&foreign)?)
        .await?;
    sqlx::query("CREATE TABLE notes (body TEXT)")
        .execute(&pool)
        .await?;
    pool.close().await;
    let refused = Database::open(&foreign)
        .await
        .err()
        .ok_or("foreign db must be refused")?;
    assert!(
        matches!(refused, StoreError::IncompatibleSchema(_)),
        "{refused}"
    );
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options(&foreign)?)
        .await?;
    let (tables,): (i64,) = sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE type='table'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(tables, 1, "foreign database must be left untouched");
    pool.close().await;

    let garbage = sandbox.path().join("garbage.sqlite");
    fs::write(
        &garbage,
        b"this is not a sqlite database at all, just bytes........",
    )?;
    assert!(Database::open(&garbage).await.is_err());
    Ok(())
}

#[tokio::test]
async fn foreign_keys_and_idempotency_are_enforced() -> TestResult {
    let sandbox = Sandbox::new("db-fk")?;
    let (db, _) = Database::open(&db_path(&sandbox)).await?;
    let missing = db
        .enqueue_job(&job("job-x", Some("no-such-repo"), "key-x"))
        .await;
    assert!(
        matches!(missing, Err(StoreError::ForeignKey)),
        "{missing:?}"
    );
    seed_repository(&db, "repo-1").await?;
    db.enqueue_job(&job("job-1", Some("repo-1"), "key-1"))
        .await?;
    let duplicate = db.enqueue_job(&job("job-2", Some("repo-1"), "key-1")).await;
    assert!(
        matches!(duplicate, Err(StoreError::Duplicate)),
        "{duplicate:?}"
    );
    let retry = NewJob {
        attempt: 2,
        ..job("job-3", Some("repo-1"), "key-1")
    };
    db.enqueue_job(&retry).await?;
    let bad_coverage = NewJob {
        coverage_json: "{}",
        ..job("job-4", None, "key-4")
    };
    assert!(matches!(
        db.enqueue_job(&bad_coverage).await,
        Err(StoreError::InvalidInput("coverage_json"))
    ));
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn one_active_job_per_repository() -> TestResult {
    let sandbox = Sandbox::new("db-active")?;
    let (db, _) = Database::open(&db_path(&sandbox)).await?;
    seed_repository(&db, "repo-1").await?;
    db.enqueue_job(&job("job-1", Some("repo-1"), "key-1"))
        .await?;
    db.enqueue_job(&job("job-2", Some("repo-1"), "key-2"))
        .await?;
    db.transition_job("job-1", JobState::Running, "fetch", None, LATER)
        .await?;
    let blocked = db
        .transition_job("job-2", JobState::Running, "fetch", None, LATER)
        .await;
    assert!(
        matches!(blocked, Err(StoreError::ActiveJobExists)),
        "{blocked:?}"
    );
    db.transition_job("job-1", JobState::CancelRequested, "fetch", None, LATER)
        .await?;
    let still = db
        .transition_job("job-2", JobState::Running, "fetch", None, LATER)
        .await;
    assert!(
        matches!(still, Err(StoreError::ActiveJobExists)),
        "cancel_requested still owns the repo"
    );
    db.transition_job("job-1", JobState::Canceled, "fetch", None, LATER)
        .await?;
    db.transition_job("job-2", JobState::Running, "fetch", None, LATER)
        .await?;
    // Jobs without a repository (e.g. scans) are not limited by the index.
    db.enqueue_job(&job("scan-1", None, "scan-key-1")).await?;
    db.enqueue_job(&job("scan-2", None, "scan-key-2")).await?;
    db.transition_job("scan-1", JobState::Running, "observe_source", None, LATER)
        .await?;
    db.transition_job("scan-2", JobState::Running, "observe_source", None, LATER)
        .await?;
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn transitions_follow_the_state_machine() -> TestResult {
    use JobState::*;
    let all = [
        Queued,
        Running,
        WaitingForApproval,
        WaitingForRetry,
        CancelRequested,
        Succeeded,
        Partial,
        Failed,
        Canceled,
        Interrupted,
    ];
    for from in all.into_iter().filter(|s| s.is_terminal()) {
        assert!(
            all.iter().all(|to| !allowed_transition(from, *to)),
            "{from:?} is terminal"
        );
    }
    assert!(
        !allowed_transition(Queued, Succeeded),
        "cannot succeed without running"
    );
    // An interrupted attempt is only ever closed (after reconciliation); the logical
    // job continues as a new attempt, never by re-queuing or re-running this row.
    for to in [Queued, Running, CancelRequested, Canceled, Interrupted] {
        assert!(!allowed_transition(Interrupted, to), "{to:?}");
    }
    for to in [Succeeded, Partial, Failed] {
        assert!(allowed_transition(Interrupted, to), "{to:?}");
    }
    for waiting in [WaitingForApproval, WaitingForRetry] {
        assert!(
            all.iter()
                .all(|s| !allowed_transition(*s, waiting) && !allowed_transition(waiting, *s)),
            "{waiting:?} edges await scheduler-task ratification"
        );
    }

    let sandbox = Sandbox::new("db-transitions")?;
    let (db, _) = Database::open(&db_path(&sandbox)).await?;
    db.enqueue_job(&job("job-1", None, "key-1")).await?;
    let skip = db
        .transition_job("job-1", Succeeded, "finalize", None, LATER)
        .await;
    assert!(matches!(skip, Err(StoreError::InvalidTransition { .. })));
    db.transition_job("job-1", Running, "fetch", None, LATER)
        .await?;
    db.transition_job("job-1", Failed, "push", Some("PUSH_PARTIAL"), LATER)
        .await?;
    let again = db
        .transition_job("job-1", Running, "fetch", None, LATER)
        .await;
    assert!(matches!(again, Err(StoreError::InvalidTransition { .. })));
    assert!(matches!(
        db.transition_job("job-1", Running, "nonsense", None, LATER)
            .await,
        Err(StoreError::InvalidInput("stage"))
    ));
    assert!(matches!(
        db.transition_job("nope", Running, "fetch", None, LATER)
            .await,
        Err(StoreError::NotFound)
    ));
    let record = db.get_job("job-1").await?.ok_or("job missing")?;
    assert_eq!(
        (record.state, record.error_code.as_deref()),
        (Failed, Some("PUSH_PARTIAL"))
    );
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn restart_recovery_interrupts_owned_jobs() -> TestResult {
    let sandbox = Sandbox::new("db-recovery")?;
    let path = db_path(&sandbox);
    let (db, _) = Database::open(&path).await?;
    seed_repository(&db, "repo-1").await?;
    for (id, key) in [
        ("job-run", "k1"),
        ("job-cancel", "k2"),
        ("job-queued", "k3"),
    ] {
        db.enqueue_job(&job(id, None, key)).await?;
    }
    db.transition_job("job-run", JobState::Running, "push", None, LATER)
        .await?;
    db.transition_job("job-cancel", JobState::Running, "fetch", None, LATER)
        .await?;
    db.transition_job(
        "job-cancel",
        JobState::CancelRequested,
        "fetch",
        None,
        LATER,
    )
    .await?;
    // Simulated crash: the process disappears without finishing these jobs.
    drop(db);

    let (db, _) = Database::open(&path).await?;
    let before = db.revision().await?;
    let recovered = db.recover_interrupted_jobs(LATER).await?;
    assert_eq!(recovered, ["job-cancel", "job-run"]);
    assert_eq!(
        db.revision().await?,
        before + 1,
        "one atomic revision for the recovery"
    );
    let run = db.get_job("job-run").await?.ok_or("missing")?;
    let cancel = db.get_job("job-cancel").await?.ok_or("missing")?;
    let queued = db.get_job("job-queued").await?.ok_or("missing")?;
    assert_eq!((run.state, run.error_code), (JobState::Interrupted, None));
    assert_eq!(
        (cancel.state, cancel.error_code.as_deref()),
        (JobState::Interrupted, Some("CANCEL_UNCERTAIN"))
    );
    assert_eq!(
        queued.state,
        JobState::Queued,
        "queued intent survives restart"
    );
    assert_eq!(
        db.recover_interrupted_jobs(LATER).await?,
        Vec::<String>::new()
    );
    // Leaving `interrupted` goes through reconciliation (`resolve_interrupted`).
    db.resolve_interrupted(
        "job-run",
        InterruptedClose::Failed,
        Some("PLAN_STALE"),
        Some("job-run-2"),
        LATER,
    )
    .await?;
    assert_eq!(
        db.get_job("job-run-2").await?.ok_or("missing")?.state,
        JobState::Queued
    );
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn revisions_increase_with_every_committed_mutation() -> TestResult {
    let sandbox = Sandbox::new("db-revisions")?;
    let (db, _) = Database::open(&db_path(&sandbox)).await?;
    let r1 = db.upsert_account(&account("acc-1", "42")).await?;
    let r2 = db.enqueue_job(&job("job-1", None, "key-1")).await?;
    let r3 = db
        .transition_job("job-1", JobState::Running, "fetch", None, LATER)
        .await?;
    assert!(r1 < r2 && r2 < r3);
    assert_eq!(db.get_job("job-1").await?.ok_or("missing")?.revision, r3);
    assert_eq!(db.revision().await?, r3);
    // A rejected mutation does not advance the revision.
    assert!(
        db.transition_job("job-1", JobState::Queued, "fetch", None, LATER)
            .await
            .is_err()
    );
    assert_eq!(db.revision().await?, r3);
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn no_secret_values_are_stored() -> TestResult {
    let sandbox = Sandbox::new("db-secrets")?;
    let path = db_path(&sandbox);
    let (db, _) = Database::open(&path).await?;
    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(db.pool())
            .await?;
    for (table,) in tables {
        let columns: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(db.pool())
            .await?;
        for (name,) in columns {
            let lowered = name.to_ascii_lowercase();
            assert!(
                !["token", "secret", "password", "pat"]
                    .iter()
                    .any(|w| lowered.split('_').any(|p| p == *w)),
                "{table}.{name} looks like a secret column"
            );
        }
    }
    const MARKER: &str = "glpat-SYNTHETICMARKER000";
    for bad in [
        MARKER,
        "ghp_SYNTHETICMARKER000",
        "github_pat_X",
        "cred:glpat-x",
        "cred:",
        "cred:0123456789abcdef0123456789abcdef01234567",
    ] {
        let attempt = NewAccount {
            credential_ref: Some(bad),
            ..account("acc-1", "42")
        };
        assert!(
            matches!(
                db.upsert_account(&attempt).await,
                Err(StoreError::InvalidInput("credential_ref"))
            ),
            "{bad}"
        );
    }
    let leaky = NewActivity {
        id: "act-1",
        repository_id: None,
        job_id: None,
        occurred_at: NOW,
        severity: Severity::Error,
        event_type: "push_failed",
        safe_message: "push to https://oauth2:glpat-SYNTHETICMARKER000@gitlab.com failed",
        error_code: None,
    };
    assert!(matches!(
        db.append_activity(&leaky).await,
        Err(StoreError::InvalidInput("safe_message"))
    ));
    let no_store = NewAccount {
        storage_mode: "none",
        ..account("acc-1", "42")
    };
    assert!(matches!(
        db.upsert_account(&no_store).await,
        Err(StoreError::InvalidInput("credential_ref"))
    ));
    let missing_ref = NewAccount {
        credential_ref: None,
        ..account("acc-1", "42")
    };
    assert!(matches!(
        db.upsert_account(&missing_ref).await,
        Err(StoreError::InvalidInput("credential_ref"))
    ));
    db.upsert_account(&account("acc-1", "42")).await?;
    db.disconnect_account("acc-1").await?;
    let accounts = db.list_accounts().await?;
    assert_eq!(accounts.len(), 1, "disconnect keeps the account row");
    assert_eq!(
        (
            accounts[0].credential_ref.as_deref(),
            accounts[0].auth_state.as_str()
        ),
        (None, "disconnected")
    );
    db.close().await;
    // Defence-in-depth scan: the APIs above refuse the marker, so it must not appear in
    // the database or WAL files either.
    for file in fs::read_dir(sandbox.path())? {
        let file = file?.path();
        if file.is_file() {
            let bytes = fs::read(&file)?;
            assert!(
                !bytes.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()),
                "marker in {}",
                file.display()
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn queries_are_parameterized() -> TestResult {
    let sandbox = Sandbox::new("db-params")?;
    let (db, _) = Database::open(&db_path(&sandbox)).await?;
    let hostile = "x'); DROP TABLE accounts; --";
    db.upsert_account(&NewAccount {
        username: hostile,
        ..account("acc-1", "42")
    })
    .await?;
    let accounts = db.list_accounts().await?;
    assert_eq!(accounts[0].username, hostile);
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM sqlite_master WHERE name = 'accounts'"
        )
        .await?,
        1
    );
    let rejected = NewAccount {
        id: "acc-2' OR '1'='1",
        ..account("acc-2", "43")
    };
    assert!(matches!(
        db.upsert_account(&rejected).await,
        Err(StoreError::InvalidInput("id"))
    ));
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn settings_use_optimistic_concurrency_and_contract_validation() -> TestResult {
    let sandbox = Sandbox::new("db-settings")?;
    let (db, _) = Database::open(&db_path(&sandbox)).await?;
    assert_eq!(db.get_settings().await?, None, "no invented defaults");
    let doc = json!({
        "scanIntervalMinutes": 30, "autoScan": true, "autoSync": false, "maxConcurrentJobs": 3,
        "includePrivate": true, "includeForks": true, "includeArchived": true,
        "includeCollaborations": false, "includeOrganizations": false, "workspaceEnabled": false,
        "notifyErrors": true, "notifySuccess": false, "startMinimized": false,
        "closeBehavior": "ask", "theme": "light", "retentionCleanup": "manual", "telemetry": false
    });
    let r1 = db.put_settings(&doc, None).await?;
    let stale = db.put_settings(&doc, None).await;
    assert!(matches!(stale, Err(StoreError::StaleRevision { current }) if current == r1));
    let mut telemetry = doc.clone();
    telemetry["telemetry"] = json!(true);
    assert!(matches!(
        db.put_settings(&telemetry, Some(r1)).await,
        Err(StoreError::InvalidInput("settings"))
    ));
    let r2 = db.put_settings(&doc, Some(r1)).await?;
    assert_eq!(
        db.get_settings().await?,
        Some(SettingsRecord {
            revision: r2,
            document: doc
        })
    );
    db.close().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writers_serialize_without_busy_errors() -> TestResult {
    let sandbox = Sandbox::new("db-concurrent")?;
    let (db, _) = Database::open(&db_path(&sandbox)).await?;
    seed_repository(&db, "repo-1").await?;
    let ids: Vec<String> = (0..8).map(|i| format!("job-{i}")).collect();
    for (i, id) in ids.iter().enumerate() {
        db.enqueue_job(&job(id, Some("repo-1"), &format!("key-{i}")))
            .await?;
    }
    // Eight different jobs for one repository race to start: exactly one wins.
    let mut handles = Vec::new();
    for id in ids.clone() {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            db.transition_job(&id, JobState::Running, "fetch", None, LATER)
                .await
        }));
    }
    let mut started = 0;
    for handle in handles {
        match handle.await? {
            Ok(_) => started += 1,
            Err(StoreError::ActiveJobExists) => {}
            Err(other) => return Err(format!("unexpected {other:?}").into()),
        }
    }
    assert_eq!(started, 1);
    // Eight writers race the same edge on one job while 16 independent inserts run.
    db.enqueue_job(&job("solo", None, "solo-key")).await?;
    let mut racers = Vec::new();
    for _ in 0..8 {
        let db = db.clone();
        racers.push(tokio::spawn(async move {
            db.transition_job("solo", JobState::Running, "fetch", None, LATER)
                .await
        }));
    }
    let mut inserts = Vec::new();
    for i in 0..16 {
        let db = db.clone();
        inserts.push(tokio::spawn(async move {
            let id = format!("bulk-{i}");
            let key = format!("bulk-key-{i}");
            db.enqueue_job(&job(&id, None, &key)).await
        }));
    }
    let (mut wins, mut lost) = (0, 0);
    for handle in racers {
        match handle.await? {
            Ok(_) => wins += 1,
            Err(StoreError::InvalidTransition { .. }) => lost += 1,
            Err(other) => return Err(format!("unexpected {other:?}").into()),
        }
    }
    for handle in inserts {
        handle.await??;
    }
    assert_eq!((wins, lost), (1, 7));
    // seed account (1) + 8 enqueues + 1 start + solo enqueue + 1 race win + 16 inserts
    assert_eq!(db.revision().await?, 1 + 8 + 1 + 1 + 1 + 16);
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn dirty_history_and_refused_opens_leave_no_side_effects() -> TestResult {
    let sandbox = Sandbox::new("db-dirty")?;
    let path = db_path(&sandbox);
    let (db, _) = Database::open(&path).await?;
    seed_repository(&db, "repo-1").await?;
    sqlx::query("UPDATE _sqlx_migrations SET success = 0 WHERE version = 1")
        .execute(db.pool())
        .await?;
    db.close().await;
    let dirty = Database::open(&path)
        .await
        .err()
        .ok_or("dirty history must be refused")?;
    assert!(matches!(dirty, StoreError::MigrationFailed(_)), "{dirty}");

    // A refused upgrade (modified migration + a pending one) must not leave a backup.
    let clean = sandbox.path().join("clean.sqlite");
    Database::open(&clean).await?.0.close().await;
    let m = migrator(
        &sandbox.path().join("m-mod2"),
        &[
            ("0001_initial.sql", "CREATE TABLE accounts (id TEXT);"),
            ("0002_x.sql", "CREATE TABLE x (id TEXT);"),
        ],
    )
    .await?;
    assert!(open_with(&clean, &m).await.is_err());
    let backups = fs::read_dir(sandbox.path())?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".pre-migration-"))
        .count();
    assert_eq!(backups, 0, "refused open must not leave a backup");

    // A foreign database is refused before WAL is enabled on it.
    let foreign = sandbox.path().join("foreign2.sqlite");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&foreign)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    sqlx::query("CREATE TABLE notes (body TEXT)")
        .execute(&pool)
        .await?;
    pool.close().await;
    assert!(Database::open(&foreign).await.is_err());
    let mut wal = foreign.as_os_str().to_owned();
    wal.push("-wal");
    assert!(
        !PathBuf::from(wal).exists(),
        "foreign database must not get a WAL file"
    );
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&foreign)
        .read_only(true);
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    let (mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await?;
    assert_eq!(mode, "delete");
    pool.close().await;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn database_and_backup_files_are_owner_only() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let sandbox = Sandbox::new("db-perms")?;
    let path = db_path(&sandbox);
    let (db, _) = Database::open(&path).await?;
    seed_repository(&db, "repo-1").await?;
    db.close().await;
    let m = migrator(
        &sandbox.path().join("m-perm"),
        &[
            ("0001_initial.sql", MIGRATION_0001),
            ("0002_scheduler.sql", MIGRATION_0002),
            ("0003_checkpoints.sql", MIGRATION_0003),
            ("0004_x.sql", "CREATE TABLE x (id TEXT);"),
        ],
    )
    .await?;
    let backup = open_with(&path, &m).await?.backup.ok_or("backup missing")?;
    for file in [path.clone(), backup] {
        let mode = fs::metadata(&file)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{} has mode {mode:o}", file.display());
    }
    Ok(())
}

mod scheduler_queue {
    //! TEST-SCHEDULER with real SQLite: 100 repositories, duplicate requests,
    //! concurrency 3, pause, shared destination mapping lock and retry attempts.

    use gbm_application::scheduler::{
        ActiveJob, JobIdentity, QueuedJob, idempotency_key, select_starts,
    };
    use gbm_application::time::rfc3339_to_ms;

    use super::*;

    async fn seed_many(db: &Database, count: usize) -> TestResult {
        db.upsert_account(&account("acc-1", "42")).await?;
        for i in 0..count {
            sqlx::query(
                "INSERT INTO repositories (id, account_id, provider_repository_id, full_name, \
                 is_private, is_fork, is_archived, selected) \
                 VALUES (?, 'acc-1', ?, 'octo/x', 1, 0, 0, 1)",
            )
            .bind(format!("r{i:03}"))
            .bind(format!("{}", 1000 + i))
            .execute(db.pool())
            .await?;
        }
        Ok(())
    }

    fn sync_job<'a>(id: &'a str, repo: &'a str, key: &'a str) -> NewJob<'a> {
        NewJob {
            plan_id: None,
            ..job(id, Some(repo), key)
        }
    }

    async fn plan_round(
        db: &Database,
        paused: bool,
        now_ms: i64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let queued: Vec<QueuedJob> = db
            .queued_jobs()
            .await?
            .into_iter()
            .map(|row| QueuedJob {
                not_before_ms: row.not_before_ms,
                id: row.id,
                repository_id: row.repository_id,
                mapping_id: row.mapping_id,
                sequence: row.sequence,
            })
            .collect();
        let active: Vec<ActiveJob> = db
            .active_jobs()
            .await?
            .into_iter()
            .map(|row| ActiveJob {
                id: row.id,
                repository_id: row.repository_id,
                mapping_id: row.mapping_id,
            })
            .collect();
        Ok(select_starts(&queued, &active, paused, 3, now_ms))
    }

    #[tokio::test]
    async fn a_hundred_repositories_with_duplicates_run_at_most_three_at_a_time() -> TestResult {
        let sandbox = Sandbox::new("db-scheduler")?;
        let (db, _) = Database::open(&db_path(&sandbox)).await?;
        seed_many(&db, 100).await?;
        let mut keys = Vec::new();
        for i in 0..100 {
            let repo = format!("r{i:03}");
            keys.push(idempotency_key(&JobIdentity {
                kind: "sync",
                repository_id: Some(&repo),
                basis: Some("snap-1"),
                mapping_id: None,
                policy_revision: 1,
            }));
        }
        for (i, key) in keys.iter().enumerate() {
            let (id, repo) = (format!("job-{i:03}"), format!("r{i:03}"));
            assert!(matches!(
                db.enqueue_or_existing(&sync_job(&id, &repo, key)).await?,
                Enqueued::New(_)
            ));
        }
        // Duplicate requests for the same logical jobs return the existing job.
        for (i, key) in keys.iter().enumerate().take(10) {
            let (id, repo) = (format!("dup-{i:03}"), format!("r{i:03}"));
            assert_eq!(
                db.enqueue_or_existing(&sync_job(&id, &repo, key)).await?,
                Enqueued::Existing(format!("job-{i:03}"))
            );
        }
        assert_eq!(count(&db, "SELECT count(*) FROM jobs").await?, 100);

        let mut finished = 0;
        let mut rounds = 0;
        while finished < 100 && rounds < 500 {
            rounds += 1;
            if rounds == 5 {
                // Pause: nothing new starts; running jobs are not touched.
                let running_before = db.active_jobs().await?;
                assert!(plan_round(&db, true, 0).await?.is_empty());
                assert_eq!(db.active_jobs().await?, running_before);
            }
            for id in plan_round(&db, false, 0).await? {
                db.transition_job(&id, JobState::Running, "fetch", None, LATER)
                    .await?;
            }
            let active = db.active_jobs().await?;
            assert!(active.len() <= 3, "{} running", active.len());
            if let Some(first) = active.first() {
                db.transition_job(&first.id, JobState::Succeeded, "finalize", None, LATER)
                    .await?;
                finished += 1;
            }
        }
        assert_eq!(finished, 100);
        assert_eq!(
            count(&db, "SELECT count(*) FROM jobs WHERE state = 'succeeded'").await?,
            100
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_shared_destination_mapping_is_locked_and_retries_are_new_attempts() -> TestResult {
        let sandbox = Sandbox::new("db-mapping-lock")?;
        let (db, _) = Database::open(&db_path(&sandbox)).await?;
        seed_many(&db, 2).await?;
        db.upsert_account(&NewAccount {
            id: "acc-gl",
            provider: "gitlab",
            ..account("acc-gl", "77")
        })
        .await?;
        sqlx::query(
            "INSERT INTO roots (id, canonical_path, ownership_marker, created_at) \
             VALUES ('root-1', '/x', 'm', ?)",
        )
        .bind(NOW)
        .execute(db.pool())
        .await?;
        sqlx::query(
            "INSERT INTO mappings (id, repository_id, root_id, gitlab_account_id, namespace_id, \
             intended_project_path, mirror_relative_path) \
             VALUES ('map-1', 'r000', 'root-1', 'acc-gl', 'gl-ns-1', 'x-gh1', '.gbm/mirrors/gh-1.git')",
        )
        .execute(db.pool())
        .await?;
        let sync = NewJob {
            mapping_id: Some("map-1"),
            ..job("sync-1", Some("r000"), "k-sync")
        };
        // Another job (no repository) writing to the same destination mapping.
        let other = NewJob {
            mapping_id: Some("map-1"),
            kind: JobKind::Plan,
            ..job("plan-1", None, "k-plan")
        };
        db.enqueue_job(&sync).await?;
        db.enqueue_job(&other).await?;
        // The pure scheduler starts only one of them.
        assert_eq!(plan_round(&db, false, 0).await?, vec!["sync-1".to_owned()]);
        db.transition_job("sync-1", JobState::Running, "fetch", None, LATER)
            .await?;
        // The store refuses the second even if a caller ignores the scheduler.
        assert!(matches!(
            db.transition_job("plan-1", JobState::Running, "plan", None, LATER)
                .await,
            Err(StoreError::ActiveJobExists)
        ));

        db.transition_job("sync-1", JobState::Failed, "fetch", Some("TIMEOUT"), LATER)
            .await?;
        let not_before = "2026-01-01T00:10:00Z";
        db.enqueue_retry("sync-1", "sync-1-a2", not_before, LATER)
            .await?;
        let failed = db.get_job("sync-1").await?.ok_or("job")?;
        assert_eq!(
            (failed.state, failed.error_code.as_deref()),
            (JobState::Failed, Some("TIMEOUT")),
            "evidence kept"
        );
        let retry = db.get_job("sync-1-a2").await?.ok_or("retry")?;
        assert_eq!(
            (retry.attempt, retry.idempotency_key.as_str()),
            (2, "k-sync")
        );
        let due = rfc3339_to_ms(not_before).ok_or("time")?;
        // Before its time only plan-1 can start; at its time the retry still waits for
        // the shared mapping (FIFO: plan-1 is older).
        assert_eq!(
            plan_round(&db, false, due - 1).await?,
            vec!["plan-1".to_owned()]
        );
        assert_eq!(
            plan_round(&db, false, due).await?,
            vec!["plan-1".to_owned()]
        );
        db.transition_job("plan-1", JobState::Running, "plan", None, LATER)
            .await?;
        assert!(
            plan_round(&db, false, due).await?.is_empty(),
            "mapping busy"
        );
        db.transition_job("plan-1", JobState::Succeeded, "finalize", None, LATER)
            .await?;
        assert_eq!(
            plan_round(&db, false, due).await?,
            vec!["sync-1-a2".to_owned()]
        );
        // A repeated request for the same logical job finds the pending retry attempt.
        let again = NewJob {
            mapping_id: Some("map-1"),
            ..job("sync-1-dup", Some("r000"), "k-sync")
        };
        assert_eq!(
            db.enqueue_or_existing(&again).await?,
            Enqueued::Existing("sync-1-a2".to_owned())
        );
        // Only a failed attempt can be retried.
        assert!(matches!(
            db.enqueue_retry("sync-1-a2", "sync-1-a3", not_before, LATER)
                .await,
            Err(StoreError::InvalidTransition { .. })
        ));
        Ok(())
    }

    #[tokio::test]
    async fn store_rules_for_dedupe_retry_start_and_interrupted_locks() -> TestResult {
        let sandbox = Sandbox::new("db-store-rules")?;
        let (db, _) = Database::open(&db_path(&sandbox)).await?;
        seed_many(&db, 6).await?;

        // A logical job requested again after it finished gets a new linked attempt.
        assert!(matches!(
            db.enqueue_or_existing(&job("s-1", Some("r000"), "k-s"))
                .await?,
            Enqueued::New(_)
        ));
        db.transition_job("s-1", JobState::Running, "fetch", None, LATER)
            .await?;
        db.transition_job("s-1", JobState::Succeeded, "finalize", None, LATER)
            .await?;
        assert!(matches!(
            db.enqueue_or_existing(&job("s-2", Some("r000"), "k-s"))
                .await?,
            Enqueued::New(_)
        ));
        assert_eq!(db.get_job("s-2").await?.ok_or("job")?.attempt, 2);

        // A failed logical job is never re-run by a plain request (push-stage failure,
        // auth failure): the owner or the retry gate decides.
        for (id, stage, code, key) in [
            ("p-1", "push", "PUSH_PARTIAL", "k-p"),
            ("a-1", "observe_source", "AUTH_REQUIRED", "k-a"),
        ] {
            db.enqueue_job(&job(id, Some("r001"), key)).await?;
            db.transition_job(id, JobState::Running, "fetch", None, LATER)
                .await?;
            db.transition_job(id, JobState::Failed, stage, Some(code), LATER)
                .await?;
            for again in 0..3 {
                let request = job("again", Some("r001"), key);
                assert_eq!(
                    db.enqueue_or_existing(&request).await?,
                    Enqueued::NeedsOwner(id.to_owned()),
                    "{id} request {again}"
                );
            }
        }
        assert_eq!(
            count(&db, "SELECT count(*) FROM jobs WHERE id = 'again'").await?,
            0
        );

        // not_before must be canonical UTC.
        for bad in ["2026-01-01T00:10:00+07:00", "2026-01-01t00:10:00z"] {
            let invalid = NewJob {
                not_before: Some(bad),
                ..job("nb", Some("r001"), "k-nb")
            };
            assert!(matches!(
                db.enqueue_job(&invalid).await,
                Err(StoreError::InvalidInput("not_before"))
            ));
        }

        // Retries only for transient failures before any write.
        for (id, stage, code) in [
            ("f-push", "push", "TIMEOUT"),
            ("f-verify", "verify", "NETWORK_UNAVAILABLE"),
            ("f-auth", "fetch", "AUTH_REQUIRED"),
            ("f-partial", "fetch", "PUSH_PARTIAL"),
        ] {
            let repo = "r002";
            db.enqueue_job(&job(id, Some(repo), id)).await?;
            db.transition_job(id, JobState::Running, "fetch", None, LATER)
                .await?;
            db.transition_job(id, JobState::Failed, stage, Some(code), LATER)
                .await?;
            assert!(
                matches!(
                    db.enqueue_retry(id, &format!("{id}-2"), LATER, LATER).await,
                    Err(StoreError::InvalidInput("retry"))
                ),
                "{id}"
            );
        }

        // Global concurrency and pause are enforced by the store itself.
        for i in 3..6 {
            db.enqueue_job(&job(
                &format!("c-{i}"),
                Some(&format!("r00{i}")),
                &format!("kc{i}"),
            ))
            .await?;
        }
        assert!(matches!(
            db.start_job("c-3", "fetch", 2, true, LATER).await,
            Err(StoreError::QueuePaused)
        ));
        db.start_job("c-3", "fetch", 2, false, LATER).await?;
        db.start_job("c-4", "fetch", 2, false, LATER).await?;
        assert!(matches!(
            db.start_job("c-5", "fetch", 2, false, LATER).await,
            Err(StoreError::CapacityReached)
        ));

        // An interrupted (unreconciled) job keeps its repository locked.
        db.recover_interrupted_jobs(LATER).await?;
        db.enqueue_job(&job("after-crash", Some("r003"), "k-after"))
            .await?;
        assert!(matches!(
            db.start_job("after-crash", "fetch", 8, false, LATER).await,
            Err(StoreError::ActiveJobExists)
        ));
        assert!(
            db.active_jobs().await?.iter().any(|a| a.id == "c-3"),
            "interrupted jobs are reported as lock holders"
        );
        Ok(())
    }
}

mod reconcile_store {
    //! TEST-CRASH through the store: recover → load checkpoint → reconcile → close the
    //! interrupted attempt; the stale plan and approval expire and are never resumed.

    use gbm_application::destination::{CreateIntent, Visibility};
    use gbm_application::reconcile::{
        Checkpoint, CheckpointRecord, ClosedAs, Reconciliation, reconcile_interrupted,
    };

    use super::*;

    const AT: &str = "2026-01-01T00:07:00Z";

    async fn seed(db: &Database) -> TestResult {
        seed_repository(db, "repo-1").await?;
        db.upsert_account(&NewAccount {
            id: "acc-gl",
            provider: "gitlab",
            ..account("acc-gl", "77")
        })
        .await?;
        for sql in [
            "INSERT INTO roots (id, canonical_path, ownership_marker, created_at) \
             VALUES ('root-1', '/x', 'm', '2026-01-01T00:00:00Z')",
            "INSERT INTO mappings (id, repository_id, root_id, gitlab_account_id, namespace_id, \
             intended_project_path, mirror_relative_path) VALUES ('map-1', 'repo-1', 'root-1', \
             'acc-gl', 'gl-ns-1', 'x-gh1', '.gbm/mirrors/gh-1.git')",
            "INSERT INTO snapshots (id, repository_id, kind, status, digest, object_format, \
             created_at, coverage_json) VALUES ('snap-1', 'repo-1', 'source', 'ready', 'd', \
             'sha1', '2026-01-01T00:00:00Z', '{}')",
            "INSERT INTO plans (id, repository_id, source_snapshot_id, mapping_id, source_digest, \
             destination_digest, plan_digest, policy_revision, operations_json, \
             requires_approval, expires_at) VALUES ('plan-1', 'repo-1', 'snap-1', 'map-1', 's', \
             'd', 'p', 1, '[]', 1, '2026-01-01T00:10:00Z')",
            "INSERT INTO approvals (id, plan_id, plan_digest, policy_revision, approved_at, \
             expires_at) VALUES ('ap-1', 'plan-1', 'p', 1, '2026-01-01T00:01:00Z', \
             '2026-01-01T00:10:00Z')",
        ] {
            sqlx::query(sql).execute(db.pool()).await?;
        }
        Ok(())
    }

    async fn close(
        db: &Database,
        id: &str,
        decision: &Reconciliation,
        new_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let closing = decision.closing().ok_or("decision keeps the job")?;
        let state = match closing.state {
            ClosedAs::Succeeded => InterruptedClose::Succeeded,
            ClosedAs::Partial => InterruptedClose::Partial,
            ClosedAs::Failed => InterruptedClose::Failed,
        };
        let code = closing
            .error
            .map(serde_json::to_value)
            .transpose()?
            .and_then(|v| v.as_str().map(str::to_owned));
        db.resolve_interrupted(
            id,
            state,
            code.as_deref(),
            closing.new_attempt.then_some(new_id),
            AT,
        )
        .await?;
        Ok(())
    }

    async fn text(db: &Database, sql: &'static str) -> Result<String, Box<dyn std::error::Error>> {
        let (value,): (String,) = sqlx::query_as(sql).fetch_one(db.pool()).await?;
        Ok(value)
    }

    #[tokio::test]
    async fn a_push_killed_by_a_restart_never_resumes_its_plan_or_approval() -> TestResult {
        let sandbox = Sandbox::new("db-reconcile-push")?;
        let (db, _) = Database::open(&db_path(&sandbox)).await?;
        seed(&db).await?;
        db.enqueue_job(&NewJob {
            plan_id: Some("plan-1"),
            mapping_id: Some("map-1"),
            ..job("sync-1", Some("repo-1"), "k-sync")
        })
        .await?;
        let push = CheckpointRecord::Push {
            plan_id: "plan-1".to_owned(),
            snapshot_id: "snap-1".to_owned(),
        };
        // Only the running attempt records checkpoints.
        assert!(matches!(
            db.save_checkpoint("sync-1", push.kind(), &push.payload(), NOW)
                .await,
            Err(StoreError::InvalidTransition { .. })
        ));
        db.transition_job("sync-1", JobState::Running, "push", None, NOW)
            .await?;
        for (kind, payload) in [("argv", "{}"), ("push", ""), ("push", &"x".repeat(4097))] {
            assert!(matches!(
                db.save_checkpoint("sync-1", kind, payload, NOW).await,
                Err(StoreError::InvalidInput(_))
            ));
        }
        db.save_checkpoint("sync-1", push.kind(), &push.payload(), NOW)
            .await?;

        // Crash + restart.
        assert_eq!(db.recover_interrupted_jobs(LATER).await?, vec!["sync-1"]);
        let stored = db.checkpoint("sync-1").await?.ok_or("checkpoint lost")?;
        assert_eq!(
            CheckpointRecord::decode(&stored.kind, &stored.payload),
            Some(push)
        );
        // The interrupted row can neither be re-queued nor run directly.
        for to in [JobState::Queued, JobState::Running, JobState::Failed] {
            assert!(matches!(
                db.transition_job("sync-1", to, "push", None, AT).await,
                Err(StoreError::InvalidTransition { .. })
            ));
        }
        // The destination still equals the plan's start (decided in the application
        // tests): close it as a new attempt.
        close(&db, "sync-1", &Reconciliation::NewAttempt, "sync-2").await?;

        let old = db.get_job("sync-1").await?.ok_or("job")?;
        assert_eq!(
            (old.state, old.error_code.as_deref()),
            (JobState::Failed, Some("PLAN_STALE"))
        );
        let new = db.get_job("sync-2").await?.ok_or("job")?;
        assert_eq!(
            (new.state, new.attempt, new.stage.as_str()),
            (JobState::Queued, 2, "preflight")
        );
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM jobs WHERE id = 'sync-2' AND plan_id IS NULL"
            )
            .await?,
            1,
            "the next attempt plans again"
        );
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM job_checkpoints WHERE job_id = 'sync-2'"
            )
            .await?,
            0,
            "a push checkpoint is not carried over"
        );
        assert_eq!(
            text(&db, "SELECT expires_at FROM approvals WHERE id = 'ap-1'").await?,
            AT
        );
        assert_eq!(
            text(&db, "SELECT expires_at FROM plans WHERE id = 'plan-1'").await?,
            AT
        );
        // Locks are released: the new attempt can start.
        db.transition_job("sync-2", JobState::Running, "fetch", None, AT)
            .await?;
        // A closed attempt cannot be closed again.
        assert!(matches!(
            db.resolve_interrupted("sync-1", InterruptedClose::Failed, None, None, AT)
                .await,
            Err(StoreError::InvalidTransition { .. })
        ));
        Ok(())
    }

    #[tokio::test]
    async fn create_intents_carry_over_and_unobservable_jobs_escalate() -> TestResult {
        let sandbox = Sandbox::new("db-reconcile-create")?;
        let (db, _) = Database::open(&db_path(&sandbox)).await?;
        seed(&db).await?;
        let intent = CreateIntent {
            namespace_id: 7,
            namespace_full_path: "backups".to_owned(),
            path: "x-gh1".to_owned(),
            visibility: Visibility::Private,
            source_private: true,
            marker: "f3a9c1d2e4b5a6978877".to_owned(),
        };
        let create = CheckpointRecord::ProjectCreate(intent.clone());
        db.enqueue_job(&NewJob {
            mapping_id: Some("map-1"),
            ..job("c-1", Some("repo-1"), "k-c")
        })
        .await?;
        db.transition_job("c-1", JobState::Running, "preflight", None, NOW)
            .await?;
        db.save_checkpoint("c-1", create.kind(), &create.payload(), NOW)
            .await?;
        db.recover_interrupted_jobs(LATER).await?;
        let stored = db.checkpoint("c-1").await?.ok_or("checkpoint lost")?;
        let Some(CheckpointRecord::ProjectCreate(loaded)) =
            CheckpointRecord::decode(&stored.kind, &stored.payload)
        else {
            return Err("intent did not round-trip".into());
        };
        let decision =
            reconcile_interrupted("preflight", &Checkpoint::ProjectCreate(&loaded), None, 0);
        assert_eq!(decision, Reconciliation::RerunCreateIntent);
        close(&db, "c-1", &decision, "c-2").await?;
        // The new attempt re-runs the very same intent (same marker).
        let carried = db
            .checkpoint("c-2")
            .await?
            .ok_or("intent not carried over")?;
        assert_eq!(
            CheckpointRecord::decode(&carried.kind, &carried.payload),
            Some(CheckpointRecord::ProjectCreate(intent))
        );

        // A job stuck in a write stage without a checkpoint holds its locks only until
        // it is decided: here at once (uncertain), releasing them.
        db.transition_job("c-2", JobState::Running, "fetch", None, AT)
            .await?;
        db.transition_job("c-2", JobState::Succeeded, "finalize", None, AT)
            .await?;
        db.enqueue_job(&NewJob {
            mapping_id: Some("map-1"),
            ..job("v-1", Some("repo-1"), "k-v")
        })
        .await?;
        db.transition_job("v-1", JobState::Running, "verify", None, AT)
            .await?;
        db.recover_interrupted_jobs(AT).await?;
        db.enqueue_job(&NewJob {
            mapping_id: Some("map-1"),
            ..job("w-1", Some("repo-1"), "k-w")
        })
        .await?;
        assert!(matches!(
            db.transition_job("w-1", JobState::Running, "fetch", None, AT)
                .await,
            Err(StoreError::ActiveJobExists)
        ));
        let decision = reconcile_interrupted("verify", &Checkpoint::BeforeWrite, None, 0);
        assert_eq!(decision, Reconciliation::Uncertain);
        // A keep decision never closes anything.
        assert!(Reconciliation::KeepInterrupted.closing().is_none());
        close(&db, "v-1", &decision, "unused").await?;
        let v = db.get_job("v-1").await?.ok_or("job")?;
        assert_eq!(
            (v.state, v.error_code.as_deref()),
            (JobState::Failed, Some("VERIFY_FAILED"))
        );
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM jobs WHERE idempotency_key = 'k-v'"
            )
            .await?,
            1,
            "an uncertain outcome is not retried automatically"
        );
        db.transition_job("w-1", JobState::Running, "fetch", None, AT)
            .await?;
        // Only a failed close may queue a new attempt.
        db.recover_interrupted_jobs(AT).await?;
        assert!(matches!(
            db.resolve_interrupted("w-1", InterruptedClose::Partial, None, Some("w-2"), AT)
                .await,
            Err(StoreError::InvalidInput(_))
        ));
        Ok(())
    }

    #[tokio::test]
    async fn in_job_plans_cancels_and_partials_stay_owner_decisions() -> TestResult {
        let sandbox = Sandbox::new("db-reconcile-injob")?;
        let (db, _) = Database::open(&db_path(&sandbox)).await?;
        seed(&db).await?;
        // The plan was built inside the job: only the push checkpoint names it.
        db.enqueue_job(&NewJob {
            mapping_id: Some("map-1"),
            ..job("j-1", Some("repo-1"), "k-j")
        })
        .await?;
        db.transition_job("j-1", JobState::Running, "push", None, NOW)
            .await?;
        let push = CheckpointRecord::Push {
            plan_id: "plan-1".to_owned(),
            snapshot_id: "snap-1".to_owned(),
        };
        db.save_checkpoint("j-1", push.kind(), &push.payload(), NOW)
            .await?;
        db.transition_job("j-1", JobState::CancelRequested, "push", None, NOW)
            .await?;
        db.recover_interrupted_jobs(LATER).await?;
        close(&db, "j-1", &Reconciliation::PartialNeedsReview, "unused").await?;
        assert_eq!(
            text(&db, "SELECT expires_at FROM approvals WHERE id = 'ap-1'").await?,
            AT,
            "the checkpoint's plan approval expires"
        );
        assert_eq!(
            text(&db, "SELECT expires_at FROM plans WHERE id = 'plan-1'").await?,
            AT
        );
        let j = db.get_job("j-1").await?.ok_or("job")?;
        assert_eq!(
            (j.state, j.error_code.as_deref()),
            (JobState::Partial, Some("CANCEL_UNCERTAIN")),
            "the pending cancel stays on record"
        );
        // A partially pushed logical job is not re-run by a plain request.
        assert_eq!(
            db.enqueue_or_existing(&NewJob {
                mapping_id: Some("map-1"),
                ..job("j-2", Some("repo-1"), "k-j")
            })
            .await?,
            Enqueued::NeedsOwner("j-1".to_owned())
        );

        // A new attempt is never duplicated next to a pending one of the same job.
        db.enqueue_job(&job("d-1", Some("repo-1"), "k-d")).await?;
        db.transition_job("d-1", JobState::Running, "fetch", None, AT)
            .await?;
        db.recover_interrupted_jobs(AT).await?;
        db.enqueue_job(&NewJob {
            attempt: 2,
            ..job("d-x", Some("repo-1"), "k-d")
        })
        .await?;
        close(&db, "d-1", &Reconciliation::NewAttempt, "d-2").await?;
        assert_eq!(db.get_job("d-2").await?, None);
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM jobs WHERE idempotency_key = 'k-d'"
            )
            .await?,
            2
        );
        Ok(())
    }
}
