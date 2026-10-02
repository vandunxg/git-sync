# Source references and provenance

Prepared 2026-10-01. Official documentation was consulted while preparing the blueprint.
Package rules are project decisions, not quotations or a claim of endorsement by upstream.
Dependencies and documentation can change: pin and inspect exact installed versions in P00.
No exact new dependency version is invented in this package.

## S01 - Requested scaffold

https://github.com/agmmnn/tauri-ui

Use in this blueprint: Verify the upstream template revision and actual supported frontend/Tauri versions during P00.

## S02 - Tauri capabilities

https://v2.tauri.app/security/capabilities/

Use in this blueprint: Capabilities and custom native command authorization must be reviewed together; frontend permission lists are not a substitute for native validation.

## S03 - Git push

https://git-scm.com/docs/git-push

Use in this blueprint: Mirror, atomic and exact expected-value lease semantics inform the safe explicit-ref plan.

## S04 - GitHub repository duplication

https://docs.github.com/en/repositories/creating-and-managing-repositories/duplicating-a-repository

Use in this blueprint: Git LFS payload transfer is separate from a Git mirror.

## S05 - GitHub repositories REST API

https://docs.github.com/en/rest/repos/repos

Use in this blueprint: Authenticated repository enumeration, pagination and permissions must be matched to the chosen credential type.

## S06 - GitHub App user authentication

https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app

Use in this blueprint: GitHub App user-access flow is not equivalent to a PAT and requires separately verified registration/permissions.

## S07 - Rust API guidelines

https://rust-lang.github.io/api-guidelines/checklist.html

Use in this blueprint: Use idiomatic typed public interfaces, useful errors and documented invariants.

## S08 - Tokio process

https://docs.rs/tokio/latest/tokio/process/index.html

Use in this blueprint: Child lifecycle, cancellation and waiting/reaping require deliberate implementation.

## S09 - React effect guidance

https://react.dev/learn/you-might-not-need-an-effect

Use in this blueprint: Avoid redundant derived state/effects; keep actual synchronization effects bounded and cleaned up.

## S10 - Playwright visual comparisons

https://playwright.dev/docs/test-snapshots

Use in this blueprint: Pinned rendering and managed expected screenshots are required for meaningful visual regressions.

## S11 - Lucide React

https://lucide.dev/guide/react

Use in this blueprint: Use actual supported icon exports; do not fabricate component names or recreate brand artwork.

## S12 - GitLab projects API

https://docs.gitlab.com/api/projects/

Use in this blueprint: Project creation, namespace, visibility and lookup behavior require verified API permissions.

## S13 - SQLite WAL

https://sqlite.org/wal.html

Use in this blueprint: Read concurrency does not remove writer/lifecycle/backup constraints; validate chosen journal setup.

## S14 - Git bundle

https://git-scm.com/docs/git-bundle

Use in this blueprint: Bundle creation and verification support isolated Git recovery; coverage is explicitly selected.

## S15 - GitLab OAuth

https://docs.gitlab.com/api/oauth2/

Use in this blueprint: OAuth/PKCE/device support and instance/version prerequisites must be checked for the deferred auth extension.

## S16 - Tauri prerequisites

https://v2.tauri.app/start/prerequisites/

Use in this blueprint: Linux native dependencies and target environment must be recorded, not inferred from a web build.

## S17 - Tauri WebDriver testing

https://v2.tauri.app/develop/tests/webdriver/

Use in this blueprint: Native application tests require platform-specific setup; they are distinct from browser mocks.

## S18 - Git fetch

https://git-scm.com/docs/git-fetch

Use in this blueprint: Refspec, tags, shallow/partial history and recursion behavior must be explicit in the backup engine.

## S19 - Rust keyring crate

https://docs.rs/keyring/latest/keyring/

Use in this blueprint: Inspect the exact installed backend, feature flags and APIs during P00/P03; no exact version is claimed here.

## Connected design evidence

The Figma Refined V2 page has historical node IDs in docs/ui/FIGMA_MAP.json. A fresh metadata read in this packaging task returned the Starter MCP call-limit error. The latest requested status-dot/icon replacement was not written. Do not interpret the existing page as an approved corrected UI. No signed asset URL, image, private token or font file is packaged.
