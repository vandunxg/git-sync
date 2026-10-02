# P04-T02 review record

Reviewer: independent review subagent (owner-designated 2026-10-02).

## Round 1 — ACCEPTED with recommended fixes
- M1 mapping.rs: a mapped GitLab project renamed (GitLab-side) onto a new source's
  suggested path was exempt from the observed-path check → new source Proposed onto an
  occupied path. Fixed: any observed path match blocks (Mapped/Foreign); test added;
  negative control N6.
- L2 claimed-path key included namespace, DB UNIQUE is (gitlab_account_id, path).
  Fixed: keyed per account; test added.
- L3 RelativePath::parse validated safety, not shape. Fixed: parse_mirror_path /
  parse_workspace_path; test added.
- L4 TOCTOU doc did not mention a stray dir on race. Fixed (doc).
- L5 root mode not checked. Fixed: world-writable root rejected; test; N7.
  Group-writable allowed (user-private-group umask); foreign-uid components rejected.
- Note: unwritable test is vacuous when run as uid 0 (documented in test).

## Round 2 — ACCEPTED, no new findings
Reviewer re-ran domain/application/infrastructure tests and workspace clippy.
