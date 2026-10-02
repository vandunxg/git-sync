# ADR-002: System Git as the network engine

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-07, REQ-19, REQ-22.

## Decision
Use supervised system git and git-lfs processes, with domain-owned validated arguments and observable exit/results.

## Alternatives
git2/gix can read data later but are not the initial transport engine; do not reimplement Git.

## Consequences and verification
External process cancellation, full streams, Git config isolation and credential brokering are first-class work.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
