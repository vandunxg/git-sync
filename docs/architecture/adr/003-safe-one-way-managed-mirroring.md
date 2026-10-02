# ADR-003: Safe one-way managed mirroring

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-06, REQ-20, REQ-21.

## Decision
GitHub is authoritative. Explicit heads/tags, exact expected-OID leases and reviewed destructive plans replace blind --mirror.

## Alternatives
Two-way merge and always-force mirroring are rejected for V1.

## Consequences and verification
More review for rewritten history; safer preservation of destination drift and history.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
