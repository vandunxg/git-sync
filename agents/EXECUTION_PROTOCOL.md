# Execution protocol

## 1. Enter with evidence

Read AGENTS.md, the source-of-truth record, scoped rules, task record, linked requirements,
contracts, tests and UI spec. Inspect actual source and repository instructions; this
blueprint is not proof that any dependency, function or test already exists. Record the
current commit, dirty files, operating system, task ID and permitted files. A fresh agent
must re-read the handoff and actual diff instead of relying on remembered progress.

## 2. Check readiness

A task is ready only when its direct task dependencies and phase dependencies have
accepted evidence. P03 follows P01 and does not wait for a missing P02 visual baseline.
For P00 inspection tasks, evidence is executed inspection/probe output and decisions;
linked future acceptance scenarios are constraints, not tests falsely marked passed.
For design-review tasks, source captures and a human review replace compilation as that
task's specific outcome. Product implementation tests remain NOT_RUN until executed.

Use a plan of at most a few actionable steps for the current task. Do not implement all
phases in a single uncontrolled pass. A dependency uncertainty requires reading the
installed source/documentation or a small throwaway probe before production integration.

## 3. Claim ownership

Follow COORDINATION.md. Declare task, worktree/branch, owner, file allowlist and dependencies.
A worktree may contain uncommitted owner changes; do not reset, stash or clean them.
State/evidence updates are a common permitted scope for every task. Shared contracts,
lockfiles and migrations have one owner; request a handoff before changing them.

## 4. Implement one vertical result

Write the smallest implementation satisfying the selected acceptance criteria. Test the
failure and security paths, not only success. Use production adapters only after their
phase implements them. Fixture adapters are explicitly test-only, never fallbacks to a
failed production connection. Keep compilation errors visible; no fake return values,
disabled test bodies, invented progress, silent catch, or broad lint suppression.

New public commands/types require the contract owner. A discovered necessary behavior
outside scope gets a change request with alternatives and impact; do not silently build it.
An unsupported browser/native API is an implementation constraint, not permission to claim
the feature works. Risky external writes need a configured authorized sandbox or owner
approval of the exact operation, not a general permission inferred from this task.

## 5. Verify the result

Run the exact relevant test selectors and quality profiles after the final source change.
Capture command, working directory, commit/diff identifier, exit code, time, environment,
and a redacted log path. A missing command or tool is BLOCKED, not SKIPPED/PASS. Product
scenario IDs in TESTS.json are obligations; the Python tools in this archive do not
implement the Git/React/Tauri acceptance suite.

Run scripts/check_task_scope.py with the recorded task-start commit; review any out-of-scope diff instead of suppressing the guard.

For UI work, capture the implementation at the specified viewport and fixture seed,
compare with separately owner-approved design references, test keyboard/overflow and
inspect native WebKitGTK where applicable. A generated golden needs independent approval;
never update a snapshot merely to make CI green.

## 6. Review and hand off

Use templates/TASK_REPORT.md and HANDOFF.md. A reviewer reads the diff and evidence,
checks failure modes and verifies critical claims. Use REVIEW_READY until accepted. An
implementer cannot self-approve destructive policies or the visual baseline. A tool's
success response proves its particular action, not all product acceptance criteria.

State transitions: NOT_STARTED -> IN_PROGRESS -> REVIEW_READY -> DONE. BLOCKED is allowed
from any unfinished state; record cause, affected scope and unblocking condition. A failed
review returns to IN_PROGRESS. The coordinator updates state/IMPLEMENTATION.json; the
TASKS.json registry defines scope and its initial status, not a second mutable status board.
No commit, push, remote project creation or release publication without current authorization.
