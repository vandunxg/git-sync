//! TEST-PROJECT-CREATE / TEST-PRIVACY policy tests with a stateful in-memory GitLab
//! double (test-only); the HTTP adapter is tested in infrastructure/src/gitlab.

use std::cell::{Cell, RefCell};
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use gbm_domain::identity::{GitHubRepoId, mirror_relative_path};

use super::*;
use crate::discovery::Relation;
use crate::mapping::preview_mappings;

/// The double completes every future immediately, so one poll suffices.
fn run<T>(future: impl Future<Output = T>) -> Option<T> {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CreateBehaviour {
    Succeed,
    /// The project is created remotely, but the response is lost.
    TimeoutAfterSuccess,
    /// Nothing is created and the request fails.
    FailBeforeSuccess,
    Unauthorized,
}

struct FakeGitLab {
    projects: RefCell<Vec<ProjectView>>,
    behaviour: Cell<CreateBehaviour>,
    creates: Cell<u32>,
    next_id: Cell<u64>,
    lookup_fails: Cell<bool>,
}

impl FakeGitLab {
    fn new(projects: Vec<ProjectView>, behaviour: CreateBehaviour) -> Self {
        Self {
            projects: RefCell::new(projects),
            behaviour: Cell::new(behaviour),
            creates: Cell::new(0),
            next_id: Cell::new(1000),
            lookup_fails: Cell::new(false),
        }
    }

    fn count_at(&self, full_path: &str) -> usize {
        self.projects
            .borrow()
            .iter()
            .filter(|p| p.path_with_namespace.eq_ignore_ascii_case(full_path))
            .count()
    }
}

impl DestinationApi for FakeGitLab {
    async fn find_by_path(&self, full_path: &str) -> Result<Option<ProjectView>, ProviderError> {
        if self.lookup_fails.get() {
            return Err(ProviderError::Network);
        }
        Ok(self
            .projects
            .borrow()
            .iter()
            .find(|p| p.path_with_namespace.eq_ignore_ascii_case(full_path))
            .cloned())
    }

    async fn get_by_id(&self, project_id: u64) -> Result<Option<ProjectView>, ProviderError> {
        Ok(self
            .projects
            .borrow()
            .iter()
            .find(|p| p.project_id == project_id)
            .cloned())
    }

    async fn create(&self, intent: &CreateIntent) -> Result<ProjectView, ProviderError> {
        self.creates.set(self.creates.get() + 1);
        let behaviour = self.behaviour.get();
        if behaviour == CreateBehaviour::Unauthorized {
            return Err(ProviderError::InvalidOrExpired);
        }
        if behaviour == CreateBehaviour::FailBeforeSuccess {
            return Err(ProviderError::Timeout);
        }
        if self.count_at(&intent.full_path()) > 0 {
            return Err(ProviderError::UnexpectedResponse); // 400 path taken
        }
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let project = ProjectView {
            project_id: id,
            namespace_id: intent.namespace_id,
            path_with_namespace: intent.full_path(),
            visibility: intent.visibility,
            clone_url: format!("https://gitlab.com/{}.git", intent.full_path()),
            description: intent.description(),
            empty_repo: true,
            access_level: Some(50),
        };
        self.projects.borrow_mut().push(project.clone());
        if behaviour == CreateBehaviour::TimeoutAfterSuccess {
            return Err(ProviderError::Timeout);
        }
        Ok(project)
    }
}

const NS: u64 = 77;

fn intent(marker: &str) -> CreateIntent {
    CreateIntent {
        namespace_id: NS,
        namespace_full_path: "backups".to_owned(),
        path: "tool-gh42".to_owned(),
        visibility: Visibility::Private,
        source_private: true,
        marker: marker.to_owned(),
    }
}

const MARKER: &str = "f3a9c1d2e4b5a6978877";

fn foreign(id: u64, path: &str) -> ProjectView {
    ProjectView {
        project_id: id,
        namespace_id: NS,
        path_with_namespace: path.to_owned(),
        visibility: Visibility::Private,
        clone_url: format!("https://gitlab.com/{path}.git"),
        description: "someone else's project".to_owned(),
        empty_repo: false,
        access_level: Some(40),
    }
}

#[test]
fn create_then_rerun_reconciles_without_duplicate() -> Result<(), ProviderError> {
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::Succeed);
    let first = run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??;
    let EnsureOutcome::Created(created) = first else {
        return Err(ProviderError::UnexpectedResponse);
    };
    let again = run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??;
    assert_eq!(again, EnsureOutcome::Reconciled(created));
    assert_eq!(gitlab.creates.get(), 1);
    assert_eq!(gitlab.count_at("backups/tool-gh42"), 1);
    Ok(())
}

