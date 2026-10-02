# UI contract - clean, useful, not generic

## Owner expectation

The design must be clean without becoming a sparse wireframe. Use meaningful icons,
clear grouping and usable information density. Do not add charts/cards merely to make
the UI feel "more designed". Do not allow misalignment, collisions or clipped text.
Replace status dots and Unicode action placeholders with actual semantic SVG icons.

## Reference hierarchy

1. Latest owner corrections and accepted design-delta records.
2. Owner-approved screen exports/implementation goldens in BASELINES.json.
3. Refined V2 frame references in FIGMA_MAP.json and each screen's structural spec.
4. Audited shadcn/Tauri primitive styling. The template does not define product layout.

Current baseline status is MISSING. The token file is a candidate; its readable body
size is an explicit proposed correction to the tiny labels in earlier layouts. P02
freezes measured tokens and approved screenshots before claiming fidelity. Do not
silently replicate incorrect permissions, fake data or overlapping layers from Figma.

## Desktop shell

1440x900 is the review viewport. Reference shell: 36 px titlebar, 220 px sidebar,
28 px main padding. Use measured Grid/Flex tracks, not absolute x/y layout. At narrower
widths, collapse only the documented secondary pane; maintain a reachable page title,
primary action and list. Tables may have a local horizontal scroll region, but the whole
app must not accidentally scroll horizontally. Main content and sidebar have clear
independent overflow ownership. Design at 1280x800, 1920x1080 and compact 1024x768 too.

Navigation: Dashboard, Repositories, Sync Center, Activity, Connections, Settings.
Do not add a production "States & Dialogs" nav item; that Figma frame is a design-system
review sheet. Route guards send an unconfigured profile to onboarding, not a fake ready
dashboard. Preserve filters/tab/selection on navigation where appropriate.

## Density and hierarchy

One page title, at most one dominant primary action per area. Dashboard has exactly
three derived summary metrics plus attention list, pipeline and recent activity. Do
not add a health-score dial, sparkline, decorative pie chart or nested card per row.
Group related settings as rows; a card is for a meaningful section, not every field.
Body text candidate 14 px, control 13 px, metadata 12 px; use a readable line height.
Do not shrink labels to conceal insufficient width. Long repository names truncate
with accessible full text; descriptions wrap. Every row has an appropriate minimum height.

## Icon contract

Use contracts/icons.json. StatusBadge is `semantic icon + text`, not colored dot + text.
Use consistent outline weight and fixed slots. Do not use generic RefreshCw for size,
branch, unknown or failed just because it is available. Do not import nonexistent
exports from memory; compile-test installed Lucide exports. No hand-drawn brand marks.

Icons support information, not replace it. Avoid an icon tile for every settings label.
Status color is secondary to icon and text. A live job may animate only its real running
icon, honors reduced motion, and does not imply network progress when unavailable.

## Async and feedback

Loading skeleton -> real data, or honest empty/error state. Empty account and filtered
no-results are distinct. Stale data stays visible with observation time and retry action.
Network failures do not erase cached successful backups. Generic "Something went wrong"
is insufficient where a stable actionable error code exists.

Toasts summarize completed user actions; batch jobs do not emit 1,000 success toasts.
Use inline state for ongoing work. No fake reconnect/sync controls. Exact CTA and native
command mappings are defined per screen. Preserve disabled reasons as accessible text.

## Safety and no-invention deltas

DD-01: Replace status dots/ellipsis characters by semantic SVGs (owner request).
DD-02: Raise tiny body/metadata text to readable candidates; final values await approval.
DD-03: Replace "Authorize" with "Connect with token" in PAT-only V1; no mock OAuth.
DD-04: Never show "Backup healthy" merely because the scheduler runs; use a service
label separately from actual repository health.
DD-05: Dirty-workspace dialog defaults to "Skip workspace update"; no auto-stash or
"Continue anyway" that implies destructive overwrite.
DD-06: Pause queue means pause scheduling, not a false paused Git process.
DD-07: Unsupported root relocation/theme options are not fake working controls.

All deltas are explicit; DD-02/layout changes require owner visual acceptance. Safety
and functional corrections must not be reverted to imitate an old mockup.
