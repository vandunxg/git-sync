//! Source-to-destination mapping preview (REQ-04, REQ-05, REQ-40).
//!
//! A mapping is bound to the immutable provider repository ID. An existing mapping is
//! always kept as stored: a rename or transfer only updates display metadata and may
//! surface a new suggestion, never a filesystem move or a new destination. A new
//! mapping is only *proposed*; any clash with another mapping, a project observed in
//! the namespace, or another proposal blocks it for review. Nothing is adopted because
//! its name matches.

use std::collections::{HashMap, HashSet};

use gbm_domain::identity::{
    GitHubRepoId, RelativePath, mirror_relative_path, project_path_key, proposed_project_path,
    workspace_relative_path,
};

use crate::discovery::DiscoveredRepository;

/// The owner-chosen GitLab destination namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationNamespace {
    pub gitlab_account_id: String,
    pub namespace_id: String,
}

/// A persisted mapping (from the `mappings` table joined with its repository).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMapping {
    pub provider_repository_id: String,
    pub gitlab_account_id: String,
    pub namespace_id: String,
    pub intended_project_path: String,
    pub project_id: Option<String>,
    pub mirror_relative_path: RelativePath,
    pub workspace_relative_path: Option<RelativePath>,
    /// Source `owner/name` when the mapping was last confirmed.
    pub full_name: String,
}

/// A project path already present in the chosen destination namespace (observed via
/// API). The caller passes only projects of that namespace. Any path match blocks a
/// new proposal: a project mapped to another source keeps its ID across a GitLab-side
/// rename, so its stored intended path alone cannot prove the path is free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedProject {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingProposal {
    pub provider_repository_id: String,
    pub namespace: DestinationNamespace,
    pub intended_project_path: String,
    pub mirror_relative_path: RelativePath,
    pub workspace_relative_path: Option<RelativePath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockReason {
    /// The provider ID is not a canonical numeric ID.
    InvalidSourceId,
    /// The same source appears twice in the input.
    DuplicateSource,
    /// Another source's mapping already uses this project path (case-insensitive).
    ProjectPathMapped { path: String },
    /// A project with this path exists but is not ours: explicit adoption required.
    ProjectPathForeign { path: String, project_id: String },
    /// Another mapping or proposal already owns (or nests with) this local path.
    LocalPathTaken { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingDecision {
    /// Mapping kept exactly as stored. `display_changed` is set when the source was
    /// renamed or transferred; `suggested_project_path` differs from the stored path
    /// only as an informational hint (no automatic rename of the destination).
    Existing {
        mapping: StoredMapping,
        display_changed: bool,
        suggested_project_path: String,
    },
    Proposed(MappingProposal),
    Blocked {
        provider_repository_id: String,
        reason: BlockReason,
    },
}

/// Builds the preview for `repos` into `namespace`. Pure: performs no I/O, so the
/// caller can show it before any filesystem or GitLab write.
pub fn preview_mappings(
    repos: &[DiscoveredRepository],
    stored: &[StoredMapping],
    observed: &[ObservedProject],
    namespace: &DestinationNamespace,
    with_workspace: bool,
) -> Vec<MappingDecision> {
    let by_source: HashMap<&str, &StoredMapping> = stored
        .iter()
        .map(|m| (m.provider_repository_id.as_str(), m))
        .collect();
    let mapped_project_ids: HashSet<&str> = stored
        .iter()
        .filter_map(|m| m.project_id.as_deref())
        .collect();
    // Project paths claimed per GitLab account, keyed case-insensitively. This matches
    // `UNIQUE(gitlab_account_id, intended_project_path)` in the mappings table, which
    // is stricter than GitLab's per-namespace uniqueness.
    let mut claimed_paths: HashSet<(String, String)> = stored
        .iter()
        .map(|m| {
            (
                m.gitlab_account_id.clone(),
                project_path_key(&m.intended_project_path),
            )
        })
        .collect();
    let mut local_paths: Vec<RelativePath> = stored
        .iter()
        .flat_map(|m| {
            std::iter::once(m.mirror_relative_path.clone()).chain(m.workspace_relative_path.clone())
        })
        .collect();
    let mut seen = HashSet::new();
    let mut decisions = Vec::with_capacity(repos.len());

    for repo in repos {
        let source = repo.provider_repository_id.clone();
        let blocked = |reason| MappingDecision::Blocked {
            provider_repository_id: source.clone(),
            reason,
        };
        let Ok(id) = GitHubRepoId::parse(&source) else {
            decisions.push(blocked(BlockReason::InvalidSourceId));
            continue;
        };
        if !seen.insert(id) {
            decisions.push(blocked(BlockReason::DuplicateSource));
            continue;
        }
        let suggested = proposed_project_path(&repo.name, id);
        if let Some(mapping) = by_source.get(source.as_str()) {
            decisions.push(MappingDecision::Existing {
                mapping: (*mapping).clone(),
                display_changed: mapping.full_name != repo.full_name,
                suggested_project_path: suggested,
            });
            continue;
        }

        let key = (
            namespace.gitlab_account_id.clone(),
            project_path_key(&suggested),
        );
        if claimed_paths.contains(&key) {
            decisions.push(blocked(BlockReason::ProjectPathMapped { path: suggested }));
            continue;
        }
        if let Some(project) = observed.iter().find(|p| project_path_key(&p.path) == key.1) {
            let reason = if mapped_project_ids.contains(project.project_id.as_str()) {
                BlockReason::ProjectPathMapped { path: suggested }
            } else {
                BlockReason::ProjectPathForeign {
                    path: suggested,
                    project_id: project.project_id.clone(),
                }
            };
            decisions.push(blocked(reason));
            continue;
        }
        let mirror = mirror_relative_path(id);
        let workspace =
            with_workspace.then(|| workspace_relative_path(&repo.owner, &repo.name, id));
        let clash = std::iter::once(&mirror)
            .chain(workspace.as_ref())
            .find(|candidate| local_paths.iter().any(|taken| taken.overlaps(candidate)));
        if let Some(path) = clash {
            decisions.push(blocked(BlockReason::LocalPathTaken {
                path: path.to_string(),
            }));
            continue;
        }

        claimed_paths.insert(key);
        local_paths.push(mirror.clone());
        local_paths.extend(workspace.clone());
        decisions.push(MappingDecision::Proposed(MappingProposal {
            provider_repository_id: source,
            namespace: namespace.clone(),
            intended_project_path: suggested,
            mirror_relative_path: mirror,
            workspace_relative_path: workspace,
        }));
    }
    decisions
}

#[cfg(test)]
mod tests;
