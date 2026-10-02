# UI-09 - Settings and preference sections

Reference: Refined V2 `6:1231`. Status: **candidate / baseline missing**.
Requirements: REQ-24, REQ-26, REQ-34, REQ-39, REQ-44.
Test scenario: `TEST-UI-09` plus shared visual/icon/accessibility tests.

## Native commands

`gbm_settings_get`, `gbm_settings_update`, `gbm_environment_diagnostics`.
See [IPC contract](../../../contracts/ipc.v1.json); these are app-owned interfaces to implement.

## Layout

Secondary section navigation: General, Backup, Sync, Notifications, Appearance,
Advanced. Show only the selected section's form, not six dense cards side by side.
Each setting uses the right control: switches for booleans, constrained number/select
for intervals/concurrency, read-only field for a post-setup root and scope choices for
repository categories. Avoid an icon box in front of every form row; section icons suffice.

General: startup/close behavior. Backup: root, workspace and scope. Sync: auto scan,
auto sync and max jobs. Notifications: failures, optionally successes. Appearance:
reviewed theme options only. Advanced: diagnostic/log access and explanatory safe
maintenance; no destructive reset-cache control without a specific implementation plan.

## Behavior

Settings are a draft until Save. Cancel/Reset draft restores persisted values, not
deletes caches or backups. Save uses expectedRevision; concurrent change returns a
conflict/reload flow rather than silently overwriting. Validate in form and native code.
Starting auto sync requires the configured mapping/scope confirmation, not only a toggle.

Only light is a V1 visual candidate until review. A System/Dark control is absent or
explicitly unavailable unless a new approved baseline covers it. Telemetry is not a
hidden default; V1 collects none. Tray-dependent choices must be disabled when the
current session has no verified reopen path. Root relocation is not implemented in V1.

## Acceptance

Test invalid intervals, min/max job values, unsaved-change close, native save failure,
revision conflict, unavailable tray/portal and startup registration failure. No success
toast before the native response. Form labels/errors remain readable at 200% text zoom.
Non-General sections need reviewed visual states before release, not fictional Figma IDs.
