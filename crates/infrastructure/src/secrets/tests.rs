//! TEST-KEYRING / TEST-REDACTION for the credential stores.
//!
//! Keyring scenarios never touch the owner's keyring: each test re-executes this test
//! binary inside `dbus-run-session` with a cleared environment, a temporary HOME and a
//! fresh gnome-keyring unlocked with a throwaway password. Missing tools => the test
//! fails loudly with a BLOCKED message instead of passing silently.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use gbm_application::credentials::{
    AccountDirectory, CredentialError, CredentialRef, CredentialStore, StorageMode,
    disconnect_account, store_credential,
};
use gbm_domain::contract::secret::SecretString;
use gbm_test_fixtures::Sandbox;

use super::keyring::map_error;
use super::{KeyringStore, SessionStore};

const SCENARIO_ENV: &str = "GBM_KEYRING_SCENARIO";
const SENTINEL: &str = "glpat-TESTSENTINEL-KEYRING-0001";

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn tool_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

/// Runs `test_path` again inside an isolated Secret Service session.
fn run_isolated(test_path: &str, scenario: &str, setup: &str) -> TestResult {
    for tool in [
        "dbus-run-session",
        "gnome-keyring-daemon",
        "gdbus",
        "timeout",
    ] {
        if !tool_exists(tool) {
            return Err(format!("BLOCKED: {tool} not installed; keyring scenario not run").into());
        }
    }
    let sandbox = Sandbox::new("keyring")?;
    let runtime = sandbox.path().join("run");
    std::fs::create_dir(&runtime)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))?;
    }
    let exe = std::env::current_exe()?;
    let script = format!(
        "printf 'throwaway-test-password' | gnome-keyring-daemon --unlock --components=secrets >/dev/null && {setup} \
         exec \"$0\" --exact {test_path} --nocapture --test-threads=1"
    );
    let output = Command::new("timeout")
        .args(["90", "dbus-run-session", "--", "sh", "-c", &script])
        .arg(&exe)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", sandbox.path())
        .env("XDG_DATA_HOME", sandbox.path().join("data"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env(SCENARIO_ENV, scenario)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
        "sentinel leaked to output"
    );
    if !output.status.success() || !stdout.contains("1 passed") {
        return Err(format!("isolated scenario {scenario} failed:\n{stdout}\n{stderr}").into());
    }
    // No plaintext copy of the secret anywhere in the isolated HOME (gnome-keyring
    // stores items encrypted, so any plaintext hit would be a fallback/leak).
    let hits = plaintext_hits(sandbox.path())?;
    assert!(hits.is_empty(), "plaintext sentinel found in {hits:?}");
    Ok(())
}

fn plaintext_hits(dir: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            hits.extend(plaintext_hits(&path)?);
        } else if path.is_file()
            && std::fs::read(&path)?
                .windows(SENTINEL.len())
                .any(|w| w == SENTINEL.as_bytes())
        {
            hits.push(path.display().to_string());
        }
    }
    Ok(hits)
}

fn child_scenario() -> Option<String> {
    std::env::var(SCENARIO_ENV).ok()
}

/// The child must be running in the temporary environment, never the owner's session.
fn assert_isolated() -> TestResult {
    let temp = std::env::temp_dir();
    let home = std::env::var("HOME")?;
    let isolated_home = Path::new(&home).starts_with(&temp);
    // The available/locked scenarios run under dbus-run-session (own runtime dir and bus);
    // the unavailable scenario points the bus address into its sandbox.
    let runtime_ok =
        std::env::var("XDG_RUNTIME_DIR").map_or(true, |d| Path::new(&d).starts_with(&temp));
    let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
    let owner_bus = bus.contains("/run/user/");
    if !isolated_home || !runtime_ok || owner_bus {
        return Err("refusing to run a keyring scenario outside the isolated session".into());
    }
    Ok(())
}

struct TestDirectory {
    entry: Option<(CredentialRef, StorageMode)>,
    disconnected: Mutex<bool>,
}

impl AccountDirectory for TestDirectory {
    fn credential_of(
        &self,
        _: &str,
    ) -> Result<Option<(CredentialRef, StorageMode)>, CredentialError> {
        Ok(self.entry.clone())
    }
    fn mark_disconnected(&self, _: &str) -> Result<(), CredentialError> {
        *self
            .disconnected
            .lock()
            .map_err(|_| CredentialError::Backend)? = true;
        Ok(())
    }
}

