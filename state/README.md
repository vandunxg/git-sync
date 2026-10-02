# Implementation state

IMPLEMENTATION.json records application work only. All tasks begin NOT_STARTED.
Package script tests are documented in PACKAGE_VALIDATION.md, not mislabeled as app tests.
TOOLCHAIN.json is deliberately unresolved until the actual repository/environment is inspected.

Task evidence goes in `state/evidence/<task-id>/<run-id>/` or a configured external
private directory. Do not commit secrets/private raw logs. Review records and sanitized
summaries can be committed when authorized. An agent may record evidence but must not
impersonate the approving owner or edit the past failed result to look successful.
