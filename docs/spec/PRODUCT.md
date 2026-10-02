# Product specification - V1

## Goal

Back up the owner's selected GitHub repositories to GitLab and a local folder, detect
new repositories and changed refs, run verified one-way sync jobs, and show whether
backups are current. Provide useful density without an overloaded operations dashboard.
Primary runtime is Arch Linux + Hyprland. Cross-platform boundaries must be respected,
but Windows/macOS support is not claimed without their separate delivery gates.

## V1 included

- One GitHub account and one GitLab account per profile; PAT-based connection and OS
  keyring, with explicit session-only fallback when persistent keyring is unavailable.
- Discover all pages of selected repositories, including owned forks, private and
  archived repositories when granted. Owned repositories are the default selection;
  organization/collaboration repositories are opt-in, with access limitations shown.
- Choose a backup root. App-managed bare mirrors are mandatory; independent normal
  working copies are optional and never authoritative for remote backups.
- One GitLab project per source repository under a chosen namespace, with stable
  source-ID mapping and an explicit initial preview. This is a SPEC_DEFAULT, not a
  requirement to aggregate every source into a single Git project.
- Full selected branches and tags with reachable history, ref-based change detection,
  safe plan/apply/verify, persistent jobs, retries, cancellation and auto scan/sync.
- LFS payload coverage as a separate verified capability; submodule limitations are
  explicit, never disguised as a complete submodule backup.
- History snapshots before destructive operations; restore validation and a runbook.
- Ten page families described by the UI catalog, including onboarding and dialogs.
- Offline/stale/error states, bounded redacted logs, safe native open-folder/browser
  actions, startup/close behavior and Linux packaging verification.

## Excluded from V1

Two-way sync, conflict resolution UI, commit editing, creating branches, automatic
stashing/merging/resetting, git hosting, team SaaS, embedded terminal, shell execution,
analytics/telemetry, AI features, custom provider plugins, automatic release publishing,
cloud backend, a system daemon, mobile screens, and a full restore-management UI.

Repository Git objects do not include issues, PRs, discussions, wiki repositories,
release assets, Actions artifacts, packages, registries, secrets, project boards or
uncommitted local changes. Do not label these as backed up. They need separate scoped
features. Additional providers, custom enterprise hosts, browser authorization,
Windows/macOS releases and a single-project GitLab archive are deferred.

## Default policies (editable only through their defined flows)

| Setting | Default | Rationale / boundary |
|---|---|---|
| Auto scan | 30 minutes while app runs | Not a 24/7 remote monitor |
| Auto sync | Off until initial review enables it | No surprise remote writes |
| Git job concurrency | 3, allowed 1-8 | Bounded CPU, disk and sockets |
| Owned/private/forks/archived | Included when granted | Do not silently omit selected repositories |
| Collaborations/organizations | Explicit opt-in | Prevent unintended copying of unrelated code |
| Destination visibility | Private unless explicitly selected otherwise | Never weaken source privacy automatically |
| Ref policy | Safe mirror | Rewrites/deletes wait for plan approval |
| Working copy | Optional; selected current branch only | Mirror still preserves all selected refs |
| Retention cleanup | Manual in V1 | Do not silently expire recovery history |
| Notifications | Errors by default; success optional | Avoid a toast per repository |
| Telemetry | None | No hidden account/repo data collection |
| Theme | Light candidate until visual freeze | Do not implement an unapproved dark theme |

## Meaning of "all", "latest" and "backed up"

"All" means all selected repositories the token actually grants, not invisible account
resources. Report inaccessible/partial discovery separately. "Latest" means current
at `sourceObservedAt` for a captured ref map; remote refs may change after observation.
The app cannot recover history deleted upstream before it ever observed/fetched it.

`Git refs verified` means GitLab's selected ref map equals the captured source map and
required local objects were validated. LFS upload acknowledgment and an actual LFS
restore check are distinct claims. Submodule pointers are preserved; their child
contents need separately covered repositories. Workspace freshness is another dimension.

## Completion boundary

A V1 release requires all P00-P10 required acceptance tests and owner-reviewed UI
baselines. Feature code without native verification is not a Linux release. The
package's validators may pass while implementation and visual acceptance remain blocked.
