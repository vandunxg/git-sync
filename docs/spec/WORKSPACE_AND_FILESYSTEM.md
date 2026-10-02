# Filesystem layout and working-copy policy

## Location separation

Application metadata belongs under the platform's app-data/config directories, not
inside backed-up projects. Linux defaults use XDG paths. Do not hardcode a username.

```text
<app-data>/state.sqlite
<app-data>/logs/
<chosen-backup-root>/.gbm/root-manifest.json
<chosen-backup-root>/.gbm/mirrors/gh-<repository-id>.git/
<chosen-backup-root>/.gbm/snapshots/gh-<repository-id>/<snapshot-id>/
<chosen-backup-root>/workspace/<owner>/<name>-<repository-id>/   # optional
```

Paths here are a design contract, not already existing folders. The root manifest
establishes app ownership and version. Internal IDs are validated and path-safe.
Normal workspace clone object storage is independent: avoid borrowed alternates or
hardlink assumptions that make a workspace unusable after mirror maintenance.

## Validation

Canonicalize root and parent ancestry; defend against symlink escapes, traversal,
reserved names, case-insensitive collisions and cross-device assumptions. App data and
backup root must be writable by the current user without privilege escalation. Never
write to filesystem root, an arbitrary existing repository, or a folder adopted only
because its name matches. Newly created directories use restrictive permissions.

Check free space using real OS APIs; unknown capacity is not a made-up number. Snapshot
creation and clone staging use atomic rename within the same filesystem only where
valid. A failed clone leaves a tracked temp directory and the prior good mirror intact.
Cleanup removes only exact app-owned temp paths tied to a failed job. Never glob-delete
folders by a broad name prefix. Do not use network-hosted SQLite/WAL storage [S13].

## Workspace update policy

Before a workspace update, check branch/HEAD, index, tracked and untracked changes, local
commits and upstream mapping. If clean, on a known branch and strictly fast-forward,
a safe update may proceed. Dirty/ahead/detached/diverged/permission-denied workspaces
remain untouched and get an explicit status plus Open folder/View changes guidance.

Do not auto stash, merge, rebase, discard, clean, checkout another branch, reset hard,
or overwrite untracked files. Only the selected working branch is checked out; all
selected branches/tags remain in the bare snapshot. A source default-branch rename
updates metadata, not an automatic user checkout.

Changes already in the workspace do not flow upstream or to GitLab through this app.
No claim that uncommitted local files were backed up. Syncing a bare mirror may continue
while workspace updates are skipped; label both facts separately.

## Root changes

V1 supports root selection before initial configuration. Once repositories are managed,
root relocation is disabled with an explanation. A future relocation workflow needs
its own dry-run, job pause, rollback, free-space and path-validation requirements.
Do not pretend a Settings path field has moved existing backups.
