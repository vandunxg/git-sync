# ADR-010: Typed app-owned command boundary

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-13, REQ-36, REQ-43.

## Decision
Version app-owned IPC, validate native inputs and permissions, generate or verify Rust/TypeScript parity; no renderer general shell/fs.

## Alternatives
Stringly-typed arbitrary commands or frontend direct provider writes are rejected.

## Consequences and verification
A schema-compatible DTO still needs cross-field/business authorization checks; capability files alone are insufficient.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
