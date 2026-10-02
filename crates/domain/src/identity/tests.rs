use super::*;

fn id(text: &str) -> Result<GitHubRepoId, IdentityError> {
    GitHubRepoId::parse(text)
}

#[test]
fn source_ids_are_canonical_decimal_only() -> Result<(), IdentityError> {
    assert_eq!(id("123456")?.get(), 123_456);
    assert_eq!(id("18446744073709551615")?.get(), u64::MAX);
    for bad in [
        "",
        "0",
        "007",
        "-1",
        "+1",
        " 1",
        "1 ",
        "1e3",
        "12a",
        "18446744073709551616",
        "１２",
    ] {
        assert_eq!(id(bad), Err(IdentityError::InvalidSourceId), "{bad:?}");
    }
    Ok(())
}

#[test]
fn storage_paths_embed_the_provider_id() -> Result<(), IdentityError> {
    let repo = id("42")?;
    assert_eq!(
        mirror_relative_path(repo).to_string(),
        ".gbm/mirrors/gh-42.git"
    );
    assert_eq!(
        snapshots_relative_dir(repo).to_string(),
        ".gbm/snapshots/gh-42"
    );
    assert_eq!(
        workspace_relative_path("Octo-Org", "Hello.World", repo).to_string(),
        "workspace/octo-org/Hello.World-42"
    );
    Ok(())
}

#[test]
fn rename_and_transfer_keep_the_mirror_identity() -> Result<(), IdentityError> {
    // TEST-IDENTITY: the mirror path depends only on the provider ID.
    let repo = id("9001")?;
    let before = workspace_relative_path("alice", "tools", repo);
    let after = workspace_relative_path("acme", "tools-renamed", repo);
    assert_ne!(before, after, "workspace suggestion follows display names");
    assert_eq!(mirror_relative_path(repo), mirror_relative_path(repo));
    assert_eq!(
        proposed_project_path("tools", repo),
        "tools-gh9001",
        "slug suffix is the stable id"
    );
    Ok(())
}

#[test]
fn case_and_sanitize_collisions_stay_distinct_by_id() -> Result<(), IdentityError> {
    // TEST-IDENTITY: names that collide case-insensitively or after sanitizing never
    // derive the same local path or project path for different sources.
    let a = id("1")?;
    let b = id("2")?;
    let pa = workspace_relative_path("Org", "Repo", a);
    let pb = workspace_relative_path("org", "repo", b);
    assert_ne!(pa.collision_key(), pb.collision_key());
    assert!(!pa.overlaps(&pb));
    assert_ne!(
        project_path_key(&proposed_project_path("Repo", a)),
        project_path_key(&proposed_project_path("repo", b))
    );
    let sa = proposed_project_path("my repo", a);
    let sb = proposed_project_path("my_repo", b);
    assert_eq!(sa, "my-repo-gh1");
    assert_eq!(sb, "my-repo-gh2");
    Ok(())
}

#[test]
fn hostile_display_names_are_neutralized() -> Result<(), IdentityError> {
    let repo = id("7")?;
    for (name, leaf) in [
        ("..", "repo-7"),
        (".", "repo-7"),
        ("", "repo-7"),
        ("../../etc", "_..-..-etc-7"),
        (".hidden", "_.hidden-7"),
        ("a/b", "a-b-7"),
        ("a\\b", "a-b-7"),
        ("x\0y", "x-y-7"),
        ("CON", "_CON-7"),
        ("com1.txt", "_com1.txt-7"),
        ("ünï", "-n--7"),
    ] {
        let path = workspace_relative_path("owner", name, repo);
        assert_eq!(path.components()[2], leaf, "{name:?}");
        assert!(RelativePath::parse(&path.to_string()).is_ok());
    }
    let long = "x".repeat(400);
    let path = workspace_relative_path(&long, &long, repo);
    assert!(path.components().iter().all(|c| c.len() <= 255));
    assert!(path.components()[2].ends_with("-7"));
    Ok(())
}

#[test]
fn project_paths_follow_gitlab_rules() -> Result<(), IdentityError> {
    let repo = id("31337")?;
    for (name, expected) in [
        ("Hello.World", "hello-world-gh31337"),
        ("--weird__name..git", "weird-name-git-gh31337"),
        ("...", "repo-gh31337"),
        ("日本", "repo-gh31337"),
        ("a.atom", "a-atom-gh31337"),
    ] {
        assert_eq!(proposed_project_path(name, repo), expected, "{name:?}");
    }
    let long = proposed_project_path(&"ab-".repeat(200), repo);
    assert!(long.len() <= 255);
    assert!(long.ends_with("-gh31337"));
    assert!(!long.contains("--"));
    Ok(())
}

#[test]
fn stored_relative_paths_reject_traversal_and_absolutes() {
    for bad in [
        "",
        "/etc/passwd",
        "a//b",
        "a/",
        "../x",
        "a/../b",
        "a/./b",
        "a\\b",
        "a/b c",
        "a/\u{0}",
        "a/ü",
    ] {
        assert_eq!(
            RelativePath::parse(bad),
            Err(IdentityError::UnsafeRelativePath),
            "{bad:?}"
        );
    }
    assert!(RelativePath::parse(&"x".repeat(256)).is_err());
    assert!(RelativePath::parse(".gbm/mirrors/gh-1.git").is_ok());
}

#[test]
fn overlap_detects_nesting_and_case_variants() -> Result<(), IdentityError> {
    let parent = RelativePath::parse("workspace/Org")?;
    let child = RelativePath::parse("workspace/org/repo-1")?;
    let sibling = RelativePath::parse("workspace/org2/repo-1")?;
    assert!(parent.overlaps(&child));
    assert!(child.overlaps(&parent));
    assert!(!child.overlaps(&sibling));
    Ok(())
}

#[test]
fn stored_paths_must_have_the_expected_shape() -> Result<(), IdentityError> {
    let repo = id("55")?;
    assert_eq!(
        parse_mirror_path(".gbm/mirrors/gh-55.git")?,
        (mirror_relative_path(repo), repo)
    );
    for bad in [
        ".gbm",
        "workspace",
        ".gbm/mirrors",
        ".gbm/mirrors/gh-055.git",
        ".gbm/mirrors/gh-55",
        ".gbm/snapshots/gh-55.git",
        "x/.gbm/mirrors/gh-55.git",
    ] {
        assert!(parse_mirror_path(bad).is_err(), "{bad:?}");
    }
    let ws = workspace_relative_path("Alice", "tools", repo);
    assert_eq!(parse_workspace_path(&ws.to_string(), repo)?, ws);
    for bad in [
        "workspace",
        "workspace/alice",
        "workspace/alice/tools-56",
        "workspace/alice/-55",
        "other/alice/tools-55",
        "workspace/alice/tools-55/deeper",
    ] {
        assert!(parse_workspace_path(bad, repo).is_err(), "{bad:?}");
    }
    Ok(())
}