#[test]
fn keyring_store_reconnect_and_disconnect() -> TestResult {
    if child_scenario().as_deref() != Some("available") {
        return run_isolated(
            "secrets::tests::keyring_store_reconnect_and_disconnect",
            "available",
            "",
        );
    }
    assert_isolated()?;
    let keyring = KeyringStore::connect()?;
    let reference = store_credential(&keyring, "acc-1", &SecretString::new(SENTINEL.to_owned()))?;
    // "Reconnect": a new connection (as after an app restart) reads the same secret.
    let reopened = KeyringStore::connect()?;
    assert_eq!(reopened.get(&reference)?.expose(), SENTINEL);
    // Replacement overwrites in place.
    reopened.put(
        &reference,
        &SecretString::new(format!("{SENTINEL}-rotated")),
    )?;
    assert_eq!(
        keyring.get(&reference)?.expose(),
        format!("{SENTINEL}-rotated")
    );
    let session = SessionStore::new();
    let directory = TestDirectory {
        entry: Some((reference.clone(), StorageMode::Keyring)),
        disconnected: Mutex::new(false),
    };
    disconnect_account(&keyring, &session, &directory, "acc-1")?;
    assert!(*directory.disconnected.lock().map_err(|_| "poisoned")?);
    assert!(matches!(
        keyring.get(&reference),
        Err(CredentialError::NotFound)
    ));
    keyring.delete(&reference)?; // idempotent
    Ok(())
}

#[test]
fn locked_keyring_is_reported_without_fallback() -> TestResult {
    if child_scenario().as_deref() != Some("locked") {
        // Lock the isolated login collection; with no display the unlock prompt fails.
        let lock = "gdbus call --session --dest org.freedesktop.secrets \
                    --object-path /org/freedesktop/secrets \
                    --method org.freedesktop.Secret.Service.Lock \
                    \"['/org/freedesktop/secrets/collection/login']\" >/dev/null &&";
        return run_isolated(
            "secrets::tests::locked_keyring_is_reported_without_fallback",
            "locked",
            lock,
        );
    }
    assert_isolated()?;
    let keyring = KeyringStore::connect()?;
    let outcome = store_credential(&keyring, "acc-1", &SecretString::new(SENTINEL.to_owned()));
    assert_eq!(outcome, Err(CredentialError::KeyringLocked));
    // No-fallback evidence: the parent scans the isolated HOME for any plaintext copy.
    Ok(())
}

#[test]
fn missing_secret_service_is_unavailable() -> TestResult {
    if child_scenario().as_deref() != Some("unavailable") {
        let exe = std::env::current_exe()?;
        let sandbox = Sandbox::new("keyring-none")?;
        let output = Command::new(exe)
            .args([
                "--exact",
                "secrets::tests::missing_secret_service_is_unavailable",
                "--nocapture",
            ])
            .env_clear()
            .env("HOME", sandbox.path())
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}/no-bus", sandbox.path().display()),
            )
            .env(SCENARIO_ENV, "unavailable")
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{stdout}"
        );
        return Ok(());
    }
    assert_isolated()?;
    assert!(matches!(
        KeyringStore::connect(),
        Err(CredentialError::KeyringUnavailable)
    ));
    Ok(())
}

#[test]
fn session_store_is_memory_only_and_explicit() -> TestResult {
    let session = SessionStore::new();
    assert_eq!(session.mode(), StorageMode::SessionOnly);
    let reference = store_credential(&session, "acc-1", &SecretString::new(SENTINEL.to_owned()))?;
    assert_eq!(session.get(&reference)?.expose(), SENTINEL);
    assert_eq!(format!("{session:?}"), "SessionStore(1 entries)");
    session.delete(&reference)?;
    assert!(matches!(
        session.get(&reference),
        Err(CredentialError::NotFound)
    ));
    // A new store (= app restart) has nothing: session credentials do not persist.
    assert!(matches!(
        SessionStore::new().get(&reference),
        Err(CredentialError::NotFound)
    ));
    Ok(())
}

#[test]
fn keyring_errors_are_mapped_without_secret_bytes() {
    let platform =
        || -> keyring_core::error::PlatformError { Box::new(std::io::Error::other(SENTINEL)) };
    let cases = [
        (
            keyring_core::Error::BadEncoding(SENTINEL.as_bytes().to_vec()),
            CredentialError::Backend,
        ),
        (
            keyring_core::Error::BadDataFormat(SENTINEL.as_bytes().to_vec(), platform()),
            CredentialError::Backend,
        ),
        (
            keyring_core::Error::NoStorageAccess(platform()),
            CredentialError::KeyringLocked,
        ),
        (
            keyring_core::Error::PlatformFailure(platform()),
            CredentialError::KeyringUnavailable,
        ),
        (keyring_core::Error::NoEntry, CredentialError::NotFound),
        (
            keyring_core::Error::TooLong("password".into(), 1),
            CredentialError::InvalidSecret,
        ),
    ];
    for (error, expected) in cases {
        let mapped = map_error(error);
        assert_eq!(mapped, expected);
        assert!(!format!("{mapped} {mapped:?}").contains(SENTINEL));
    }
    let keyring_debug = format!("{:?}", SessionStore::new());
    assert!(!keyring_debug.contains(SENTINEL));
}
