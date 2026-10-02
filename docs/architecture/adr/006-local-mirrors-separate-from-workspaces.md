# ADR-006: Local mirrors separate from workspaces

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-05, REQ-17.

## Decision
Managed full-history bare data and optional independent working clones have separate safety/status semantics.

## Alternatives
Using one editable working tree as the only backup is rejected.

## Consequences and verification
Additional disk use; dirty or ahead workspaces are preserved and can lag independently.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
