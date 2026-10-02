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
