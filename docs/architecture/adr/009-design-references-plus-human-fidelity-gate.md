# ADR-009: Design references plus human fidelity gate

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-23, REQ-24, REQ-25, REQ-26.

## Decision
Use Refined V2 as the historical starting point, apply recorded corrections and freeze real design references separately from implementation goldens.

## Alternatives
Rejected crowded images, sparse old page, self-approved snapshots and screenshot-in-Figma deliverables are not acceptance authority.

## Consequences and verification
P02 visual acceptance is blocked until captures/approval; independent backend work continues.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
