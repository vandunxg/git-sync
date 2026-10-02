# ADR-007: Snapshots and restore scope

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-15, REQ-16, REQ-18, REQ-42.

## Decision
Retain verified full bundles and manifests before destructive operations; report Git/LFS/submodule coverage independently.

## Alternatives
A live GitLab mirror is not immutable history; incremental archive chains deferred.

## Consequences and verification
Requires storage and restore checks; local archive policy does not guarantee offline or immutable protection.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
