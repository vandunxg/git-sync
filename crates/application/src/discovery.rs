//! Source catalog discovery (REQ-03, REQ-21, REQ-41; DISCOVERY_AND_PROVIDERS.md).
//!
//! Identity is the immutable provider repository ID, never the name. A discovery
//! is complete only when every page was read; any failure leaves it incomplete with
//! the items seen so far. Reconciliation never produces a deletion: repositories not
//! seen are retained, and only a complete scan marks them as `source_unavailable`
//! candidates for review (a 404 means inaccessible, not deleted).

use std::collections::{BTreeMap, HashSet};

use crate::providers::ProviderError;

/// How the authenticated user relates to a repository (from the provider payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Owner,
    Collaborator,
    Organization,
}

/// One repository as reported by the provider; metadata is a hint, not a ref map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRepository {
    /// Numeric provider ID as a string (immutable identity).
    pub provider_repository_id: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    /// `None` when the provider reports no default branch.
    pub default_branch: Option<String>,
    pub is_private: bool,
    pub is_fork: bool,
    pub is_archived: bool,
    pub relation: Relation,
    pub pushed_at: Option<String>,
}

/// Explicit scope filters (Settings `include*`; PRODUCT.md default policies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryScope {
    pub include_private: bool,
    pub include_forks: bool,
    pub include_archived: bool,
    pub include_collaborations: bool,
    pub include_organizations: bool,
}

impl DiscoveryScope {
    pub fn from_settings(settings: &gbm_domain::contract::Settings) -> Self {
        Self {
            include_private: settings.include_private,
            include_forks: settings.include_forks,
            include_archived: settings.include_archived,
            include_collaborations: settings.include_collaborations,
            include_organizations: settings.include_organizations,
        }
    }

    /// GitHub `affiliation` values to request; owned repositories are always listed.
    pub fn affiliations(&self) -> Vec<&'static str> {
        let mut out = vec!["owner"];
        if self.include_collaborations {
            out.push("collaborator");
        }
        if self.include_organizations {
            out.push("organization_member");
        }
        out
    }

    pub fn includes(&self, repo: &DiscoveredRepository) -> bool {
        // The provider already filters by `affiliation`. Locally the relation is only
        // inferred (an outside collaborator on an organization repository looks like
        // `Organization`), so any non-owned repository is kept when either opt-in is on.
        let relation_ok = match repo.relation {
            Relation::Owner => true,
            Relation::Collaborator | Relation::Organization => {
                self.include_collaborations || self.include_organizations
            }
        };
        relation_ok
            && (self.include_private || !repo.is_private)
            && (self.include_forks || !repo.is_fork)
            && (self.include_archived || !repo.is_archived)
    }
}

/// Why a discovery is not complete. Items read before the stop are still valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incomplete {
    /// A page request failed (rate limit, 403, timeout, ...).
    PageFailed { page: u32, error: ProviderError },
    /// The page limit was reached before the provider reported a last page.
    PageLimitReached { pages: u32 },
    /// Some entries were malformed and skipped, so the set is not exact.
    RejectedEntries { count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    /// In-scope repositories, deduplicated by provider ID (last occurrence wins).
    pub items: Vec<DiscoveredRepository>,
    /// Provider IDs listed by the provider but excluded by the scope.
    pub out_of_scope: Vec<String>,
    pub pages_fetched: u32,
    /// `None` = every page read and every entry valid.
    pub incomplete: Option<Incomplete>,
}

impl Discovery {
    pub fn is_complete(&self) -> bool {
        self.incomplete.is_none()
    }
}

/// Accumulates pages; applies scope and de-duplication (pagination can shift items).
#[derive(Debug)]
pub struct DiscoveryBuilder {
    scope: DiscoveryScope,
    by_id: BTreeMap<String, DiscoveredRepository>,
    out_of_scope: HashSet<String>,
    pages: u32,
    rejected: usize,
}

