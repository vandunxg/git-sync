# Scope exception — P05-T03 (coordinator, 2026-10-02)

Out-of-allowlist paths and reason:
- crates/infrastructure/src/git/runner.rs (+ runner/tests.rs): the only way to run Git
  is the reviewed runner. Bundle support needed: `Subcommand::Bundle` with exact words
  create/verify/list-heads/--quiet; typed `Arg::Bundle(BundleFile)` (absolute,
  canonical parent, safe `<name>.bundle`) and `Arg::RefGlob(RefPrefix)` →
  `--glob=<prefix>*`; both rejected for any other subcommand. Security reviewer to
  re-check these additions.
- crates/infrastructure/src/lib.rs: `pub mod snapshots;` registration only.
- docs/design/*: owner's untracked folder, not committed.
