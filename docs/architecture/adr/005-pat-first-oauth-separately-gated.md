# ADR-005: PAT first; OAuth separately gated

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-11, REQ-41.

## Decision
V1 accepts PATs with explicit permission checks. OAuth/App/device flows require separate app registration, threat modeling and tests in deferred P11.

## Alternatives
A fake Authorize button or bundled client secret is rejected.

## Consequences and verification
PAT setup is less seamless, but no unimplemented authorization promise is exposed.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
