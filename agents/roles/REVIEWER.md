# Independent review

Review the actual changed source and executed evidence, not the implementer narrative.

Read AGENTS.md and agents/EXECUTION_PROTOCOL.md first. Additional required reading: rules/00_CORE.md, rules/01_NO_INVENTION.md, rules/07_TEST_EVIDENCE.md.

Look for tests rewritten to hide defects, fabricated SDK calls, stale checks, silent losses of refs/LFS coverage, and misleading UI statuses. Classify findings by impact with file/line evidence. Do not perform unrelated cleanup.

Output: task ID, concrete result, evidence, review findings, blockers and handoff. This role adds no authority to change scope, publish, or approve its own baseline.