#[test]
fn a_timed_out_create_that_succeeded_remotely_is_reconciled_not_duplicated()
-> Result<(), ProviderError> {
    // TEST-PROJECT-CREATE (first half).
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::TimeoutAfterSuccess);
    let outcome = run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??;
    assert!(matches!(outcome, EnsureOutcome::Reconciled(ref p) if p.project_id == 1000));
    // A later retry (e.g. after restart) still finds it and never creates again.
    gitlab.behaviour.set(CreateBehaviour::Succeed);
    let retry = run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??;
    assert!(matches!(retry, EnsureOutcome::Reconciled(ref p) if p.project_id == 1000));
    assert_eq!(gitlab.creates.get(), 1);
    assert_eq!(gitlab.count_at("backups/tool-gh42"), 1);
    Ok(())
}

#[test]
fn an_unrelated_same_name_project_is_never_adopted() -> Result<(), ProviderError> {
    // TEST-PROJECT-CREATE (second half): same path, no marker of this intent.
    let gitlab = FakeGitLab::new(
        vec![foreign(5, "backups/Tool-GH42")],
        CreateBehaviour::Succeed,
    );
    let outcome = run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??;
    assert_eq!(
        outcome,
        EnsureOutcome::Blocked(DestinationBlock::Foreign { project_id: 5 })
    );
    assert_eq!(gitlab.creates.get(), 0);
    // A project created for a *different* intent at that path is also foreign.
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::Succeed);
    run(ensure_project(&gitlab, &intent("aaaaaaaaaaaaaaaaaaaa")))
        .ok_or(ProviderError::Network)??;
    let other = run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??;
    assert!(matches!(
        other,
        EnsureOutcome::Blocked(DestinationBlock::Foreign { .. })
    ));
    // A marker that only appears as a substring does not count.
    let mut lookalike = foreign(6, "backups/tool-gh42");
    lookalike.description = format!("xgbm-intent:{MARKER}");
    lookalike.empty_repo = true;
    let gitlab = FakeGitLab::new(vec![lookalike], CreateBehaviour::Succeed);
    let outcome = run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??;
    assert_eq!(
        outcome,
        EnsureOutcome::Blocked(DestinationBlock::Foreign { project_id: 6 })
    );
    Ok(())
}

#[test]
fn our_marker_in_another_namespace_or_with_content_is_not_accepted() -> Result<(), ProviderError> {
    let mut moved = foreign(7, "backups/tool-gh42");
    moved.description = intent(MARKER).description();
    moved.namespace_id = 99;
    moved.empty_repo = true;
    let gitlab = FakeGitLab::new(vec![moved.clone()], CreateBehaviour::Succeed);
    assert_eq!(
        run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??,
        EnsureOutcome::Blocked(DestinationBlock::Foreign { project_id: 7 })
    );
    moved.namespace_id = NS;
    moved.empty_repo = false;
    let gitlab = FakeGitLab::new(vec![moved.clone()], CreateBehaviour::Succeed);
    assert_eq!(
        run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??,
        EnsureOutcome::Blocked(DestinationBlock::NotEmpty { project_id: 7 })
    );
    moved.empty_repo = true;
    moved.visibility = Visibility::Public;
    let gitlab = FakeGitLab::new(vec![moved], CreateBehaviour::Succeed);
    assert_eq!(
        run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)??,
        EnsureOutcome::Blocked(DestinationBlock::VisibilityUnsafe)
    );
    Ok(())
}

