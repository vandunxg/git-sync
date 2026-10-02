# Review — CR-BE-EARLY-2026-10-02

Reviewer: independent review subagent (owner-designated 2026-10-02), security-focused. Two rounds.
Round 1 CHANGES_REQUIRED: (M1) reconnect/mode-switch could orphan or delete referenced secrets; (M2) failed
root DB insert left a manifest → permanent AlreadyClaimed; (L3) existing .gbm not checked; (L4) $HOME/app-data
accepted as root; (L5) id collisions; (L6) diagnostics creates the runtime dir (documented).
Verified OK: no secret in logs/activity/DB/responses; keyring failure never falls back; dialog has no renderer
permission; blocking work off the async runtime; honest null/false values; DB-open failure path; 0700/0600 perms.
Round 2 ACCEPTED after fixes (previous-row aware cleanup, Established::undo, private .gbm check, home/app-data
rejection, nanos+atomic ids). Residual: nested child repositories inside a chosen root are not detected;
best-effort cleanup of the old secret after a mode switch; token in IPC JSON buffers until drop (no zeroize);
externally deleted keyring secret shows "connected" until first use.
