# P04-T02 — Stable mappings and owned storage paths

Requirements: REQ-04, REQ-05, REQ-40. Scenarios: TEST-IDENTITY, TEST-ROOT.

## Changed files
- crates/domain/src/identity.rs (+ identity/tests.rs): GitHubRepoId (canonical decimal),
  RelativePath (safe components, parse rejects traversal/absolute/`\`/non-ASCII),
  mirror/snapshot/workspace relative paths with the provider-ID suffix, GitLab
  project-path proposal `<name>-gh<id>`, case-folded keys and overlap check.
- crates/application/src/mapping.rs (+ mapping/tests.rs): pure `preview_mappings`;
  existing mapping kept by provider ID on rename/transfer (display_changed hint only);
  new mappings proposed; project-path clash with a mapped or observed foreign project,
  local path overlap, duplicate or invalid source IDs are Blocked (never adopted).
- crates/infrastructure/src/filesystem/{mod,tests}.rs: OwnedRoot::verify (canonical
  root, private `.gbm/`, regular non-symlink manifest of this user naming the root ID),
  create_dir (component walk without following symlinks, 0700, create-new leaf,
  case-variant refusal, final canonical re-check), open_dir (no creation; Missing).
- One-line module registration in each crate's lib.rs (required to compile the new
  modules; infrastructure/src/lib.rs is outside the listed paths — scope note).

## Scenario coverage
- TEST-IDENTITY: rename/transfer keeps stored mapping; a new repo reusing an old name
  gets its own id-bound mapping; case/sanitize collisions stay distinct in domain,
  preview and on disk.
- TEST-ROOT (filesystem half): symlink escape (intermediate and leaf), traversal
  (type-level: RelativePath), unwritable `.gbm`, pre-existing foreign leaf, file in the
  way, missing mapped folder, unverified/foreign/symlinked manifest, non-canonical root.
  Root *selection* validation and picker cancel are covered by existing storage/ and
  backend tests (CR-BE-EARLY); not modified here.

## Commands
See checks.log (fmt 0, clippy -D warnings 0, workspace tests 0) and
negative-controls.log (N1–N5 each caught).

## Limitations
- Mappings are not persisted (sqlite/ outside scope) and not wired to IPC.
- No openat: a same-user race between component checks is detected by the final
  canonical re-check but not prevented.
- Free-space checks and temp staging belong to P05.