#[test]
fn failures_are_reported_and_never_retried_blindly() -> Result<(), ProviderError> {
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::FailBeforeSuccess);
    assert_eq!(
        run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)?,
        Err(ProviderError::Timeout)
    );
    assert_eq!(
        gitlab.creates.get(),
        1,
        "one create per call, no inner retry"
    );
    // Lookup unavailable: nothing is created at all.
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::Succeed);
    gitlab.lookup_fails.set(true);
    assert_eq!(
        run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)?,
        Err(ProviderError::Network)
    );
    assert_eq!(gitlab.creates.get(), 0);
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::Unauthorized);
    assert_eq!(
        run(ensure_project(&gitlab, &intent(MARKER))).ok_or(ProviderError::Network)?,
        Err(ProviderError::InvalidOrExpired)
    );
    Ok(())
}

#[test]
fn private_sources_are_never_created_or_adopted_more_visible() -> Result<(), ProviderError> {
    // TEST-PRIVACY (destination part).
    let mut public = intent(MARKER);
    public.visibility = Visibility::Public;
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::Succeed);
    assert_eq!(
        run(ensure_project(&gitlab, &public)).ok_or(ProviderError::Network)??,
        EnsureOutcome::Blocked(DestinationBlock::VisibilityUnsafe)
    );
    assert_eq!(gitlab.creates.get(), 0);
    public.visibility = Visibility::Internal;
    assert!(matches!(
        run(ensure_project(&gitlab, &public)).ok_or(ProviderError::Network)??,
        EnsureOutcome::Blocked(DestinationBlock::VisibilityUnsafe)
    ));
    // A public source may be public when explicitly requested.
    public.source_private = false;
    public.visibility = Visibility::Public;
    assert!(matches!(
        run(ensure_project(&gitlab, &public)).ok_or(ProviderError::Network)??,
        EnsureOutcome::Created(_)
    ));
    assert_eq!(
        allowed_visibility(true, Visibility::Private),
        Ok(Visibility::Private)
    );

    let mut existing = foreign(8, "backups/tool-gh42");
    existing.visibility = Visibility::Internal;
    assert_eq!(
        assess_adoption(&existing, NS, true, &[]),
        Err(DestinationBlock::VisibilityUnsafe)
    );
    assert_eq!(assess_adoption(&existing, NS, false, &[]), Ok(()));
    Ok(())
}

#[test]
fn adoption_requires_namespace_access_and_an_unmapped_project() {
    let project = foreign(9, "backups/x");
    assert_eq!(assess_adoption(&project, NS, true, &[]), Ok(()));
    assert_eq!(
        assess_adoption(&project, NS, true, &[9]),
        Err(DestinationBlock::AlreadyMapped { project_id: 9 })
    );
    assert_eq!(
        assess_adoption(&project, 1, true, &[]),
        Err(DestinationBlock::WrongNamespace { project_id: 9 })
    );
    for level in [None, Some(20)] {
        let mut weak = project.clone();
        weak.access_level = level;
        assert_eq!(
            assess_adoption(&weak, NS, true, &[]),
            Err(DestinationBlock::InsufficientAccess { project_id: 9 })
        );
    }
}

#[test]
fn invalid_intents_are_rejected_before_any_request() -> Result<(), ProviderError> {
    let gitlab = FakeGitLab::new(Vec::new(), CreateBehaviour::Succeed);
    for bad in [intent("short"), intent("bad marker with spaces!")] {
        assert_eq!(
            run(ensure_project(&gitlab, &bad)).ok_or(ProviderError::Network)??,
            EnsureOutcome::Blocked(DestinationBlock::InvalidIntent)
        );
    }
    let mut nested = intent(MARKER);
    nested.path = "a/b".to_owned();
    assert_eq!(
        run(ensure_project(&gitlab, &nested)).ok_or(ProviderError::Network)??,
        EnsureOutcome::Blocked(DestinationBlock::InvalidIntent)
    );
    assert_eq!(gitlab.creates.get(), 0);
    Ok(())
}

fn repo(id: &str, name: &str, private: bool) -> DiscoveredRepository {
    DiscoveredRepository {
        provider_repository_id: id.to_owned(),
        owner: "octo".to_owned(),
        name: name.to_owned(),
        full_name: format!("octo/{name}"),
        default_branch: Some("main".to_owned()),
        is_private: private,
        is_fork: false,
        is_archived: false,
        relation: Relation::Owner,
        pushed_at: None,
    }
}

