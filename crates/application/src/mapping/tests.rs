use gbm_domain::identity::IdentityError;

use super::*;
use crate::discovery::Relation;

fn repo(id: &str, owner: &str, name: &str) -> DiscoveredRepository {
    DiscoveredRepository {
        provider_repository_id: id.to_owned(),
        owner: owner.to_owned(),
        name: name.to_owned(),
        full_name: format!("{owner}/{name}"),
        default_branch: Some("main".to_owned()),
        is_private: true,
        is_fork: false,
        is_archived: false,
        relation: Relation::Owner,
        pushed_at: None,
    }
}

fn namespace() -> DestinationNamespace {
    DestinationNamespace {
        gitlab_account_id: "acc-gl".to_owned(),
        namespace_id: "77".to_owned(),
    }
}

fn stored(id: &str, full_name: &str, project_path: &str) -> Result<StoredMapping, IdentityError> {
    let repo_id = GitHubRepoId::parse(id)?;
    let (owner, name) = full_name.split_once('/').unwrap_or((full_name, full_name));
    Ok(StoredMapping {
        provider_repository_id: id.to_owned(),
        gitlab_account_id: "acc-gl".to_owned(),
        namespace_id: "77".to_owned(),
        intended_project_path: project_path.to_owned(),
        project_id: Some(format!("p{id}")),
        mirror_relative_path: mirror_relative_path(repo_id),
        workspace_relative_path: Some(workspace_relative_path(owner, name, repo_id)),
        full_name: full_name.to_owned(),
    })
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn new_sources_get_id_bound_proposals() -> TestResult {
    let decisions = preview_mappings(
        &[repo("10", "Alice", "Tools")],
        &[],
        &[],
        &namespace(),
        true,
    );
    let [MappingDecision::Proposed(p)] = decisions.as_slice() else {
        return Err(format!("{decisions:?}").into());
    };
    assert_eq!(p.intended_project_path, "tools-gh10");
    assert_eq!(p.mirror_relative_path.to_string(), ".gbm/mirrors/gh-10.git");
    assert_eq!(
        p.workspace_relative_path.as_ref().map(ToString::to_string),
        Some("workspace/alice/Tools-10".to_owned())
    );
    let without = preview_mappings(&[repo("10", "a", "t")], &[], &[], &namespace(), false);
    assert!(matches!(
        without.as_slice(),
        [MappingDecision::Proposed(MappingProposal {
            workspace_relative_path: None,
            ..
        })]
    ));
    Ok(())
}

#[test]
fn rename_and_transfer_keep_the_stored_mapping() -> Result<(), IdentityError> {
    // TEST-IDENTITY: same provider ID, new owner and name -> same mapping, no new
    // destination, no move; only a display-change flag and a hint.
    let existing = stored("10", "alice/tools", "tools-gh10")?;
    let decisions = preview_mappings(
        &[repo("10", "acme", "toolbox")],
        std::slice::from_ref(&existing),
        &[],
        &namespace(),
        true,
    );
    assert_eq!(
        decisions,
        vec![MappingDecision::Existing {
            mapping: existing,
            display_changed: true,
            suggested_project_path: "toolbox-gh10".to_owned(),
        }]
    );
    Ok(())
}

#[test]
fn a_new_source_reusing_an_old_name_gets_its_own_mapping() -> TestResult {
    // Repo 10 was renamed away from "tools"; a different repo 11 now named "tools"
    // must not inherit repo 10's mapping, folders or project.
    let existing = stored("10", "alice/tools", "tools-gh10")?;
    let decisions = preview_mappings(
        &[repo("11", "alice", "tools")],
        std::slice::from_ref(&existing),
        &[ObservedProject {
            project_id: "p10".to_owned(),
            path: "tools-gh10".to_owned(),
        }],
        &namespace(),
        true,
    );
    let [MappingDecision::Proposed(p)] = decisions.as_slice() else {
        return Err(format!("{decisions:?}").into());
    };
    assert_eq!(p.intended_project_path, "tools-gh11");
    assert_ne!(p.mirror_relative_path, existing.mirror_relative_path);
    assert_ne!(p.workspace_relative_path, existing.workspace_relative_path);
    Ok(())
}

#[test]
fn case_insensitive_project_clashes_are_blocked_not_adopted() -> Result<(), IdentityError> {
    // A foreign project already holds the path (different case): never adopt it.
    let foreign = preview_mappings(
        &[repo("12", "alice", "Tools")],
        &[],
        &[ObservedProject {
            project_id: "999".to_owned(),
            path: "TOOLS-GH12".to_owned(),
        }],
        &namespace(),
        false,
    );
    assert_eq!(
        foreign,
        vec![MappingDecision::Blocked {
            provider_repository_id: "12".to_owned(),
            reason: BlockReason::ProjectPathForeign {
                path: "tools-gh12".to_owned(),
                project_id: "999".to_owned(),
            },
        }]
    );
    // Another source's mapping holds the path (corrupt/legacy state): blocked too.
    let mut odd = stored("13", "bob/x", "Tools-GH12")?;
    odd.project_id = None;
    let mapped = preview_mappings(
        &[repo("12", "alice", "tools")],
        &[odd],
        &[],
        &namespace(),
        false,
    );
    assert!(matches!(
        mapped.as_slice(),
        [MappingDecision::Blocked {
            reason: BlockReason::ProjectPathMapped { .. },
            ..
        }]
    ));
    Ok(())
}

#[test]
fn local_path_clashes_are_blocked() -> Result<(), IdentityError> {
    // A stored mapping whose workspace path happens to equal the new suggestion
    // (case-insensitively) must not be shared.
    let mut odd = stored("20", "x/y", "y-gh20")?;
    odd.workspace_relative_path = Some(RelativePath::parse("workspace/Alice/tools-21")?);
    let decisions = preview_mappings(
        &[repo("21", "alice", "tools")],
        &[odd],
        &[],
        &namespace(),
        true,
    );
    assert_eq!(
        decisions,
        vec![MappingDecision::Blocked {
            provider_repository_id: "21".to_owned(),
            reason: BlockReason::LocalPathTaken {
                path: "workspace/alice/tools-21".to_owned(),
            },
        }]
    );
    Ok(())
}

#[test]
fn colliding_names_in_one_batch_stay_distinct() {
    let decisions = preview_mappings(
        &[
            repo("30", "Org", "Repo"),
            repo("31", "org", "repo"),
            repo("32", "org", "re po"),
            repo("30", "Org", "Repo"),
            repo("0030", "Org", "Repo"),
        ],
        &[],
        &[],
        &namespace(),
        true,
    );
    let proposals: Vec<&MappingProposal> = decisions
        .iter()
        .filter_map(|d| match d {
            MappingDecision::Proposed(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(proposals.len(), 3);
    let keys: HashSet<String> = proposals
        .iter()
        .map(|p| project_path_key(&p.intended_project_path))
        .collect();
    assert_eq!(keys.len(), 3);
    assert!(matches!(
        decisions[3],
        MappingDecision::Blocked {
            reason: BlockReason::DuplicateSource,
            ..
        }
    ));
    assert!(matches!(
        decisions[4],
        MappingDecision::Blocked {
            reason: BlockReason::InvalidSourceId,
            ..
        }
    ));
}

#[test]
fn our_own_mapped_project_is_not_reported_as_foreign() -> Result<(), IdentityError> {
    let existing = stored("40", "a/b", "b-gh40")?;
    let decisions = preview_mappings(
        &[repo("40", "a", "b")],
        std::slice::from_ref(&existing),
        &[ObservedProject {
            project_id: "p40".to_owned(),
            path: "b-gh40".to_owned(),
        }],
        &namespace(),
        true,
    );
    assert!(matches!(
        decisions.as_slice(),
        [MappingDecision::Existing {
            display_changed: false,
            ..
        }]
    ));
    Ok(())
}

#[test]
fn a_mapped_project_renamed_in_gitlab_still_blocks_its_new_path() -> TestResult {
    // Review finding: project p1 (mapped to source 1, stored path a-gh1) was renamed in
    // GitLab to b-gh2; source 2 named "b" must not be proposed onto that path.
    let existing = stored("1", "x/a", "a-gh1")?;
    let decisions = preview_mappings(
        &[repo("2", "x", "b")],
        &[existing],
        &[ObservedProject {
            project_id: "p1".to_owned(),
            path: "b-gh2".to_owned(),
        }],
        &namespace(),
        false,
    );
    assert_eq!(
        decisions,
        vec![MappingDecision::Blocked {
            provider_repository_id: "2".to_owned(),
            reason: BlockReason::ProjectPathMapped {
                path: "b-gh2".to_owned(),
            },
        }]
    );
    Ok(())
}

#[test]
fn a_path_mapped_in_another_namespace_of_the_account_is_blocked() -> TestResult {
    // Matches the mappings table's UNIQUE(gitlab_account_id, intended_project_path).
    let mut elsewhere = stored("3", "x/y", "tools-gh4")?;
    elsewhere.namespace_id = "other".to_owned();
    let decisions = preview_mappings(
        &[repo("4", "x", "tools")],
        &[elsewhere],
        &[],
        &namespace(),
        false,
    );
    assert!(matches!(
        decisions.as_slice(),
        [MappingDecision::Blocked {
            reason: BlockReason::ProjectPathMapped { .. },
            ..
        }]
    ));
    Ok(())
}
