# R-FE - React, TypeScript and shadcn/ui

## Structure

Use feature folders and shared primitives. Pages compose components and hooks; they
do not execute Git, parse process output, query SQLite, manage provider secrets, or
reimplement native status derivation. Only `lib/native/` knows Tauri invoke/event APIs.
Imports crossing a feature boundary go through its public exports.

TypeScript strict mode, `noUncheckedIndexedAccess`, and exhaustive discriminated unions
are expected. Use `unknown` + runtime validation at external boundaries, not `any`,
blind assertions or eslint disables. Ensure JSON nullability and casing match contracts.

## State ownership

TanStack Query owns snapshots/list/detail data from native commands. URL/router state
owns filters, selected repo, tabs and pagination. Local state owns open dialogs and
transient form values. Zustand is optional and limited to truly shared ephemeral UI
state; never duplicate Query caches or native job state inside it.

Use Effects only for synchronization with external systems. Derive view data during
render or in memoized selectors when measured; handle user actions in handlers [S09].
Unsubscribe Tauri events on unmount and handle React Strict Mode effect re-runs. Query
responses/events carry revisions so old events cannot overwrite newer data.

## Components and layout

Use shadcn/Radix primitives from the audited template; choose one primitive family,
not mixed generated versions. Compose Button, Dialog, Table, Tabs, Tooltip, Select,
Checkbox and Switch with consistent tokens. Semantic HTML first. Prefer CSS Grid/Flex
with `min-width: 0`, explicit wrapping and local scroll containers; do not translate
Figma x/y coordinates into absolute positioning for ordinary content.

Keep components purpose-focused. A large component prompts review, not an arbitrary
line-count workaround. Extract meaningful responsibilities, not dozens of one-line
wrappers. No CSS !important/negative-margin patching to mask layout bugs.

## Forms and async UI

Use React Hook Form + Zod for substantive forms; field errors have labels and
`aria-describedby`. Validate again in Rust. Cancel/dismiss does not silently submit.
Native Save success updates the cache; an invoke resolving to queued is not completed
backup. Prevent duplicate mutation submissions and preserve form input on recoverable
errors. Never optimistically label a destructive/sync operation as successful.

Loading skeletons preserve layout. Empty, filtered-empty, offline, stale, forbidden and
failed are different states. No fake throughput or ETA. Errors expose a useful next
action and sanitized details. PAT input is short-lived; clear on submit/unmount and
exclude it from persist stores, devtools, analytics and snapshots.

## Accessibility and testing

Visible focus, correct Tab order, Escape to dismiss, focus trapping and restoration
for dialogs, labels on icon-only actions, reduced-motion support, and status text in
addition to color. Tooltips are not a replacement for accessible names.

Testing Library/Vitest verify behavior by roles and names; Playwright verifies flows,
layout and visual regressions. Browser mock tests do not prove native dialogs, keyring,
tray or Hyprland behavior. Native verification is a separate release gate [S17].

References: [S09 React; S10 Playwright; S11 Lucide; S17 Tauri tests](../sources/REFERENCES.md).
