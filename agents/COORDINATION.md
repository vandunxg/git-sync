# Multi-agent coordination

Default to one implementer plus one reviewer. Parallelize only after stable interfaces
exist and independent tasks are ready. More agents do not waive the dependency graph.

| Role | Owns | Must not own simultaneously with another agent |
|---|---|---|
| Coordinator | ready-task selection, decisions, integration, state | state merge, shared dependency changes |
| Rust agent | native domain/adapters for assigned task | IPC definitions, migrations, Cargo.lock without handoff |
| Frontend agent | assigned screens, reusable UI, browser tests | token/icon registry or package lock without handoff |
| QA/reviewer | independent fixtures, adversarial review, acceptance evidence | auto-approving own UI reference |

Use separate worktrees and task branches where the repository owner permits them. Store
local ownership notes under state/locks/ (ignored); record durable assignment summaries
in handoffs. These notes are a coordination protocol, not a distributed locking service.
Task ownership is per file/module, not merely per screen; shared components have one owner.

Schema changes are serialized: propose -> review contract delta -> regenerate/check types
-> implement both adapters -> run contract tests -> handoff. Migrations must have unique
ordered IDs; never rewrite an applied migration. Dependency/lockfile changes are merged by
one assigned integrator. Resolve conflicts by reading both intents, never accepting all
of one side mechanically. Re-run tests on the integrated tree, not only isolated branches.

P00 is serial. P01 establishes contracts. P02 visual work can proceed alongside P03-P07
backend work after P01. P08 integration waits for both UI approval and P07. P09 coverage/
restore work can run independently of UI after P06 with exclusive Git/LFS file ownership.
P10 is a combined release gate, not an opportunity to hide deferred failures.

When context is exhausted, stop at a recoverable point and write HANDOFF.md: task, state,
files, actual commands/results, known defects, unresolved decisions and next exact action.
Never describe an ongoing background worker unless the environment actually has one.