fn namespace() -> DestinationNamespace {
    DestinationNamespace {
        gitlab_account_id: "acc-gl".to_owned(),
        namespace_id: "gl-ns-77".to_owned(),
    }
}

#[test]
fn preview_marks_only_definitely_absent_paths_creatable() -> Result<(), ProviderError> {
    let repos = vec![
        repo("1", "free", true),
        repo("2", "taken", true),
        repo("3", "secret", true),
        repo("4", "open", false),
    ];
    let decisions = preview_mappings(&repos, &[], &[], &namespace(), false);
    // `taken-gh2` exists in GitLab but was not in the namespace listing.
    let gitlab = FakeGitLab::new(
        vec![foreign(50, "backups/taken-gh2")],
        CreateBehaviour::Succeed,
    );
    let observed = run(observe_intended_paths(&gitlab, &decisions, "backups"))
        .ok_or(ProviderError::Network)??;
    assert_eq!(gitlab.creates.get(), 0, "preview is read-only");

    let private_preview = build_preview(
        &decisions,
        &repos,
        &observed,
        &namespace(),
        NS,
        &[],
        Visibility::Private,
    );
    let items = &private_preview.items;
    assert!(items[0].needs_creation && items[0].proposal.is_some());
    assert_eq!(items[0].intended_path, "free-gh1");
    assert!(!items[1].needs_creation);
    assert_eq!(items[1].existing_project_id.as_deref(), Some("50"));
    assert!(items[1].can_adopt, "adoptable, but only by explicit choice");
    assert!(
        items[1]
            .warnings
            .contains(&warning::EXISTING_NOT_OURS.to_owned())
    );
    assert!(
        items[1]
            .warnings
            .contains(&warning::EXISTING_NOT_EMPTY.to_owned())
    );

    // Public requested: private sources are blocked, the public source is creatable.
    let public_preview = build_preview(
        &decisions,
        &repos,
        &observed,
        &namespace(),
        NS,
        &[],
        Visibility::Public,
    );
    assert!(!public_preview.items[2].needs_creation);
    assert_eq!(
        public_preview.items[2].warnings,
        vec![warning::VISIBILITY_BLOCKED.to_owned()]
    );
    assert!(public_preview.items[3].needs_creation);
    assert_ne!(private_preview.digest, public_preview.digest);

    // Missing observation: not creatable.
    let partial = build_preview(
        &decisions,
        &repos,
        &PathObservations::new(),
        &namespace(),
        NS,
        &[],
        Visibility::Private,
    );
    assert!(partial.items.iter().all(|i| !i.needs_creation));
    Ok(())
}

#[test]
fn commit_is_bound_to_digest_and_expiry_and_never_adopts() -> Result<(), CommitError> {
    let repos = vec![repo("1", "free", true), repo("2", "taken", true)];
    let decisions = preview_mappings(&repos, &[], &[], &namespace(), false);
    let mut observed = PathObservations::new();
    observed.insert("1".to_owned(), None);
    observed.insert("2".to_owned(), Some(foreign(50, "backups/taken-gh2")));
    let preview = build_preview(
        &decisions,
        &repos,
        &observed,
        &namespace(),
        NS,
        &[],
        Visibility::Private,
    );
    assert_eq!(
        commit_preview(&preview, "other", 0, 10, true),
        Err(CommitError::DigestMismatch)
    );
    assert_eq!(
        commit_preview(&preview, &preview.digest, 10, 10, true),
        Err(CommitError::Expired)
    );
    let committed = commit_preview(&preview, &preview.digest, 0, 10, true)?;
    assert_eq!(committed.proposals.len(), 1);
    assert_eq!(committed.proposals[0].proposal.provider_repository_id, "1");
    assert!(committed.proposals[0].source_private);
    assert_eq!(committed.visibility, Visibility::Private);
    let id = GitHubRepoId::parse("1").map_err(|_| CommitError::DigestMismatch)?;
    assert_eq!(
        committed.proposals[0].proposal.mirror_relative_path,
        mirror_relative_path(id)
    );
    assert!(!commit_preview(&preview, &preview.digest, 0, 10, false)?.allow_create_missing);
    Ok(())
}

