# ADR-004: SQLite state and OS keyring

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-10, REQ-12, REQ-13.

## Decision
Persist metadata/jobs/plans in SQLite; credentials in the verified OS credential service with opaque references only.

## Alternatives
Plaintext config and renderer storage are rejected; server databases are unnecessary.

## Consequences and verification
Missing/locked Secret Service becomes an actionable blocked state; crash-safe migrations and journals remain necessary.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
