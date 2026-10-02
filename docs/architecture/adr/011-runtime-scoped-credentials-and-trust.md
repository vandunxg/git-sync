# ADR-011: Runtime-scoped credentials and trust

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-14, REQ-22, REQ-45.

## Decision
Authorize provider host/repo/job and serve credentials through a tested short-lived native broker; no secrets in argv/env/URLs/files.

## Alternatives
Simple token-in-URL or generic persistent askpass scripts are rejected.

## Consequences and verification
The credential transport spike is an explicit blocker until implemented/tested; later platform transports need their own evidence.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
