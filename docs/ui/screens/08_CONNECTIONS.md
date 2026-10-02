# UI-08 - Connections

Reference: Refined V2 `6:1110`. Status: **candidate / baseline missing**.
Requirements: REQ-11, REQ-12, REQ-24, REQ-33, REQ-41.
Test scenario: `TEST-UI-08` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_accounts_list`, `gbm_credentials_validate_store`, `gbm_accounts_disconnect`, `gbm_namespaces_list`, `gbm_environment_diagnostics`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

One concise section each for GitHub, GitLab and local storage. Show verified account,
namespace/root, connection state and last check with one Manage action. Credential
capabilities expand on demand; no card per OAuth scope and no sudo/admin/registry badges.
Verified official provider logos are content assets, not hand-drawn Lucide-like guesses.

## Manage flows

Reconnect/replace token opens the masked PAT dialog and validates the new credential
before replacing the old reference. Do not call this Rotate token if the app only stores
a new user-created token. Persistent keyring vs session-only mode is visible. Expiry is
shown only when verified; `Unknown` is acceptable. Test connection is read-only.

Disconnect confirmation explains: affected jobs stop/pause; local credentials are
removed; local/GitLab backups are retained. Do not imply that provider tokens were
revoked remotely or that backups were deleted. Never display existing secrets.

Local storage shows actual capability/path; keyring health is distinct from storage
capacity. Root relocation after setup is disabled with a precise explanation, not a
fake Change Location button.

## Acceptance

A connected old account with expired current credentials is not shown as healthy.
Keyboard forms, token replace failure, keyring lock, partial repository grant, no GitLab
create permission and disconnected-but-existing-backups states are covered. UI copy
reflects verified capabilities and never invents read/write scope success.
