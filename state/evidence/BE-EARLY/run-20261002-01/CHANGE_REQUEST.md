# CR-BE-EARLY-2026-10-02 — wire existing backend capabilities to IPC early

Requested by: repository owner (chat 2026-10-02, "nối sớm BE"; record state/evidence/OWNER-APPROVAL-2026-10-02.md).
Related tasks: P07-T03 (read-model wiring), P08-T01/T04 (onboarding, connections, settings), P04-T02 (owned
storage root — minimal subset), P06-T01 (namespaces — read-only listing only).

Gap: every native command answered UNSUPPORTED, so the real window could only show the startup failure.
Change: a live CommandHandler (apps/desktop/src-tauri/src/backend) backed by existing stores for:
gbm_app_bootstrap, gbm_accounts_list, gbm_credentials_validate_store, gbm_accounts_disconnect,
gbm_settings_get/update (PRODUCT.md defaults, optimistic revision), gbm_environment_diagnostics (real probes,
`unknown` where not probed), gbm_activity_list (stored entries), gbm_storage_pick_root/commit_root (native
folder dialog, canonical/owned/non-system/non-git validation, `.gbm/root-manifest.json` create-new),
gbm_namespaces_list (GitLab read-only; canCreateProject = null).
All other commands still answer UNSUPPORTED. onboardingComplete stays false until destination commit
(P06-T01) exists, so the app remains in onboarding by design.

Not invented: availableBytes null (no statvfs probe), expiry/capabilities only from the current session's
validation, no namespace create permission claimed, activity not linked to repositories yet.
New dependencies: tauri-plugin-dialog 2.8.1 (gtk3 default; Rust-side use only, no renderer permission),
serde 1.0.229 (already locked). Decision: ACCEPTED by owner instruction; review: REVIEW.md.
