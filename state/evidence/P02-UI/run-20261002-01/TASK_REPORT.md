# Task report — P02-T01/T02/T03 + owner-directed candidate screens (CR-UI-2026-10-02)

Owner instruction (2026-10-02): build the full UI from docs/design mockups with improvements, using the
create-tauri-ui (shadcn/ui on Base UI) stack. Scope record: CHANGE_REQUEST.md.

## Changed areas
- docs/ui/BASELINES.json, docs/ui/baselines/design/*.png — owner-provided Refined V2 frames registered with
  SHA-256 as `reference_provided` (not approved).
- contracts/icons.json — added `action.copy` (Copy).
- apps/desktop: shadcn primitives (official CLI), candidate tokens in index.css, src/app (router, guard, shell
  with shadcn Sidebar, queue status), src/lib/api (NativePort, query/mutation hooks, actions, error copy),
  src/lib/icons (semantic registry + official brand marks via simple-icons), src/lib/format.ts,
  src/components/shared, src/features/{dashboard,repositories,sync,activity,connections,onboarding,settings},
  src/test/demo (labeled fixture port, demo.html only), src/test/policy/icons.test.ts, e2e/layout.spec.ts.
- Dependencies (exact pins): @tanstack/react-query 5.104.0, react-router 8.4.0, react-hook-form 7.89.0,
  zod 4.6.5, @hookform/resolvers 5.9.1, simple-icons 16.33.0, sonner 2.0.8, @playwright/test 1.63.0 (dev).

## Requirements touched
REQ-23..REQ-35, REQ-41..REQ-45 (presentation + native command wiring through the IPC contract).

## Commands (apps/desktop) — see checks.log
typecheck 0, lint 0, test:unit 0 (115), test:e2e 0 (37; 11 routes x 3 viewports + guard/keyboard),
build 0, quality/checks/no_fakes_dist.py PASS, scripts/check_visual_baselines.py BLOCKED (no owner approval).

## Evidence
candidate-screens/*.png (demo fixture, labeled), checks.log, REVIEW.md, CHANGE_REQUEST.md.

## Limitations
- All native commands still return UNSUPPORTED in the real app; it shows the startup failure screen until
  backend tasks (P03-P07) land. Screens were reviewed with the labeled fixture port only.
- Visual sign-off BLOCKED: owner must approve candidates; DD-02 sizes need acceptance.
- No native WebKitGTK/Hyprland verification; no live event subscription (P07-T03).
- Contract gaps: queue paused state not readable; storage root/free space only via diagnostics text;
  activity has no type/time-range server filter; jobs have no timestamps/duration.
- scripts/check_task_scope.py not run: change spans several tasks under one owner-directed change request.

## Next
Owner reviews candidate screenshots (P02-T04). Backend: P03-T04 is the next ready backend task.