#[test]
fn the_digest_covers_every_decision_field() {
    let base = PreviewItem {
        repository_id: "1".to_owned(),
        intended_path: "a-gh1".to_owned(),
        existing_project_id: None,
        needs_creation: true,
        can_adopt: false,
        warnings: Vec::new(),
        source_private: true,
        proposal: None,
    };
    let digest = |item: &PreviewItem, v: Visibility| {
        preview_digest(&namespace(), v, std::slice::from_ref(item))
    };
    let original = digest(&base, Visibility::Private);
    let mut variants = Vec::new();
    let mut v = base.clone();
    v.intended_path = "b-gh1".to_owned();
    variants.push(v);
    let mut v = base.clone();
    v.existing_project_id = Some("5".to_owned());
    variants.push(v);
    let mut v = base.clone();
    v.needs_creation = false;
    variants.push(v);
    let mut v = base.clone();
    v.can_adopt = true;
    variants.push(v);
    let mut v = base.clone();
    v.warnings = vec!["x".to_owned()];
    variants.push(v);
    let mut v = base.clone();
    v.source_private = false;
    variants.push(v);
    for variant in &variants {
        assert_ne!(digest(variant, Visibility::Private), original);
    }
    assert_ne!(digest(&base, Visibility::Public), original);
}

#[test]
fn every_preview_item_has_a_non_empty_intended_path() {
    // DestinationPreviewItem.intendedPath has minLength 1.
    let repos = vec![
        repo("1", "a", true),
        repo("1", "dup", true),
        repo("x1", "bad", true),
        repo("2", "b", false),
    ];
    let decisions = preview_mappings(&repos, &[], &[], &namespace(), false);
    let preview = build_preview(
        &decisions,
        &repos,
        &PathObservations::new(),
        &namespace(),
        NS,
        &[],
        Visibility::Private,
    );
    assert_eq!(preview.items.len(), 4);
    for item in &preview.items {
        assert!(!item.intended_path.is_empty(), "{item:?}");
        assert!(!item.needs_creation, "unobserved: {item:?}");
        assert!(!item.warnings.is_empty());
    }
    assert!(!preview.items[3].source_private);
    assert!(
        preview.items[2].source_private,
        "unknown privacy is private"
    );
}

#[test]
fn one_uncertain_lookup_does_not_abort_the_preview() -> Result<(), ProviderError> {
    struct Flaky;
    impl DestinationApi for Flaky {
        async fn find_by_path(&self, path: &str) -> Result<Option<ProjectView>, ProviderError> {
            if path.ends_with("odd-gh2") {
                Err(ProviderError::UnexpectedResponse)
            } else {
                Ok(None)
            }
        }
        async fn get_by_id(&self, _: u64) -> Result<Option<ProjectView>, ProviderError> {
            Ok(None)
        }
        async fn create(&self, _: &CreateIntent) -> Result<ProjectView, ProviderError> {
            Err(ProviderError::Network)
        }
    }
    struct Expired;
    impl DestinationApi for Expired {
        async fn find_by_path(&self, _: &str) -> Result<Option<ProjectView>, ProviderError> {
            Err(ProviderError::InvalidOrExpired)
        }
        async fn get_by_id(&self, _: u64) -> Result<Option<ProjectView>, ProviderError> {
            Ok(None)
        }
        async fn create(&self, _: &CreateIntent) -> Result<ProjectView, ProviderError> {
            Err(ProviderError::Network)
        }
    }
    let repos = vec![repo("1", "fine", true), repo("2", "odd", true)];
    let decisions = preview_mappings(&repos, &[], &[], &namespace(), false);
    let observed = run(observe_intended_paths(&Flaky, &decisions, "backups"))
        .ok_or(ProviderError::Network)??;
    let preview = build_preview(
        &decisions,
        &repos,
        &observed,
        &namespace(),
        NS,
        &[],
        Visibility::Private,
    );
    assert!(preview.items[0].needs_creation);
    assert!(!preview.items[1].needs_creation);
    assert_eq!(
        preview.items[1].warnings,
        vec![warning::STATE_UNKNOWN.to_owned()]
    );
    assert_eq!(
        run(observe_intended_paths(&Expired, &decisions, "backups"))
            .ok_or(ProviderError::Network)?,
        Err(ProviderError::InvalidOrExpired)
    );
    Ok(())
}
