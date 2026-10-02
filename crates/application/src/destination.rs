//! GitLab destination preview, adoption assessment and reconciled creation
//! (REQ-04, REQ-41; DISCOVERY_AND_PROVIDERS.md "GitLab projects").
//!
//! - The owner chooses the namespace; each source gets one project at
//!   `<name>-gh<id>` (see `mapping`). A project found at that path is never taken over
//!   because of its name: it is shown as existing and can only be adopted explicitly.
//! - Visibility defaults to private. A private source is never mapped to a more visible
//!   destination: the request contract has no consent field, so V1 blocks it.
//! - Creation is reconciled. Before the create request the caller persists a
//!   `CreateIntent` with a random marker; the marker goes into the project description.
//!   Any uncertain or failed create is followed by a lookup of the exact path, and only
//!   a project in the intended namespace carrying that marker is accepted as ours.
//!   Absent after an uncertain create, the original error is returned (retry later,
//!   starting with the same lookup). No duplicate is ever created for one intent.

use std::collections::HashMap;
use std::future::Future;

use sha2::{Digest, Sha256};

use gbm_domain::identity::{GitHubRepoId, proposed_project_path};

use crate::discovery::DiscoveredRepository;
use crate::mapping::{BlockReason, DestinationNamespace, MappingDecision, MappingProposal};
use crate::providers::ProviderError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Visibility {
    Private,
    Internal,
    Public,
}

impl Visibility {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "private" => Some(Self::Private),
            "internal" => Some(Self::Internal),
            "public" => Some(Self::Public),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Internal => "internal",
            Self::Public => "public",
        }
    }
}

/// GitLab's Developer role: the minimum that can push to unprotected branches.
pub const DEVELOPER_ACCESS: u32 = 30;

/// A GitLab project as observed via the API (validated by the adapter: numeric IDs,
/// canonical HTTPS clone URL on the configured origin, no userinfo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectView {
    pub project_id: u64,
    pub namespace_id: u64,
    pub path_with_namespace: String,
    pub visibility: Visibility,
    pub clone_url: String,
    pub description: String,
    pub empty_repo: bool,
    /// Highest of project/group access level reported for the token user, if any.
    pub access_level: Option<u32>,
}

/// One create request, persisted by the caller *before* it is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateIntent {
    pub namespace_id: u64,
    pub namespace_full_path: String,
    /// Project path (last segment), e.g. `tool-gh42`.
    pub path: String,
    pub visibility: Visibility,
    /// Checked again here, but only as good as the caller's value: the job must take
    /// it from a fresh source observation (or the committed value, `true` when unknown)
    /// and must not default it to `false`.
    pub source_private: bool,
    /// Random, caller-generated token (`[A-Za-z0-9-]{16,64}`).
    pub marker: String,
}

impl CreateIntent {
    pub fn full_path(&self) -> String {
        format!("{}/{}", self.namespace_full_path, self.path)
    }

    /// Description carrying the marker. It names no source repository.
    pub fn description(&self) -> String {
        format!("Git Backup Manager backup. gbm-intent:{}", self.marker)
    }

    fn marker_is_valid(&self) -> bool {
        (16..=64).contains(&self.marker.len())
            && self
                .marker
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }
}

