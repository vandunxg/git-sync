use std::fs;
use std::path::PathBuf;

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn scratch(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("gbm-root-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[test]
fn establishes_owned_root_once() -> TestResult {
    let dir = scratch("ok")?;
    let candidate = validate_candidate(&dir)?;
    let root = establish(&candidate, "root-1", "2026-10-02T00:00:00Z")?;
    let manifest = fs::read_to_string(root.root().path().join(APP_DIR).join(MANIFEST_FILE))?;
    assert!(manifest.contains("\"rootId\":\"root-1\""));
    // A folder that already holds a manifest is never adopted again.
    assert_eq!(validate_candidate(&dir), Err(RootError::AlreadyClaimed));
    assert_eq!(
        establish(&candidate, "root-2", "2026-10-02T00:00:00Z").err(),
        Some(RootError::AlreadyClaimed)
    );
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn rejects_system_relative_missing_and_git_paths() -> TestResult {
    assert_eq!(
        validate_candidate(Path::new("relative")),
        Err(RootError::NotAbsolute)
    );
    assert_eq!(
        validate_candidate(Path::new("/")),
        Err(RootError::SystemLocation)
    );
    assert_eq!(
        validate_candidate(Path::new("/etc")),
        Err(RootError::SystemLocation)
    );
    assert_eq!(
        validate_candidate(Path::new("/nonexistent-gbm")),
        Err(RootError::NotADirectory)
    );
    let dir = scratch("git")?;
    fs::create_dir_all(dir.join("repo/.git"))?;
    fs::create_dir_all(dir.join("repo/sub"))?;
    assert_eq!(
        validate_candidate(&dir.join("repo/sub")),
        Err(RootError::InsideGitRepository)
    );
    let file = dir.join("file");
    fs::write(&file, "x")?;
    assert_eq!(validate_candidate(&file), Err(RootError::NotADirectory));
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn symlinked_selection_is_stored_canonically() -> TestResult {
    let dir = scratch("link")?;
    let real = dir.join("real");
    fs::create_dir(&real)?;
    std::os::unix::fs::symlink(&real, dir.join("alias"))?;
    let candidate = validate_candidate(&dir.join("alias"))?;
    assert_eq!(candidate.path(), fs::canonicalize(&real)?);
    assert_eq!(
        candidate.display_path(Some(&fs::canonicalize(&dir)?)),
        "~/real"
    );
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn undo_restores_a_choosable_folder_and_lax_app_dir_is_refused() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("undo")?;
    let candidate = validate_candidate(&dir)?;
    establish(&candidate, "root-1", "2026-10-02T00:00:00Z")?.undo();
    assert!(!dir.join(APP_DIR).exists(), "created .gbm removed on undo");
    assert_eq!(
        validate_candidate(&dir)?,
        candidate,
        "folder can be chosen again"
    );
    fs::create_dir(dir.join(APP_DIR))?;
    fs::set_permissions(dir.join(APP_DIR), fs::Permissions::from_mode(0o755))?;
    assert_eq!(
        establish(&candidate, "root-2", "2026-10-02T00:00:00Z").err(),
        Some(RootError::AlreadyClaimed)
    );
    fs::remove_dir_all(dir)?;
    Ok(())
}
