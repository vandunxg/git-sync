# ADR-001: Modular Tauri desktop

## Status
Selected blueprint direction. Owner-specific runtime choices and the visual baseline remain separate gates. This status is not a fabricated human sign-off.

## Context
The owner wants a Linux-first GitHub-to-GitLab/local backup application and scoped, verifiable AI implementation. Related requirements: REQ-01, REQ-02.

## Decision
Tauri 2 plus Rust native application services and React/TypeScript/Vite views, using the requested agmmnn template as an audited scaffold.

## Alternatives
Electron offers bundled rendering consistency but was not the selected direction; a hosted server adds operations and conflicts with local-first scope.

## Consequences and verification
Native Linux libraries and WebKitGTK behavior require recorded native tests; web mocks do not replace them.

Changes require a documented impact review of safety, contracts, UI and acceptance scenarios. See [decision ledger](../DECISIONS.json) and [source authority](../../spec/SOURCE_OF_TRUTH.md).
