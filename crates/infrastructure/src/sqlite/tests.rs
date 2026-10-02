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
            to_version: 1,
            applied: vec![1],
            backup: None
        }
    );
    assert_eq!(SCHEMA_VERSION, 1);
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
            ("0002_description.sql", v2),
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
        (Some(1), 2, vec![2])
    );
    let backup = report.backup.ok_or("backup missing")?;
    assert!(backup.exists());

    // The backup is a complete, openable v1 database with the sample data.
    let pool = SqlitePoolOptions::new()
        .connect_with(connect_options(&backup)?)
        .await?;
    let (repos,): (i64,) = sqlx::query_as("SELECT count(*) FROM repositories")
        .fetch_one(&pool)
        .await?;
    let (max,): (i64,) = sqlx::query_as("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    assert_eq!((repos, max), (1, 1));
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
        &[("0001_initial.sql", MIGRATION_0001), ("0002_x.sql", broken)],
    )
    .await?;
    let error = open_with(&path, &m)
        .await
        .err()
        .ok_or("broken migration must fail")?;
    assert!(matches!(error, StoreError::MigrationFailed(_)), "{error}");

    let (db, report) = Database::open(&path).await?;
    assert_eq!(report.from_version, Some(1));
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
        1
    );
    db.close().await;

    let fixed = "CREATE TABLE partial_table (id TEXT);";
    let m = migrator(
        &sandbox.path().join("m-fixed"),
        &[("0001_initial.sql", MIGRATION_0001), ("0002_x.sql", fixed)],
    )
    .await?;
    assert_eq!(open_with(&path, &m).await?.applied, vec![2]);
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
            ("0002_future.sql", "CREATE TABLE future (id TEXT);"),
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
    assert!(allowed_transition(Interrupted, Queued));
    assert!(
        !allowed_transition(Interrupted, Running),
        "requires requeue after reconciliation"
    );
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
    db.transition_job("job-run", JobState::Queued, "preflight", None, LATER)
        .await?;
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
            ("0002_x.sql", "CREATE TABLE x (id TEXT);"),
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
