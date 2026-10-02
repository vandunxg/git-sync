# P05-T01 — Secure bounded Git process runner

Requirements: REQ-19, REQ-22, REQ-45.
Scenarios: TEST-GIT-INPUT, TEST-PROCESS-CANCEL, TEST-CREDENTIAL-BROKER.

## Changed files
- crates/infrastructure/src/git/runner.rs (+ runner/tests.rs): GitBinary (trusted
  absolute canonical executable, root/user owned, not group/world-writable, nor its
  dir; fixed candidate list, no $PATH lookup), Location (allowlisted HTTPS RemoteTarget
  or canonical local path), Subcommand allowlist, GitInvocation (argument checks: NUL/
  CR/LF, `<transport>::` helper syntax, non-HTTPS schemes, program/config/redirect
  options; hardening `-c` first; broker helper words via `with_auth`; file transport
  opt-in only for local locations), WorkDir (neutral private dir or explicit GIT_DIR,
  GIT_CEILING_DIRECTORIES), GitRunner::run (cleared env allowlist with fixed PATH and
  private HOME, own process group, concurrent bounded drains, cancel/timeout TERM →
  KILL to group + start-time-checked descendants, reaping, leftover group cleanup,
  stderr redaction), GitOutput::success_stdout (exit 0 + reaped + complete + UTF-8).
- crates/infrastructure/src/git/mod.rs: `pub mod runner`, doc update.
- crates/infrastructure/src/platform/mod.rs: start_time, ProcessId, descendants,
  process_group, group_members, StopSignal, signal_group, signal_process.
- crates/infrastructure/Cargo.toml, Cargo.lock: scope exception (coordinator-approval.md).

## Scenario coverage
- TEST-GIT-INPUT: hostile args rejected before spawn; real git fetch into a repository
  whose config rewrites the source to `ext::` with protocol.allow=always and points
  hooksPath/fsmonitor/credential.helper at marker scripts, plus a reference-transaction
  hook and a hostile global ~/.gitconfig in HOME → refused with "transport 'ext' not
  allowed", no marker; shell-like refspec is inert data; clean fetch runs no hook.
  LFS endpoint redirection: not exercised (no LFS operations exist before P09; git-lfs
  hardening belongs to P09-T01).
- TEST-PROCESS-CANCEL: a stand-in git flooding stdout+stderr with a background helper
  is cancelled → Cancelled, reaped, both outputs truncated at the bound, pipes closed,
  all 4 PIDs dead; TERM-ignoring tree → TimedOut via KILL escalation, reaped; background
  child after a normal exit is killed without waiting for it.
- TEST-CREDENTIAL-BROKER (argv/env half): child env is a closed allowlist, no token in
  argv/env, only helper words; secret in stderr is redacted. Peer/job/host/path binding
  and expiry were verified in P03-T04.

## Security review round 1 fixes
- H1 option abbreviation bypass (`--upload-pa`, `--exe`, `--temp` reproduced by the
  reviewer): free-form string args removed. `Arg` is typed (Flag/EndOfOptions/
  Location/RefSpec/RefPrefix/Oid); flags must be exact entries of a per-subcommand
  allowlist; no flag after `--`; RefSpec/RefPrefix are full ref names (no globs). N6.
- M1 URLs only via `Location` (RemoteTarget: allowlisted host, no userinfo).
- M2 `with_auth` takes `AuthArgs` (only from a HelperCommand via git_auth_args);
  `with_local_transport` removed — file transport only when a Location::Local is an
  argument. N12.
- M3 descendants are scanned every 8 polls while running and killed after any exit;
  a setsid helper seen while running no longer survives (test; N10). Remaining escape
  window (setsid + reparent between two scans) is a residual risk for owner acceptance.
- L1 PATH=/usr/bin:/bin, /usr/local/bin last candidate. L2 every ancestor of the
  binary must be root/user-owned and not group/world-writable (test binaries use a
  cfg(test) constructor). L3 truncated stderr drops the partial last line before
  redaction (token split at the bound tested; N11); GitOutput Debug omits stdout.
  L4 obsolete (typed args). L5 added core.attributesFile/excludesFile, core.pager,
  protocol.version=2, http.cookieFile, init.templateDir; repo-local insteadOf/remote/
  include trusted only for app-managed GIT_DIR mirrors (documented). L6 descendants()
  linear via parent->children map. L8 comment corrected. L7 recorded as residual.

## Commands
checks.log: fmt 0, clippy -D warnings 0, workspace tests 0, infrastructure suite
repeated 5× green. negative-controls.log: N1–N12 each caught (N6 reintroduces the pre-review H1). scope-check.json: FAIL
only for the documented exception paths (+ owner's untracked docs/design).

## Residual risks requiring OWNER acceptance (security decision, not self-approved)
1. A descendant that setsid()s and is reparented between two 200 ms scans escapes
   signalling; the run then reports output_complete=false (never a success). No
   subreaper/cgroup is used (would affect the whole app process).
2. Repository-local config that `-c` cannot override (url.*.insteadOf, remote.*,
   include/includeIf) is trusted only for app-managed GIT_DIR mirrors; URLs are always
   explicit Locations.
3. Trust in the git binary's own exec-path helpers (git-remote-https etc.).
4. A detached drain/waiter thread per run whose pipes or child never close.
- Production SourceRefReader over this runner and the credential broker wiring per job
  belong to P05-T02.
