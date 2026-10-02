# Security review — P03-T04 credential broker

Reviewer: independent security review subagent (owner-designated 2026-10-02). Two rounds.

Round 1: CHANGES_REQUIRED — H1 sequential accept loop starvable by a silent client; M1 bare PID binding never
invalidated; M2 descendants of Git (hooks/fsmonitor/askpass/other transports) could fetch the token; M3 quadratic
redaction; M4 redaction missed Authorization headers/encodings/prefixes; L1 socket cleanup/stale sweep; L2 no
zeroize; L3 uid source & sun_path length; L4 test gaps (wrong job, live /proc argv/env).

Fixes: peer check before any read, owned JoinSet tasks + semaphore + 1s read timeout; is_running/JobProcessExited
+ documented caller contract; hardening -c options (hooksPath, fsmonitor, askPass, protocol allowlist, and after
round 2: http.sslVerify/proxy/extraHeader/followRedirects); linear per-line redaction with headers, percent
encoding and extended prefixes; dev/inode-guarded unlink + sweep_stale_sockets; SocketPathTooLong; new tests.

Round 2: ACCEPTED. Remaining low findings: prefix detection needs a word boundary (heuristic), Denial::Malformed
not exercised directly against the broker (parser unit-tested).

Residual risks requiring explicit OWNER acceptance (not accepted by the reviewer or implementer):
1. Same-uid attacker with ptrace / /proc/pid/mem / unlocked session can read the token from memory.
2. Any process in Git's tree can obtain that job's credential; filter/textconv drivers are not disabled (bare
   mirror makes it moot now; P05-T04 workspace work needs its own review before using the broker).
3. Linux-only PID binding; brief gap if the handle outlives the child; bind race fails closed.
4. Secrets not zeroized in memory; token held up to the grant TTL (max 1h) unless shut down earlier.
5. current_uid via /proc/self owner fails closed for non-dumpable processes.
6. Crash leaves a 0600 socket (no secret); sweep_stale_sockets must be wired at startup (P05).
7. Redaction is heuristic (unknown formats, glued tokens, non-header encodings, chunk-split tokens).
8. In-tree clients can occupy the 4 slots ~1s each; MAX_SERVED=32.
9. Exact host/path match denies LFS endpoints and URL variants (fail closed; extension needs review).
10. Integration test uses a test-only Python helper; production helper binary + test land in P05-T01.
