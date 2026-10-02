# Interaction, information requests and microcopy

Use English UI copy to match the current design reference; agent status reports are
Vietnamese. Do not add a language switch that does not have complete translations.

## Ask at the point of need

Onboarding asks for folder, provider connection, selection/namespace and initial review,
not 20 advanced settings upfront. Show which information is required and why. Do not ask
for OAuth app secrets when using PAT V1. Never ask the owner to paste credentials into
agent chat or commit them into the repo. PAT creation/help opens verified official docs
in the system browser; the app remains the place of entry.

GitLab namespace/project mappings are previewed before first creation. Destructive ref
changes ask only when a concrete plan needs approval, with exact refs/old-new OIDs and
recovery evidence. Do not display repeated generic "Are you sure?" for harmless filtering.

## Actionable wording

| State | Primary copy | Next action |
|---|---|---|
| Not configured | Connect GitHub to discover repositories | Connect with token |
| No filter matches | No repositories match these filters | Clear filters |
| Stale/offline | Showing the last scan from {observedAt} | Retry scan |
| Auth unavailable | GitLab connection needs attention | Reconnect GitLab |
| Token expiry unknown | Expiry not provided by the provider | Manage connection |
| Pending refs | {n} ref changes awaiting backup | Review / Sync |
| Rewrite/delete | This plan changes existing history | Review changes |
| Dirty workspace | Your local edits will not be changed | Skip workspace update |
| Source inaccessible | Source unavailable; backups are retained | Check access |
| Partial LFS | Git refs synced; LFS payloads incomplete | Retry LFS / View details |
| Canceled uncertain push | Cancellation requested; checking destination | View job |

Do not say "all safe", "always current", "100% protected" or "fully backed up" without
scope and evidence. Commit-count claims need actual graph comparison; otherwise say
`ref changes`. Use exact stages instead of a made-up overall percentage/ETA.

## Common affordances

Clicking a repo opens details; row selection uses a checkbox and never unexpectedly
starts a sync. Icon-only more buttons have `aria-label="Actions for {fullName}"` and
proper menu keyboard behavior. Tooltips complement, not replace, accessible labels.

Dialog primary action requires validated inputs and represents the actual effect.
Focus moves to the title/first field on open, remains trapped, Escape cancels where
safe, and returns to the trigger. A running destructive operation cannot be undone by
simply closing a dialog; show current job status instead.

A settings Save button is dirty-state aware and disabled while saving; preserve edits
on failure and show field/native errors. Close with unsaved changes asks about discard,
not about unrelated backup activity. Select/switch/number controls must match the data
type; do not render every preference as a text field.
