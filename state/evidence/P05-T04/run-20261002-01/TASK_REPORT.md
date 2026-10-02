# P05-T04 — Optional workspace updates (safe)

Requirement: REQ-17. Scenario: TEST-WORKSPACE.

## Changed files
- crates/application/src/workspace.rs (+ tests): WorkspaceGit port and
  `update_workspace`: skip (with explicit reason) when missing, detached, unborn, on
  another branch, operation in progress, tracked changes (staged/unstaged/unmerged),
  untracked files, ahead, diverged, filter attributes in the target, or changed
  during preparation; otherwise bring the target into a temporary ref, re-inspect,
  `merge --ff-only`, verify HEAD/branch/cleanliness after, always drop the temp ref.
- crates/infrastructure/src/workspace/{mod,tests}.rs: GitWorkspace over the runner
  (status --porcelain=v2 -z parsing fail-closed; state files for in-progress ops;
  fetch from the mirror snapshot ref into refs/gbm/incoming/<job> with exact OID
  check; merge-base --is-ancestor; ls-tree + cat-file blob for .gitattributes
  filters and .git/info/attributes; merge --ff-only; update-ref -d with old OID).
- crates/infrastructure/src/git/runner.rs: worktree support (coordinator-approval.md).
- lib.rs registrations.

## Scenario coverage (real Git, local fixtures)
- TEST-WORKSPACE: clean clone fast-forwarded (HEAD == captured target, branch kept,
  clean, idempotent second run = Current); dirty, staged, untracked collision (file
  content preserved), detached, diverged, other branch, merge in progress and ahead
  workspaces are skipped with their reason and stay byte-for-byte unchanged (worktree
  files + HEAD + index + branch/tag refs), with no temporary ref left behind.
- Filter attributes in the target → FiltersUnsupported, unchanged.
- User hooks (post-merge, reference-transaction, post-checkout) do not run.
- `.git` symlink layout → Unsupported; missing folder → Missing.

## Commands
checks.log: fmt 0, clippy -D warnings 0, workspace tests 0, infra suite 3× green.
negative-controls.log: N1–N9 each caught. scope-check.json: FAIL only for runner.rs
and lib.rs (documented) + owner's untracked docs/design.

## Limitations / notes
- The user's own hooks and filter drivers are intentionally not run; repositories
  using filters (incl. Git LFS) get FiltersUnsupported until P09.
- Skipped ahead/diverged checks fetch the target objects into the workspace's object
  store (objects only; refs/HEAD/index/files unchanged; temp ref removed).
- Workspace creation (initial clone from the mirror) and UI wiring are not in this task.
- A user edit racing the final `merge --ff-only` is protected by Git's own refusal to
  overwrite local changes; a non-conflicting concurrent edit would be carried along
  (not lost).

## Round 2 (after security review)
- Ignored-file overwrite (review F1, HIGH) fixed: before the write, paths the target adds
  are compared with the working tree; an existing path or a non-directory parent (unless
  HEAD tracks it and the same change deletes it) → Skipped(WouldOverwriteLocalFiles).
  New runner subcommand `diff-tree` with fixed flags and OID-only arguments.
- `GIT_OPTIONAL_LOCKS=0` for working-tree runs; REBASE_HEAD/BISECT_START markers;
  drop_target fails on errors other than "ref absent".
- checks.log round 2: fmt 0, clippy 0, workspace tests 0, infra 129 tests ×3 green.
- negative-controls.log: N1–N13 each caught.
- Review: APPROVED round 2; residual risks in REVIEW.md await owner acceptance.
