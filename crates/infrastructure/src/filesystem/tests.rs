//! TEST-ROOT (filesystem half) and TEST-IDENTITY (local paths) against real
//! temporary directories. Roots are set up through the production `storage` module.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use gbm_domain::identity::{GitHubRepoId, mirror_relative_path, workspace_relative_path};

use super::*;
use crate::storage::{establish, validate_candidate};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A fresh established root plus a sibling "outside" folder for escape attempts.
struct Scratch {
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let base = std::env::temp_dir().join(format!("gbm-fs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir_all(&root)?;
        fs::create_dir_all(&outside)?;
        let candidate = validate_candidate(&root)?;
        let established = establish(&candidate, "root-1", "2026-10-02T00:00:00Z")?;
        let root = established.root().path().to_path_buf();
        let outside = fs::canonicalize(outside)?;
        Ok(Self {
            base,
            root,
            outside,
        })
    }

    fn owned(&self) -> Result<OwnedRoot, FsError> {
        OwnedRoot::verify(&self.root, "root-1")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Restore permissions changed by a test before removing.
        for dir in [self.root.join(APP_DIR), self.root.clone()] {
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn id(n: &str) -> Result<GitHubRepoId, Box<dyn std::error::Error>> {
    Ok(GitHubRepoId::parse(n)?)
}

#[test]
fn creates_private_mirror_path_once_and_reopens_it() -> TestResult {
    let s = Scratch::new("create")?;
    let root = s.owned()?;
    let rel = mirror_relative_path(id("1")?);
    let made = root.create_dir(&rel)?;
    assert_eq!(made, s.root.join(".gbm/mirrors/gh-1.git"));
    for dir in [made.clone(), s.root.join(".gbm/mirrors")] {
        assert_eq!(fs::metadata(&dir)?.permissions().mode() & 0o777, 0o700);
    }
    assert_eq!(root.open_dir(&rel)?, made);
    // Create-new semantics: the same leaf is never "created" twice.
    assert_eq!(root.create_dir(&rel), Err(FsError::ForeignPath));
    Ok(())
}

#[test]
fn pre_existing_folders_with_the_expected_name_are_not_adopted() -> TestResult {
    let s = Scratch::new("foreign")?;
    let rel = workspace_relative_path("alice", "tools", id("2")?);
    let foreign = s.root.join(rel.to_string());
    fs::create_dir_all(&foreign)?;
    fs::write(foreign.join("notes.txt"), "user data")?;
    assert_eq!(s.owned()?.create_dir(&rel), Err(FsError::ForeignPath));
    assert_eq!(fs::read_to_string(foreign.join("notes.txt"))?, "user data");
    Ok(())
}

#[test]
fn symlink_escapes_are_refused_and_outside_stays_untouched() -> TestResult {
    let s = Scratch::new("symlink")?;
    let root = s.owned()?;
    // Intermediate component is a symlink to a folder outside the root.
    symlink(&s.outside, s.root.join(".gbm/mirrors"))?;
    assert_eq!(
        root.create_dir(&mirror_relative_path(id("3")?)),
        Err(FsError::Escape)
    );
    // Leaf symlink for an already-mapped path.
    fs::create_dir(s.root.join("workspace"))?;
    fs::create_dir(s.root.join("workspace/alice"))?;
    symlink(&s.outside, s.root.join("workspace/alice/tools-3"))?;
    let rel = workspace_relative_path("alice", "tools", id("3")?);
    assert_eq!(root.open_dir(&rel), Err(FsError::Escape));
    assert_eq!(root.create_dir(&rel), Err(FsError::Escape));
    assert_eq!(
        fs::read_dir(&s.outside)?.count(),
        0,
        "nothing written outside"
    );
    Ok(())
}

#[test]
fn case_variant_siblings_are_refused() -> TestResult {
    let s = Scratch::new("case")?;
    fs::create_dir_all(s.root.join("workspace/alice/Tools-4"))?;
    let rel = workspace_relative_path("alice", "tools", id("4")?);
    assert_eq!(s.owned()?.create_dir(&rel), Err(FsError::CaseCollision));
    assert!(!s.root.join("workspace/alice/tools-4").exists());
    Ok(())
}

#[test]
fn unwritable_root_reports_not_writable_and_writes_nothing() -> TestResult {
    let s = Scratch::new("ro")?;
    if current_uid()? == 0 {
        return Ok(()); // root bypasses mode bits; covered by non-root CI runs.
    }
    let root = s.owned()?;
    fs::set_permissions(s.root.join(APP_DIR), fs::Permissions::from_mode(0o500))?;
    assert_eq!(
        root.create_dir(&mirror_relative_path(id("5")?)),
        Err(FsError::NotWritable)
    );
    assert!(!s.root.join(".gbm/mirrors").exists());
    Ok(())
}

#[test]
fn files_in_the_way_and_missing_mapped_folders_are_reported() -> TestResult {
    let s = Scratch::new("file")?;
    let root = s.owned()?;
    fs::write(s.root.join("workspace"), "not a dir")?;
    let rel = workspace_relative_path("alice", "tools", id("6")?);
    assert_eq!(root.create_dir(&rel), Err(FsError::NotADirectory));
    assert_eq!(
        root.open_dir(&mirror_relative_path(id("6")?)),
        Err(FsError::Missing)
    );
    assert!(!s.root.join(".gbm/mirrors").exists(), "open never creates");
    Ok(())
}

#[test]
fn only_a_verified_manifest_makes_a_root_mutable() -> TestResult {
    let s = Scratch::new("verify")?;
    // Wrong root id, relative path, non-canonical (symlinked) path.
    assert_eq!(
        OwnedRoot::verify(&s.root, "root-2"),
        Err(FsError::NotOwnedRoot)
    );
    assert_eq!(
        OwnedRoot::verify(Path::new("root"), "root-1"),
        Err(FsError::NotOwnedRoot)
    );
    let alias = s.base.join("alias");
    symlink(&s.root, &alias)?;
    assert_eq!(
        OwnedRoot::verify(&alias, "root-1"),
        Err(FsError::NotOwnedRoot)
    );

    // Manifest replaced by a symlink to a lookalike outside the root.
    let manifest = s.root.join(APP_DIR).join(MANIFEST_FILE);
    let lookalike = s.outside.join("m.json");
    fs::copy(&manifest, &lookalike)?;
    fs::remove_file(&manifest)?;
    symlink(&lookalike, &manifest)?;
    assert_eq!(s.owned(), Err(FsError::NotOwnedRoot));

    // Manifest from another app, and a world-readable `.gbm`.
    fs::remove_file(&manifest)?;
    fs::write(&manifest, r#"{"app":"other","rootId":"root-1"}"#)?;
    assert_eq!(s.owned(), Err(FsError::NotOwnedRoot));
    fs::write(
        &manifest,
        r#"{"app":"git-backup-manager","rootId":"root-1"}"#,
    )?;
    assert!(s.owned().is_ok());
    fs::set_permissions(s.root.join(APP_DIR), fs::Permissions::from_mode(0o755))?;
    assert_eq!(s.owned(), Err(FsError::NotOwnedRoot));
    fs::set_permissions(s.root.join(APP_DIR), fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&s.root, fs::Permissions::from_mode(0o777))?;
    assert_eq!(s.owned(), Err(FsError::NotOwnedRoot), "world-writable root");
    fs::set_permissions(&s.root, fs::Permissions::from_mode(0o755))?;
    assert!(s.owned().is_ok());

    // A folder never established (no manifest at all).
    let plain = s.base.join("plain");
    fs::create_dir(&plain)?;
    assert_eq!(
        OwnedRoot::verify(&fs::canonicalize(&plain)?, "root-1"),
        Err(FsError::NotOwnedRoot)
    );
    Ok(())
}

#[test]
fn distinct_sources_never_share_a_folder() -> TestResult {
    // TEST-IDENTITY: names colliding case-insensitively still map to separate folders.
    let s = Scratch::new("identity")?;
    let root = s.owned()?;
    let a = root.create_dir(&workspace_relative_path("Org", "Repo", id("7")?))?;
    let b = root.create_dir(&workspace_relative_path("org", "repo", id("8")?))?;
    assert_ne!(a, b);
    assert_eq!(
        a.parent(),
        b.parent(),
        "owner folder is shared, lower-cased"
    );
    Ok(())
}