impl DiscoveryBuilder {
    pub fn new(scope: DiscoveryScope) -> Self {
        Self {
            scope,
            by_id: BTreeMap::new(),
            out_of_scope: HashSet::new(),
            pages: 0,
            rejected: 0,
        }
    }

    pub fn add_page(&mut self, items: Vec<DiscoveredRepository>, rejected: usize) {
        self.pages += 1;
        self.rejected += rejected;
        for item in items {
            if self.scope.includes(&item) {
                self.out_of_scope.remove(&item.provider_repository_id);
                self.by_id.insert(item.provider_repository_id.clone(), item);
            } else {
                self.by_id.remove(&item.provider_repository_id);
                self.out_of_scope.insert(item.provider_repository_id);
            }
        }
    }

    pub fn pages(&self) -> u32 {
        self.pages
    }

    /// Finishes the discovery; `stop` is the reason the walk ended early, if any.
    pub fn finish(self, stop: Option<Incomplete>) -> Discovery {
        let incomplete = stop.or(if self.rejected > 0 {
            Some(Incomplete::RejectedEntries {
                count: self.rejected,
            })
        } else {
            None
        });
        Discovery {
            items: self.by_id.into_values().collect(),
            out_of_scope: {
                let mut ids: Vec<String> = self.out_of_scope.into_iter().collect();
                ids.sort();
                ids
            },
            pages_fetched: self.pages,
            incomplete,
        }
    }
}

/// A repository already in the catalog (from persistence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub provider_repository_id: String,
    pub full_name: String,
}

/// Changes to apply to the catalog. There is deliberately no deletion variant.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CatalogDelta {
    pub added: Vec<DiscoveredRepository>,
    /// Same provider ID, new name: update display metadata only (no filesystem move).
    pub renamed: Vec<(String, String, String)>,
    pub seen: Vec<DiscoveredRepository>,
    /// Known but not listed by a *complete* scan: mark `source_unavailable` for review.
    pub source_unavailable_candidates: Vec<String>,
    /// Known but not listed by an *incomplete* scan: state unknown, keep as is.
    pub not_observed: Vec<String>,
    /// Known and still listed by the provider, but excluded by the current scope:
    /// keep backups, stop scheduling (never a `source_unavailable` signal).
    pub out_of_scope: Vec<String>,
    pub complete: bool,
}

pub fn reconcile(existing: &[CatalogEntry], discovery: &Discovery) -> CatalogDelta {
    let known: BTreeMap<&str, &CatalogEntry> = existing
        .iter()
        .map(|e| (e.provider_repository_id.as_str(), e))
        .collect();
    let listed: HashSet<&str> = discovery
        .items
        .iter()
        .map(|r| r.provider_repository_id.as_str())
        .collect();
    let mut delta = CatalogDelta {
        complete: discovery.is_complete(),
        ..CatalogDelta::default()
    };
    for repo in &discovery.items {
        match known.get(repo.provider_repository_id.as_str()) {
            None => delta.added.push(repo.clone()),
            Some(entry) => {
                if entry.full_name != repo.full_name {
                    delta.renamed.push((
                        repo.provider_repository_id.clone(),
                        entry.full_name.clone(),
                        repo.full_name.clone(),
                    ));
                }
                delta.seen.push(repo.clone());
            }
        }
    }
    let excluded: HashSet<&str> = discovery.out_of_scope.iter().map(String::as_str).collect();
    for entry in existing {
        if excluded.contains(entry.provider_repository_id.as_str()) {
            delta
                .out_of_scope
                .push(entry.provider_repository_id.clone());
        } else if !listed.contains(entry.provider_repository_id.as_str()) {
            let id = entry.provider_repository_id.clone();
            if delta.complete {
                delta.source_unavailable_candidates.push(id);
            } else {
                delta.not_observed.push(id);
            }
        }
    }
    delta
}

#[cfg(test)]
mod tests;
