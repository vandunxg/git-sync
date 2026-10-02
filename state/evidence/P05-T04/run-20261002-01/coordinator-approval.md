# Scope exception — P05-T04 (coordinator, 2026-10-02)

- crates/infrastructure/src/git/runner.rs: Git runs only through the reviewed runner.
  Additions: `WorkDir::WorkTree` (canonical dir whose `.git` is a real directory;
  GIT_DIR + GIT_WORK_TREE, no discovery); subcommands status / merge-base / merge /
  ls-tree with exact flags (`merge` limited to --ff-only --quiet --no-stat
  --no-autostash); `cat-file blob`; hardening adds merge.autoStash=false and
  rebase.autoStash=false. Security reviewer to re-check.
- crates/infrastructure/src/lib.rs: `pub mod workspace;` registration only.
- docs/design/*: owner's untracked folder, not committed.

Round 2 addition: runner.rs also gains `Subcommand::DiffTree` (fixed flags
`-r -z --name-status --no-renames --diff-filter=AD`, OID-only operands) and
`GIT_OPTIONAL_LOCKS=0` for `WorkDir::WorkTree`, both required by review findings F1/F3.
