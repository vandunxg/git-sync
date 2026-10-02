# UI-02 - Onboarding / provider connections

Reference: Refined V2 `6:73`. Status: **candidate / baseline missing**.
Requirements: REQ-11, REQ-12, REQ-24, REQ-26, REQ-27, REQ-41, REQ-45.
Test scenario: `TEST-UI-02` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_accounts_list`, `gbm_credentials_validate_store`, `gbm_namespaces_list`, `gbm_destination_preview`, `gbm_destination_commit`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

Provider sections are simple account rows/forms, not dashboard metrics. Use verified
GitHub/GitLab marks with service name and required capability explanation. The existing
"Authorize" mock wording changes to "Connect with token" in V1. Browser authorization
must not appear to work before the deferred implementation exists.

## Credential form

One labeled masked token field, storage mode and explicit Connect action. No real/sample
token prefill, automatic clipboard read or token retention after successful submission.
Show native identity/permission validation state separately from successful keyring
storage. A timeout leaves input recoverable during this one dialog without persisting it.

Session-only mode clearly states restart consequences. Connected account shows username,
verifiedAt, capability summary and expiry only when known. A token granting limited
repos does not produce an "all account repositories" promise. GitLab namespace choices
come from the API and allowability remains unknown until confirmed; do not hardcode it.

## Review behavior

A preview shows how selected source repos map to destinations and identifies existing
projects that need adoption review. No new project is created merely by opening the
form. Confirming create policy is explicit. Public/private changes cannot downgrade
source privacy without a separate warning and explicit choice.

## Acceptance

Test empty/invalid/expired/insufficient token, keyring locked, namespace pagination,
source scope incomplete and existing destination conflicts. Copy/paste input works but
secrets never appear in screenshots/evidence. Success updates account cache only after
the native result; a simulated connection is labeled test-only.
