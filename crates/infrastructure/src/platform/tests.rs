use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn temp_base(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let base = std::env::temp_dir().join(format!("gbm-platform-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base)?;
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700))?;
    Ok(base)
}

#[test]
fn creates_private_runtime_dir_with_mode_0700() -> TestResult {
    let base = temp_base("create")?;
    let dir = PrivateRuntimeDir::under(&base)?;
    let meta = fs::metadata(dir.path())?;
    assert_eq!(meta.permissions().mode() & 0o7777, 0o700);
    // Idempotent on a second call.
    assert_eq!(PrivateRuntimeDir::under(&base)?, dir);
    fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn rejects_group_readable_existing_dir() -> TestResult {
    let base = temp_base("perm")?;
    let dir = base.join(RUNTIME_SUBDIR);
    fs::create_dir(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o750))?;
    assert_eq!(
        PrivateRuntimeDir::under(&base),
        Err(PlatformError::RuntimeDirInsecure)
    );
    fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn rejects_symlinked_runtime_dir() -> TestResult {
    let base = temp_base("link")?;
    let target = base.join("elsewhere");
    fs::create_dir(&target)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700))?;
    std::os::unix::fs::symlink(&target, base.join(RUNTIME_SUBDIR))?;
    assert_eq!(
        PrivateRuntimeDir::under(&base),
        Err(PlatformError::RuntimeDirInsecure)
    );
    fs::remove_dir_all(base)?;
    Ok(())
}

#[test]
fn rejects_relative_or_missing_base() {
    assert_eq!(
        PrivateRuntimeDir::under(Path::new("relative")),
        Err(PlatformError::RuntimeDirUnavailable)
    );
    assert_eq!(
        PrivateRuntimeDir::under(Path::new("/nonexistent-gbm-runtime")),
        Err(PlatformError::RuntimeDirUnavailable)
    );
}

#[test]
fn ancestry_follows_live_process_tree() -> TestResult {
    let me = std::process::id();
    let mut child = Command::new("sleep").arg("5").spawn()?;
    let child_pid = child.id();
    assert!(
        is_same_or_descendant(child_pid, me),
        "child descends from this process"
    );
    assert!(is_same_or_descendant(me, me));
    assert!(
        !is_same_or_descendant(me, child_pid),
        "parent is not a descendant of its child"
    );
    assert!(
        !is_same_or_descendant(me, 1),
        "init is never an accepted binding"
    );
    child.kill()?;
    child.wait()?;
    Ok(())
}

#[test]
fn parent_pid_handles_parenthesized_comm() {
    assert!(parent_pid(std::process::id()).is_some());
    assert_eq!(parent_pid(u32::MAX), None);
}

#[test]
fn uptime_parsing_is_strict() {
    assert_eq!(parse_uptime_ms("12345.67 54321.00\n"), Some(12_345_670));
    assert_eq!(parse_uptime_ms("5 1"), Some(5_000));
    for bad in ["", "abc 1", "-1.0 2", "1.2x 3", ". 1", "1. 2", "1.+5 2"] {
        assert_eq!(parse_uptime_ms(bad), None, "{bad:?}");
    }
}

#[test]
fn no_suspend_is_detected_while_awake() -> TestResult {
    let mut detector = SuspendDetector::start()?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    // /proc/uptime has 10 ms resolution; allow for it.
    assert!(detector.suspended_ms()? < 1_000);
    assert!(
        !detector.resumed()?,
        "awake time is never reported as a resume"
    );
    Ok(())
}
