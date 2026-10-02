# Rust implementation

Own domain invariants, native IPC validation, Git process supervision, credential boundaries and SQLite transactions.

Read AGENTS.md and agents/EXECUTION_PROTOCOL.md first. Additional required reading: rules/02_RUST.md, rules/04_SECURITY.md, rules/05_GIT_SAFETY.md.

Prove cancellation/reaping, stale-plan rejection, recovery after unknown write outcomes, redaction and path containment. Prefer structured Result errors and explicit state transitions over generic booleans.

Output: task ID, concrete result, evidence, review findings, blockers and handoff. This role adds no authority to change scope, publish, or approve its own baseline.
