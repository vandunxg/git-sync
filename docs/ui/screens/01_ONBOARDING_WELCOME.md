# UI-01 - Onboarding / welcome and folder

Reference: Refined V2 `6:3`. Status: **candidate / baseline missing**.
Requirements: REQ-05, REQ-23, REQ-26, REQ-27.
Test scenario: `TEST-UI-01` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_app_bootstrap`, `gbm_environment_diagnostics`, `gbm_storage_pick_root`, `gbm_storage_commit_root`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

Use the Refined V2 welcome composition as the reference. A concise intro and a compact
step list explain source, local backup and GitLab destination; do not use a marketing
hero or fake metrics. Folder and review steps are required functional subviews that
were not separately rendered in V2 and therefore need their own reviewed captures.

## Flow

Welcome -> Choose folder -> Connect GitHub -> Connect GitLab -> Review & start.
A single Get started action advances; Back preserves non-secret entries. After root
selection, show canonical display path, actual free space when known, managed mirror
location and optional workspace checkbox. Picker cancellation leaves state unchanged.

Native diagnostics list only setup blockers: Git missing, keyring unavailable, invalid
folder, portal failure. Explain each cause and a retry action. A missing persistent
keyring permits an explicit session-only choice later; it does not silently store a PAT.

Do not scan/push repos simply because the page mounted. Initial scan is a named user
action. The final review lists selected scope, destination namespace, planned project
mapping, private visibility, safe-mirror policy and auto-sync opt-in before any remote
creation. If the selection changes, invalidate the old preview.

## Acceptance

Keyboard can finish the complete setup flow. No hidden required fields. Invalid root,
unwritable folder and no portal each have a recoverable state. Secrets do not survive
navigation in persisted form state. Initial scan returns an observable job, not a fake
success animation. Onboarding completion is stored only after valid configuration;
initial job failure still leads to an actionable configured UI, not corrupted setup.