/// Read and create operations on GitLab. Implementations never follow redirects and
/// return `Ok(None)` only for a definite 404.
pub trait DestinationApi {
    fn find_by_path(
        &self,
        full_path: &str,
    ) -> impl Future<Output = Result<Option<ProjectView>, ProviderError>>;
    fn get_by_id(
        &self,
        project_id: u64,
    ) -> impl Future<Output = Result<Option<ProjectView>, ProviderError>>;
    /// `POST /projects` with name/path = `intent.path`, `namespace_id`, `visibility`,
    /// the marker description and `initialize_with_readme=false`.
    fn create(
        &self,
        intent: &CreateIntent,
    ) -> impl Future<Output = Result<ProjectView, ProviderError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestinationBlock {
    /// A project at the intended path that this intent did not create.
    Foreign {
        project_id: u64,
    },
    /// More visible than the source allows (or than the intent requested).
    VisibilityUnsafe,
    /// Our own project, but it already has content: review before use.
    NotEmpty {
        project_id: u64,
    },
    /// The token user cannot push to it (below Developer, or not reported).
    InsufficientAccess {
        project_id: u64,
    },
    /// Not in the chosen namespace.
    WrongNamespace {
        project_id: u64,
    },
    /// Already mapped to another source.
    AlreadyMapped {
        project_id: u64,
    },
    InvalidIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundProject {
    pub project_id: u64,
    pub path_with_namespace: String,
    pub clone_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnsureOutcome {
    Created(BoundProject),
    /// Found after an earlier or uncertain create: the project this intent created.
    Reconciled(BoundProject),
    Blocked(DestinationBlock),
}

/// The visibility a destination for this source may have. A private source may only
/// go to a private destination.
pub fn allowed_visibility(
    source_private: bool,
    requested: Visibility,
) -> Result<Visibility, DestinationBlock> {
    if source_private && requested != Visibility::Private {
        Err(DestinationBlock::VisibilityUnsafe)
    } else {
        Ok(requested)
    }
}

fn bound(project: &ProjectView) -> BoundProject {
    BoundProject {
        project_id: project.project_id,
        path_with_namespace: project.path_with_namespace.clone(),
        clone_url: project.clone_url.clone(),
    }
}

/// Decides whether a project at the intended path is the one `intent` created.
fn match_intent(
    project: &ProjectView,
    intent: &CreateIntent,
) -> Result<BoundProject, DestinationBlock> {
    let marker = format!("gbm-intent:{}", intent.marker);
    let ours = project.namespace_id == intent.namespace_id
        && project
            .path_with_namespace
            .eq_ignore_ascii_case(&intent.full_path())
        && project.description.split_whitespace().any(|w| w == marker);
    if !ours {
        return Err(DestinationBlock::Foreign {
            project_id: project.project_id,
        });
    }
    if project.visibility > intent.visibility {
        return Err(DestinationBlock::VisibilityUnsafe);
    }
    if !project.empty_repo {
        return Err(DestinationBlock::NotEmpty {
            project_id: project.project_id,
        });
    }
    Ok(bound(project))
}

fn decided(result: Result<BoundProject, DestinationBlock>, reconciled: bool) -> EnsureOutcome {
    match result {
        Ok(project) if reconciled => EnsureOutcome::Reconciled(project),
        Ok(project) => EnsureOutcome::Created(project),
        Err(block) => EnsureOutcome::Blocked(block),
    }
}

/// Creates the project for a persisted intent, or reconciles an earlier attempt.
/// Lookup first; create only when the exact path is definitely absent; after any
/// create failure other than an authentication error, look up again before reporting.
pub async fn ensure_project(
    api: &impl DestinationApi,
    intent: &CreateIntent,
) -> Result<EnsureOutcome, ProviderError> {
    if !intent.marker_is_valid() || intent.path.is_empty() || intent.path.contains('/') {
        return Ok(EnsureOutcome::Blocked(DestinationBlock::InvalidIntent));
    }
    if let Err(block) = allowed_visibility(intent.source_private, intent.visibility) {
        return Ok(EnsureOutcome::Blocked(block));
    }
    if let Some(existing) = api.find_by_path(&intent.full_path()).await? {
        return Ok(decided(match_intent(&existing, intent), true));
    }
    match api.create(intent).await {
        Ok(created) => Ok(decided(match_intent(&created, intent), false)),
        Err(error @ (ProviderError::InvalidOrExpired | ProviderError::MalformedToken)) => {
            Err(error)
        }
        Err(error) => match api.find_by_path(&intent.full_path()).await {
            Ok(Some(found)) => Ok(decided(match_intent(&found, intent), true)),
            // Still absent (or unknown): report the create failure; the next attempt
            // starts with the same lookup, so no duplicate can result.
            Ok(None) | Err(_) => Err(error),
        },
    }
}

/// Whether an existing project could be adopted for this source by an explicit owner
/// choice (never automatically).
pub fn assess_adoption(
    project: &ProjectView,
    namespace_id: u64,
    source_private: bool,
    mapped_project_ids: &[u64],
) -> Result<(), DestinationBlock> {
    let id = project.project_id;
    if mapped_project_ids.contains(&id) {
        return Err(DestinationBlock::AlreadyMapped { project_id: id });
    }
    if project.namespace_id != namespace_id {
        return Err(DestinationBlock::WrongNamespace { project_id: id });
    }
    if source_private && project.visibility != Visibility::Private {
        return Err(DestinationBlock::VisibilityUnsafe);
    }
    if project
        .access_level
        .is_none_or(|level| level < DEVELOPER_ACCESS)
    {
        return Err(DestinationBlock::InsufficientAccess { project_id: id });
    }
    Ok(())
}

/// Stable warning codes shown in the preview (the UI maps them to text).
pub mod warning {
    pub const VISIBILITY_BLOCKED: &str = "visibility_private_source_blocked";
    pub const EXISTING_NOT_OURS: &str = "existing_project_requires_adoption";
    pub const ADOPTION_BLOCKED: &str = "existing_project_cannot_be_adopted";
    pub const EXISTING_NOT_EMPTY: &str = "existing_project_not_empty";
    pub const PATH_MAPPED: &str = "path_mapped_to_other_source";
    pub const LOCAL_PATH_TAKEN: &str = "local_path_taken";
    pub const INVALID_SOURCE: &str = "invalid_source";
    pub const ALREADY_MAPPED: &str = "already_mapped";
    pub const STATE_UNKNOWN: &str = "destination_state_unknown";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewItem {
    pub repository_id: String,
    pub intended_path: String,
    pub existing_project_id: Option<String>,
    pub needs_creation: bool,
    pub can_adopt: bool,
    pub warnings: Vec<String>,
    /// Source privacy used for the decision (`true` when unknown).
    pub source_private: bool,
    /// Present only for a creatable item: what a commit with `allowCreateMissing`
    /// would persist.
    pub proposal: Option<MappingProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationPreview {
    pub items: Vec<PreviewItem>,
    pub visibility: Visibility,
    pub digest: String,
}

/// Exact-path observations for the preview: repository ID -> project at its intended
/// path (`None` = definitely absent; no entry = could not be determined). Built by
/// `observe_intended_paths`.
pub type PathObservations = HashMap<String, Option<ProjectView>>;

/// Looks up the intended path of every proposal (read-only). A lookup that is uncertain
/// for one item (redirect route, unexpected payload, transient error) leaves that item
/// unobserved, hence not creatable; authentication and rate-limit errors abort.
pub async fn observe_intended_paths(
    api: &impl DestinationApi,
    decisions: &[MappingDecision],
    namespace_full_path: &str,
) -> Result<PathObservations, ProviderError> {
    let mut seen = HashMap::new();
    for decision in decisions {
        let (id, path) = match decision {
            MappingDecision::Proposed(p) => (&p.provider_repository_id, &p.intended_project_path),
            MappingDecision::Blocked {
                provider_repository_id,
                reason: BlockReason::ProjectPathForeign { path, .. },
            } => (provider_repository_id, path),
            _ => continue,
        };
        match api
            .find_by_path(&format!("{namespace_full_path}/{path}"))
            .await
        {
            Ok(found) => {
                seen.insert(id.clone(), found);
            }
            Err(
                error @ (ProviderError::InvalidOrExpired
                | ProviderError::MalformedToken
                | ProviderError::RateLimited { .. }),
            ) => return Err(error),
            Err(_) => {}
        }
    }
    Ok(seen)
}

/// Builds the preview. Pure; `observations` must cover every proposed item (an item
/// without an observation is not creatable).
pub fn build_preview(
    decisions: &[MappingDecision],
    repos: &[DiscoveredRepository],
    observations: &PathObservations,
    namespace: &DestinationNamespace,
    namespace_numeric_id: u64,
    mapped_project_ids: &[u64],
    requested: Visibility,
) -> DestinationPreview {
    let private: HashMap<&str, bool> = repos
        .iter()
        .map(|r| (r.provider_repository_id.as_str(), r.is_private))
        .collect();
    // Intended path for items blocked before a path was assigned (schema: non-empty).
    let proposed: HashMap<&str, String> = repos
        .iter()
        .filter_map(|r| {
            GitHubRepoId::parse(&r.provider_repository_id)
                .ok()
                .map(|id| {
                    (
                        r.provider_repository_id.as_str(),
                        proposed_project_path(&r.name, id),
                    )
                })
        })
        .collect();
    let items: Vec<PreviewItem> = decisions
        .iter()
        .map(|decision| {
            let mut item = preview_item(
                decision,
                &private,
                &proposed,
                observations,
                namespace_numeric_id,
                mapped_project_ids,
                requested,
            );
            item.source_private = private
                .get(item.repository_id.as_str())
                .copied()
                .unwrap_or(true);
            item
        })
        .collect();
    let digest = preview_digest(namespace, requested, &items);
    DestinationPreview {
        items,
        visibility: requested,
        digest,
    }
}

fn blocked_item(id: &str, path: &str, existing: Option<u64>, warnings: &[&str]) -> PreviewItem {
    PreviewItem {
        repository_id: id.to_owned(),
        intended_path: path.to_owned(),
        existing_project_id: existing.map(|v| v.to_string()),
        needs_creation: false,
        can_adopt: false,
        warnings: warnings.iter().map(|w| (*w).to_owned()).collect(),
        source_private: true,
        proposal: None,
    }
}

fn existing_item(
    id: &str,
    path: &str,
    project: &ProjectView,
    source_private: bool,
    namespace_numeric_id: u64,
    mapped: &[u64],
) -> PreviewItem {
    let adoptable = assess_adoption(project, namespace_numeric_id, source_private, mapped);
    let mut warnings = vec![warning::EXISTING_NOT_OURS];
    if adoptable.is_err() {
        warnings.push(warning::ADOPTION_BLOCKED);
    }
    if !project.empty_repo {
        warnings.push(warning::EXISTING_NOT_EMPTY);
    }
    let mut item = blocked_item(id, path, Some(project.project_id), &warnings);
    item.can_adopt = adoptable.is_ok();
    item
}

fn preview_item(
    decision: &MappingDecision,
    private: &HashMap<&str, bool>,
    proposed: &HashMap<&str, String>,
    observations: &PathObservations,
    namespace_numeric_id: u64,
    mapped: &[u64],
    requested: Visibility,
) -> PreviewItem {
    match decision {
        MappingDecision::Existing { mapping, .. } => PreviewItem {
            repository_id: mapping.provider_repository_id.clone(),
            intended_path: mapping.intended_project_path.clone(),
            existing_project_id: mapping.project_id.clone(),
            needs_creation: false,
            can_adopt: false,
            warnings: vec![warning::ALREADY_MAPPED.to_owned()],
            source_private: true,
            proposal: None,
        },
        MappingDecision::Blocked {
            provider_repository_id: id,
            reason,
        } => {
            let fallback = |id: &str| proposed.get(id).map_or("-", String::as_str);
            // Unknown privacy is treated as private.
            let source_private = private.get(id.as_str()).copied().unwrap_or(true);
            match reason {
                BlockReason::ProjectPathForeign { path, .. } => match observations.get(id) {
                    Some(Some(project)) => existing_item(
                        id,
                        path,
                        project,
                        source_private,
                        namespace_numeric_id,
                        mapped,
                    ),
                    _ => blocked_item(id, path, None, &[warning::STATE_UNKNOWN]),
                },
                BlockReason::ProjectPathMapped { path } => {
                    blocked_item(id, path, None, &[warning::PATH_MAPPED])
                }
                BlockReason::LocalPathTaken { .. } => {
                    blocked_item(id, fallback(id), None, &[warning::LOCAL_PATH_TAKEN])
                }
                BlockReason::InvalidSourceId | BlockReason::DuplicateSource => {
                    blocked_item(id, fallback(id), None, &[warning::INVALID_SOURCE])
                }
            }
        }
        MappingDecision::Proposed(proposal) => {
            let id = proposal.provider_repository_id.as_str();
            let path = proposal.intended_project_path.as_str();
            let source_private = private.get(id).copied().unwrap_or(true);
            if allowed_visibility(source_private, requested).is_err() {
                return blocked_item(id, path, None, &[warning::VISIBILITY_BLOCKED]);
            }
            match observations.get(id) {
                // Definitely absent: creatable.
                Some(None) => PreviewItem {
                    repository_id: id.to_owned(),
                    intended_path: path.to_owned(),
                    existing_project_id: None,
                    needs_creation: true,
                    can_adopt: false,
                    warnings: Vec::new(),
                    source_private,
                    proposal: Some(proposal.clone()),
                },
                Some(Some(project)) => existing_item(
                    id,
                    path,
                    project,
                    source_private,
                    namespace_numeric_id,
                    mapped,
                ),
                None => blocked_item(id, path, None, &[warning::STATE_UNKNOWN]),
            }
        }
    }
}

/// SHA-256 over a canonical, length-prefixed encoding of everything a commit would act
/// on, so any change to the preview changes the digest.
pub fn preview_digest(
    namespace: &DestinationNamespace,
    visibility: Visibility,
    items: &[PreviewItem],
) -> String {
    let mut hasher = Sha256::new();
    let mut field = |value: &str| {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    };
    field("gbm-destination-preview-v1");
    field(&namespace.gitlab_account_id);
    field(&namespace.namespace_id);
    field(visibility.as_str());
    for item in items {
        field(&item.repository_id);
        field(&item.intended_path);
        field(item.existing_project_id.as_deref().unwrap_or("-"));
        field(if item.needs_creation {
            "create"
        } else {
            "no-create"
        });
        field(if item.can_adopt {
            "adoptable"
        } else {
            "not-adoptable"
        });
        field(&item.warnings.join(","));
        field(if item.source_private {
            "source-private"
        } else {
            "source-public"
        });
        match &item.proposal {
            Some(p) => {
                field(&p.mirror_relative_path.to_string());
                field(
                    &p.workspace_relative_path
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                );
            }
            None => field("-"),
        }
    }
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitError {
    Expired,
    DigestMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedProposal {
    pub proposal: MappingProposal,
    /// Persist with the mapping; a later create must not use a less private value
    /// than this unless the source is observed again.
    pub source_private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedMappings {
    pub proposals: Vec<CommittedProposal>,
    pub visibility: Visibility,
    /// Persisted with each mapping; a job may create the project only when set.
    pub allow_create_missing: bool,
}

/// Validates a commit against the natively stored preview (digest + expiry; instants
/// in milliseconds) and returns the mappings to persist. Creation happens later, in a
/// job, through `ensure_project`. Existing projects are never adopted by a commit.
pub fn commit_preview(
    preview: &DestinationPreview,
    expected_digest: &str,
    now_ms: i64,
    expires_ms: i64,
    allow_create_missing: bool,
) -> Result<CommittedMappings, CommitError> {
    if now_ms >= expires_ms {
        return Err(CommitError::Expired);
    }
    if preview.digest != expected_digest {
        return Err(CommitError::DigestMismatch);
    }
    Ok(CommittedMappings {
        proposals: preview
            .items
            .iter()
            .filter(|item| item.needs_creation)
            .filter_map(|item| {
                item.proposal.clone().map(|proposal| CommittedProposal {
                    proposal,
                    source_private: item.source_private,
                })
            })
            .collect(),
        visibility: preview.visibility,
        allow_create_missing,
    })
}

#[cfg(test)]
mod tests;
