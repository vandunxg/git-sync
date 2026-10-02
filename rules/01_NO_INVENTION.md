# R-FACT - no invented functionality, APIs or evidence

## Before using an API

Inspect the installed version, official documentation/type definitions, and a minimal
compile/test proving the selected API exists. Record version and source in the task.
Names in `contracts/ipc.v1.json` are app commands to implement, NOT Tauri plugin methods.
A plausible-looking crate feature, icon export, CLI flag or HTTP route is not evidence.

Do not choose `latest` repeatedly during implementation. P00 records toolchain and
resolved dependencies; lockfiles are committed when committing is authorized. Any
upgrade is a separate reviewed change. Keep speculative alternatives out of code.

## No fabricated behavior

- Do not invent GitHub account totals, token expiry, storage capacity, percentage,
  bytes transferred, ETA, branch count, permission success or backup timestamps.
- Unknown values are null and labeled appropriately. Preserve stale data with a stale
  label rather than presenting it as freshly verified.
- Do not ship `setTimeout(() => success)` or hardcoded repositories in production.
- Mock adapters are isolated under test/demo paths, visibly labeled, and excluded from
  production builds. Mock tests prove UI behavior, not GitHub/GitLab connectivity.
- Buttons require a real command and defined result, or are absent/disabled with an
  honest reason. A disabled OAuth affordance is not a completed OAuth integration.
- Do not create extra "AI", analytics, score, chart or notification features.

## Evidence integrity

Never forge logs, exit codes, screenshots, assertions, owner approval or network
responses. Do not snapshot your own UI and label it the Figma original. Keep source
reference and implementation captures separate. Do not mask the whole page in visual
comparisons, enlarge diff tolerances to hide defects, or silently update golden files.

For missing capabilities: record what was attempted, the actual error and the affected
acceptance check. Never infer that a provider is connected or an operation completed.
Use `BLOCKED`, not fake adapters, for unavailable real integration tests.
