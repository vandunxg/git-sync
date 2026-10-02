# R-CORE - execution and scope

MUST, MUST NOT and SHOULD are project requirements, not suggestions to ignore when
convenient. A SHOULD exception requires a recorded reason and reviewer sign-off.

## Source hierarchy

Platform/security rules -> explicit owner decisions for this project -> accepted
change records -> approved requirement/contract/UI baseline -> implementation and
verified official API documentation -> historical assistant sketches. Read the
[source ledger](../docs/spec/SOURCE_OF_TRUTH.md) for conflict resolution.

Do not quietly "fix" conflicting product decisions. Record the discrepancy with IDs
and continue only work whose correctness does not depend on it. User-requested safety
and fidelity corrections override obsolete visuals, including dot status markers.

## Minimal complete unit

Implement one ready task and its tests. Include loading, empty, error and cancellation
behavior when the task creates an asynchronous operation. Do not expand a narrow fix
into a new architecture, design style or additional feature. A task may touch a file
outside its path allowance only after coordinator approval recorded in its evidence.

Inspect before changes, verify after changes, preserve unrelated behavior. Prefer
small explicit modules over microservices, generic frameworks and speculative layers.
Do not hide unfinished integrations behind unconditional success or production mock data.

## Status and authority

Task states: NOT_STARTED -> IN_PROGRESS -> IN_REVIEW -> DONE; BLOCKED can interrupt
any non-DONE state. Only an owner or designated reviewer can accept a visual baseline
or a security-sensitive exception. An agent cannot impersonate that approval.

No task may be marked DONE based solely on source inspection, typechecking, generated
screenshots, a successful push, or an unexecuted test plan. Match evidence to the task's
acceptance checks. Report failed, skipped and unavailable checks distinctly.

## Change control

A new button, command, state, provider permission, database table or background job
must trace to a requirement or accepted change request. Improving implementation
without changing behavior is allowed; changing visible workflow or destructive policy
requires updating linked contracts, tests and the decision ledger first.
