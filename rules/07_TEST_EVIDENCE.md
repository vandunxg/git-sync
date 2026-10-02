# R-QA - tests, evidence and completion

Every task links to test IDs. Implement those tests and record actual commands,
working directory, toolchain, git commit/tree, timestamps, stdout/stderr paths and exit
codes. Keep deterministic mock timestamps/IDs for UI tests, not production dashboards.
The planned [test catalog](../docs/quality/TESTS.json) is not a test-run report.

Do not count `cargo check` as runtime testing, a browser screenshot as a native pass,
or a provider fake as remote integration. Distinguish PASS, FAIL, BLOCKED and NOT_RUN.
Missing binaries, credentials, screenshots or application source must not yield PASS.

Failure-path tests are required: timeout, partial push, stale plan, auth loss, corrupt
DB/mirror, disk full, event reordering, app termination, rate limiting and workspace
conflict. Use isolated temporary repositories and explicitly allowlisted sandbox
accounts; never the owner's real backup namespace as a convenient test fixture.

Golden images can be updated only with owner-approved change evidence. Do not loosen
pixel thresholds, delete tests or skip security checks to make a task green. If an
existing test is wrong, document the contradiction and get the relevant reviewer to
approve its correction separately.

Separate machine evidence from human approval. The agent that implements a UI is not
the approving owner. Reviewers must read the diff and evidence; they cannot rubber-stamp
by repeating another agent's claims. All release-blocking checks must pass on the
release commit after the last material change.
