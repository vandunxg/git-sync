//! Self-tests for the Git fixtures: the properties later sync/restore tests rely on.

use gbm_test_fixtures::{
    FixtureError, HOSTILE_REF_NAMES, Sandbox, empty_bare, lfs_repo, source_repo, workspaces,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn source_fixture_has_required_ref_shapes() -> TestResult {
    let sandbox = Sandbox::new("shapes")?;
    let repo = source_repo(&sandbox)?;
    let refs = sandbox.git(
        &repo.path,
        &["for-each-ref", "--format=%(refname) %(objecttype)"],
    )?;
    for expected in [
        "refs/heads/main commit",
        "refs/heads/feature commit",
        "refs/heads/release/1.x commit",
        "refs/heads/odd/naïve-ü commit",
        "refs/tags/v1.0 tag",
        "refs/tags/light commit",
        "refs/tags/moving commit",
    ] {
        assert!(
            refs.lines().any(|l| l == expected),
            "missing `{expected}` in:\n{refs}"
        );
    }
    assert_ne!(
        repo.feature_tip_before_rewrite,
        repo.feature_tip_after_rewrite
    );
    let ancestor = sandbox.git(
        &repo.path,
        &[
            "merge-base",
            "--is-ancestor",
            &repo.feature_tip_before_rewrite,
            &repo.feature_tip_after_rewrite,
        ],
    );
    assert!(ancestor.is_err(), "rewrite must be non-fast-forward");
    assert_ne!(repo.moved_tag_before, repo.moved_tag_after);
    assert_eq!(repo.moved_tag_after, repo.main_tip);
    Ok(())
}

#[test]
fn fixtures_are_deterministic() -> TestResult {
    let a = Sandbox::new("det-a")?;
    let b = Sandbox::new("det-b")?;
    let (ra, rb) = (source_repo(&a)?, source_repo(&b)?);
    assert_eq!(
        (
            ra.main_tip,
            ra.feature_tip_after_rewrite,
            ra.moved_tag_after
        ),
        (
            rb.main_tip,
            rb.feature_tip_after_rewrite,
            rb.moved_tag_after
        )
    );
    Ok(())
}

#[test]
fn git_environment_is_isolated_from_the_owner() -> TestResult {
    let sandbox = Sandbox::new("isolation")?;
    let repo = source_repo(&sandbox)?;
    let origins = sandbox.git(&repo.path, &["config", "--show-origin", "--list"])?;
    assert!(
        origins
            .lines()
            .all(|l| l.starts_with("file:.git/config") || l.starts_with("command line:")),
        "config must come only from the repo and the fixture command line:\n{origins}"
    );
    let helpers = sandbox.git(&repo.path, &["config", "--get-all", "credential.helper"])?;
    assert_eq!(
        helpers, "",
        "only the empty (reset) helper may be configured"
    );
    Ok(())
}

#[test]
fn empty_destination_has_no_refs() -> TestResult {
    let sandbox = Sandbox::new("empty")?;
    let bare = empty_bare(&sandbox, "dest.git")?;
    assert_eq!(sandbox.git(&bare, &["for-each-ref"])?, "");
    assert_eq!(
        sandbox.git(&bare, &["rev-parse", "--is-bare-repository"])?,
        "true"
    );
    Ok(())
}

#[test]
fn workspace_states_are_dirty_ahead_and_detached() -> TestResult {
    let sandbox = Sandbox::new("workspaces")?;
    let repo = source_repo(&sandbox)?;
    let ws = workspaces(&sandbox, &repo.path)?;
    let dirty = sandbox.git(&ws.dirty, &["status", "--porcelain"])?;
    assert!(
        dirty.contains(" M README.md") && dirty.contains("?? untracked.txt"),
        "{dirty}"
    );
    let ahead = sandbox.git(&ws.ahead, &["rev-list", "--count", "origin/main..main"])?;
    assert_eq!(ahead, "1");
    let head = sandbox.git(&ws.detached, &["symbolic-ref", "-q", "HEAD"]);
    assert!(
        head.is_err(),
        "detached workspace must not have a symbolic HEAD"
    );
    Ok(())
}

#[test]
fn git_rejects_the_hostile_names_it_can_detect() -> TestResult {
    let sandbox = Sandbox::new("hostile")?;
    let repo = source_repo(&sandbox)?;
    let mut accepted_by_git = Vec::new();
    for name in HOSTILE_REF_NAMES {
        if sandbox.git(&repo.path, &["check-ref-format", name]).is_ok() {
            accepted_by_git.push(*name);
        }
    }
    // Syntactically legal to Git but outside the backup scope (heads/tags only):
    // native ref-scope validation must reject these, not Git.
    assert_eq!(
        accepted_by_git,
        [
            "refs/heads/-option",
            "refs/pull/1/head",
            "refs/remotes/origin/main",
            "refs/notes/commits"
        ]
    );
    Ok(())
}

#[test]
fn lfs_fixture_reports_unavailability_consistently() -> TestResult {
    let sandbox = Sandbox::new("lfs-probe")?;
    let lfs_installed = sandbox.git(sandbox.path(), &["lfs", "version"]).is_ok();
    match lfs_repo(&sandbox) {
        Ok(_) => assert!(lfs_installed),
        Err(FixtureError::ToolUnavailable(tool)) => {
            assert!(!lfs_installed);
            assert_eq!(tool, "git-lfs");
        }
        Err(other) => return Err(other.into()),
    }
    Ok(())
}

/// Real LFS coverage. Ignored (visible as "ignored", never "ok") until git-lfs is
/// installed on the test host; run with `cargo test -- --ignored`.
#[test]
#[ignore = "BLOCKED: requires git-lfs (NEED-GIT-LFS)"]
fn lfs_fixture_tracks_payload_with_git_lfs() -> TestResult {
    let sandbox = Sandbox::new("lfs")?;
    let path = lfs_repo(&sandbox)?;
    assert_eq!(
        sandbox.git(&path, &["lfs", "ls-files", "--name-only"])?,
        "payload.bin"
    );
    Ok(())
}
