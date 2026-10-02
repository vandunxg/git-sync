use super::*;

fn repo(id: &str, full: &str, relation: Relation) -> DiscoveredRepository {
    let (owner, name) = full.split_once('/').unwrap_or(("o", full));
    DiscoveredRepository {
        provider_repository_id: id.to_owned(),
        owner: owner.to_owned(),
        name: name.to_owned(),
        full_name: full.to_owned(),
        default_branch: Some("main".to_owned()),
        is_private: false,
        is_fork: false,
        is_archived: false,
        relation,
        pushed_at: None,
    }
}

const ALL: DiscoveryScope = DiscoveryScope {
    include_private: true,
    include_forks: true,
    include_archived: true,
    include_collaborations: true,
    include_organizations: true,
};

const DEFAULTS: DiscoveryScope = DiscoveryScope {
    include_private: true,
    include_forks: true,
    include_archived: true,
    include_collaborations: false,
    include_organizations: false,
};

#[test]
fn scope_defaults_are_owned_only_and_explicit_opt_in_for_others() {
    assert_eq!(DEFAULTS.affiliations(), ["owner"]);
    assert_eq!(
        ALL.affiliations(),
        ["owner", "collaborator", "organization_member"]
    );
    assert!(DEFAULTS.includes(&repo("1", "me/a", Relation::Owner)));
    assert!(!DEFAULTS.includes(&repo("2", "x/b", Relation::Collaborator)));
    assert!(!DEFAULTS.includes(&repo("3", "org/c", Relation::Organization)));
    // Outside collaborator on an organization repository: kept when collaborations are on.
    let collaborations = DiscoveryScope {
        include_collaborations: true,
        ..DEFAULTS
    };
    assert!(collaborations.includes(&repo("3", "org/c", Relation::Organization)));
    let mut fork = repo("4", "me/fork", Relation::Owner);
    fork.is_fork = true;
    let no_forks = DiscoveryScope {
        include_forks: false,
        ..DEFAULTS
    };
    assert!(DEFAULTS.includes(&fork) && !no_forks.includes(&fork));
    let mut private = repo("5", "me/p", Relation::Owner);
    private.is_private = true;
    assert!(
        !DiscoveryScope {
            include_private: false,
            ..DEFAULTS
        }
        .includes(&private)
    );
}

#[test]
fn pages_are_deduplicated_by_id_and_counted() {
    let mut builder = DiscoveryBuilder::new(ALL);
    builder.add_page(
        vec![
            repo("1", "me/a", Relation::Owner),
            repo("2", "me/b", Relation::Owner),
        ],
        0,
    );
    // Pagination shifted: id 2 appears again on page two.
    builder.add_page(
        vec![
            repo("2", "me/b", Relation::Owner),
            repo("3", "me/c", Relation::Owner),
        ],
        0,
    );
    let discovery = builder.finish(None);
    assert!(discovery.is_complete());
    assert_eq!(discovery.pages_fetched, 2);
    assert_eq!(
        discovery
            .items
            .iter()
            .map(|r| r.provider_repository_id.as_str())
            .collect::<Vec<_>>(),
        ["1", "2", "3"]
    );
}

#[test]
fn rejected_entries_make_the_discovery_incomplete() {
    let mut builder = DiscoveryBuilder::new(ALL);
    builder.add_page(vec![repo("1", "me/a", Relation::Owner)], 2);
    let discovery = builder.finish(None);
    assert_eq!(
        discovery.incomplete,
        Some(Incomplete::RejectedEntries { count: 2 })
    );
}

/// TEST-NO-DELETE-ON-404 / TEST-PAGINATION (domain part).
#[test]
fn incomplete_scan_never_marks_or_deletes_unseen_repositories() {
    let existing = vec![
        CatalogEntry {
            provider_repository_id: "1".into(),
            full_name: "me/a".into(),
        },
        CatalogEntry {
            provider_repository_id: "9".into(),
            full_name: "me/gone".into(),
        },
    ];
    let mut builder = DiscoveryBuilder::new(ALL);
    builder.add_page(vec![repo("1", "me/a", Relation::Owner)], 0);
    let partial = builder.finish(Some(Incomplete::PageFailed {
        page: 2,
        error: ProviderError::AccessDenied,
    }));
    let delta = reconcile(&existing, &partial);
    assert!(!delta.complete);
    assert_eq!(delta.not_observed, ["9"]);
    assert!(delta.source_unavailable_candidates.is_empty());
}

#[test]
fn complete_scan_only_flags_unseen_for_review_and_detects_renames() {
    let existing = vec![
        CatalogEntry {
            provider_repository_id: "1".into(),
            full_name: "me/old-name".into(),
        },
        CatalogEntry {
            provider_repository_id: "9".into(),
            full_name: "me/gone".into(),
        },
    ];
    let mut builder = DiscoveryBuilder::new(ALL);
    builder.add_page(
        vec![
            repo("1", "me/new-name", Relation::Owner),
            repo("2", "me/fresh", Relation::Owner),
        ],
        0,
    );
    let delta = reconcile(&existing, &builder.finish(None));
    assert!(delta.complete);
    assert_eq!(delta.added.len(), 1);
    assert_eq!(
        delta.renamed,
        [(
            "1".to_owned(),
            "me/old-name".to_owned(),
            "me/new-name".to_owned()
        )]
    );
    assert_eq!(delta.source_unavailable_candidates, ["9"]);
    assert!(delta.not_observed.is_empty());
}

#[test]
fn scope_change_moves_items_out_without_counting_them_twice() {
    let mut builder = DiscoveryBuilder::new(DEFAULTS);
    builder.add_page(
        vec![
            repo("1", "me/a", Relation::Owner),
            repo("7", "org/x", Relation::Organization),
        ],
        0,
    );
    let discovery = builder.finish(None);
    assert_eq!(discovery.items.len(), 1);
    assert_eq!(discovery.out_of_scope, ["7"]);
    // A known repository that is now merely out of scope is not "unavailable".
    let existing = vec![CatalogEntry {
        provider_repository_id: "7".into(),
        full_name: "org/x".into(),
    }];
    let delta = reconcile(&existing, &discovery);
    assert_eq!(delta.out_of_scope, ["7"]);
    assert!(delta.source_unavailable_candidates.is_empty());
}
