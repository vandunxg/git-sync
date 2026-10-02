# Task report — P03-T04 Implement and security-review credential broker

Requirements: REQ-22, REQ-45. Scenarios: TEST-CREDENTIAL-BROKER (implemented, PASS for the broker scope),
TEST-REDACTION (git_auth redaction part only; UI/DB/events/export parts belong to later tasks).

## Changed files
crates/infrastructure/src/platform/{mod.rs,tests.rs} — private 0700 runtime dir under $XDG_RUNTIME_DIR-like base
(no /tmp fallback), current uid, /proc parent/ancestry and liveness.
crates/infrastructure/src/git_auth/{mod,broker,helper,protocol,remote,command,redact,tests}.rs — per-job Unix
socket broker (SO_PEERCRED uid + live job process tree, https + exact host + exact path, TTL ≤1h, ≤32 served,
socket 0600, dev/inode-guarded cleanup, stale sweep), blocking helper relay (store/erase never forwarded), bounded
credential-protocol parser, HTTPS allowlist for remotes (github.com, gitlab.com; no userinfo/port/dot segments),
Git argv/env builder (helper reset + hardening; closed env allowlist), redaction.
crates/infrastructure/src/lib.rs, crates/infrastructure/Cargo.toml — scope exception (coordinator-approval.md).

## Commands (repo root) — checks.log
cargo fmt --check 0; cargo clippy --workspace --all-targets -D warnings 0; git_auth+platform tests 28 passed;
cargo test --workspace 0 (1 pre-existing LFS test ignored: git-lfs missing).
Negative controls C1–C6 (helper reset, hooksPath, process tree, exited PID, path binding, expiry) each detected
(negative-controls.log). Real `git credential fill` (git 2.54.0) with a hostile global helper and hostile repo
config proves reset/hardening; /proc/<git>/cmdline+environ scanned for the sentinel.

## Limitations
Linux only. Production helper binary, runner wiring (bind_peer after spawn, shutdown before reap, startup sweep)
are P05-T01. Residual risks in REVIEW.md need explicit owner acceptance → task stays REVIEW_READY until then.
scope-check.json: FAIL only for the documented exception paths and the owner's untracked docs/design/.

## Next
Owner: accept/reject residual risks (REVIEW.md). Next ready backend task: P04-T01 (paginated discovery).
