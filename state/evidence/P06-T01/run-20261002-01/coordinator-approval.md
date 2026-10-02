# P06-T01 scope exception (coordinator record)

Allowed paths: `crates/infrastructure/src/gitlab/`, `crates/application/`.

Touched outside:
- `crates/infrastructure/src/http/mod.rs`: `ProviderHttp` had only GET. Project creation
  needs one POST. Added `post_json` (same origin pinning, no redirects, no proxy,
  sensitive auth header, bounded body) by generalising `get` into a private `send`,
  plus a read-only `origin()` accessor used to validate clone URLs. No behaviour change
  for GET.
- `crates/infrastructure/src/http/test_server.rs` (test-only, `cfg(test)`): reads the
  request body announced by content-length (to assert the POST payload), holds `Hang`
  connections in a spawned task so later replies are still served (timeout-then-
  reconcile test), and substitutes `{{ORIGIN}}/` / `{{HOSTPORT}}/` placeholders so clone
  URLs can match the loopback origin.

Not done here (out of scope): wiring `gbm_destination_preview` / `gbm_destination_commit`
in `apps/desktop/src-tauri` and persisting mappings/intents in SQLite. The onboarding
Review screen therefore still returns UNSUPPORTED until that wiring task.

Untracked `docs/design/` is the owner's and is not part of this task.
