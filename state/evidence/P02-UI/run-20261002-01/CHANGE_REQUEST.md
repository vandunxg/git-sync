# Change request CR-UI-2026-10-02 — owner-directed full candidate UI

ID / related requirements and tasks: P02-T01, P02-T02, P02-T03 (in scope) and candidate presentation for
P08-T01..P08-T04 screens; REQ-23..REQ-35, REQ-41..REQ-45.

Current requirement / observed gap: TASKS.json orders P02 (shell + dashboard/repository slice) before P08
(remaining screens, gated on P07 backend). The owner (2026-10-02, chat) supplied Refined V2 frame exports in
docs/design/ and asked to build the complete UI from them, filling gaps and improving the mockups.

Why the requested change is necessary: owner instruction; AGENTS.md defers to repository-owner instructions.

Proposed minimum change:
- Register the owner-provided frames as design references (BASELINES.json status `reference_provided`,
  not `approved`).
- Build all screens against the real IPC contract through one `NativePort`. Production injects the Tauri
  client (`lib/native`); every native command still returns UNSUPPORTED until its backend task lands, and the
  UI shows that honestly (startup failure / typed error states).
- A labeled fixture port lives only under `src/test/demo/` with its own `demo.html` entry for visual review;
  it is not part of the production bundle (vite builds index.html only; TEST-NO-FAKES guards both levels).

Alternatives: (a) stop after P02-T03 slice — rejected by owner request; (b) put fixtures behind a runtime
flag in the production bundle — rejected (R-FACT: mocks excluded from production builds).

Affected UI/IPC/DB/security/backward compatibility/tests: UI only. No IPC/DTO/DB change. New pinned
frontend dependencies (see DEPENDENCIES section of the task report). No secret persistence: PAT input is
form-local and cleared on submit/close.

Destructive, remote or privacy effect: none in this change; all write actions call native commands that
are still UNSUPPORTED.

Design deltas applied beyond the mockups (all within DD-01..DD-07 or the screen specs):
- Blank tiles and dot glyphs replaced by semantic Lucide icons and official GitHub/GitLab marks (DD-01).
- Readable text sizes per candidate tokens (DD-02, needs owner visual acceptance).
- "Authorize" -> "Connect with token" (DD-03); sidebar footer shows queue state, not "Backup healthy" (DD-04).
- Per-job Pause/Start, Activity type/time-range server filters, fake throughput/ETA removed: not in contract.
- Theme limited to Light (DD-07); root relocation shown as unsupported.
- Custom title bar from the frames is not drawn: the native window decoration provides it.

Provenance and owner decision needed: owner must review candidate screenshots and approve or correct them
before any implementation golden is recorded (P02-T04). Contract gaps found: queue paused state is not
readable from any DTO; storage root/free space is not readable after setup except via diagnostics text.

Decision: ACCEPTED for candidate implementation (owner chat instruction 2026-10-02); visual approval OPEN.
Decision maker, date and evidence: repository owner, 2026-10-02, session instruction "design mockup
docs/design/ tiến hành dựng UI ... bổ sung và cải tiến để cho ra UI hoàn thiện" and "UI yêu cầu: create-tauri-ui".
