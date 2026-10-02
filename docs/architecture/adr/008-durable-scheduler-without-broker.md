# ADR-008: Durable scheduler without broker

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-09, REQ-10, REQ-44.

## Decision
Use native async workers, bounded concurrency, SQLite jobs, single repository writer and one active app instance.

## Alternatives
Redis/Kafka and unconditional OS daemons are outside V1.

## Consequences and verification
Tasks run while the app is alive; exit/suspend/crash recovery must be explicit.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
