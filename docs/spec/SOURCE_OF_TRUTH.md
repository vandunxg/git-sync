# Source of truth, evidence and decision status

## Source ledger

| ID | Source | What is authoritative | Limitations |
|---|---|---|---|
| U-01 | Owner's application request in this conversation | GitHub -> GitLab backup + local folder; scan new changes; desktop dashboard; Linux-first Arch/Hyprland | Does not choose every low-level algorithm or version |
| U-02 | Owner's template request | Use agmmnn/tauri-ui as the starting template | Does not authorize copying its sample dashboard |
| U-03 | Owner's UI corrections | Clean but not sparse; meaningful icons; no info/card/chart spam; no overlap; better UX | Requires final per-screen visual approval |
| U-04 | Owner's icon instruction | Replace status dots and text action placeholders with valid semantic icons | Last Figma write failed; not applied in canvas |
| U-05 | Current request | Create spec, phase plan, rules, agent instructions, test enforcement and ZIP | No permission to create remote projects, publish or implement the app now |
| F-01 | Figma file NCVVCfO9j9aMyh0E6YRXwU, page 6:2 | Refined V2 layout as a reference candidate | Not an accepted pixel-perfect baseline; fresh access is blocked |
| H-01 | Earlier assistant architecture/design suggestions | Context and possible implementation direction | Not automatically owner-approved requirements; contains superseded unsafe shortcuts |
| O-01 | Official sources in sources/REFERENCES.md | API behavior at the verified documentation version/date | Must recheck against installed versions during implementation |

## Figma provenance

File: https://www.figma.com/design/NCVVCfO9j9aMyh0E6YRXwU
Page: `Git Backup Manager - Refined V2` (actual prior page name uses an em dash), ID `6:2`.
Known frame IDs are in `docs/ui/FIGMA_MAP.json` and come from earlier successful tool
outputs. They are not guessed. On 2026-10-01 this run attempted `get_metadata` for `6:2`;
the connector returned a Starter MCP tool-call-limit error. No new exports were captured.

The old `Clean UI` page and the generated dark screenshots are rejected design directions.
Do not include them as golden images. The old `outside=0` outer-bounds audit did not
check sibling collisions, readability, true fonts or all clipping; it is not UI approval.

## Interpreting this blueprint

`OWNER_REQUIREMENT`: directly requested by the owner above.
`SPEC_DEFAULT`: a selected implementation default to make tasks executable, reversible
where possible, not represented as a fact the owner explicitly stated.
`SAFETY_CONSTRAINT`: a guard against data loss or disclosure; cannot be weakened silently.
`OPEN_GATE`: information/approval required at the affected phase, not a license to guess.
`DEFERRED`: excluded until an accepted change creates tasks and tests.

The architecture direction from the conversation is used as the baseline stack.
Per-repo GitLab mapping, destructive-change approval, PAT-first auth, specific dimensions
and scheduling numbers are explicit spec defaults. See `docs/architecture/DECISIONS.json`.
No destination namespace, credentials, app registration ID, target repository URL,
package version or final visual approval has been fabricated.

## Conflict resolution

1. Identify conflicting source IDs and the affected acceptance check.
2. Apply explicit later owner corrections over old sketches. Preserve security guards.
3. Do not "copy bugs" from Figma (fake data, wrong permissions, missing actions) into code.
4. Where a correction has no approved visual, produce a candidate and request only that
   approval. Backend/contract work not dependent on it can continue.
5. Record accepted changes in a change record, update requirements/contracts/tests, then
   implement. Do not silently amend this ledger to make an output appear compliant.
