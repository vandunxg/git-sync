# R-DEPS - dependency and template policy

P00 records the chosen Rust toolchain/MSRV, Node version, package manager, Tauri major,
frontend primitive family and actual resolved dependency versions. Do not invent exact
versions in this package. Inspect upstream license and repository before adopting code.
The template is a scaffold, not a trusted complete implementation or visual baseline.

Audit the current agmmnn/tauri-ui CLI help and selected Vite/React output. Disable or
remove demo dashboards, sample invokes, permissive capabilities, release automation and
dev debug features not required here. Record the upstream commit or package checksum.
Do not rerun an unpinned scaffolder over an existing implementation.

Use one package manager/lockfile. Commit Cargo.lock for the application and the selected
frontend lockfile when commits are authorized. Prefer stable maintained crates for
concrete needs; no speculative plugin framework or unnecessary server dependencies.
Evaluate licensing, transitive dependencies and security advisories. A new dependency
needs purpose, alternatives, version proof, bundle/runtime cost and reviewer decision.

Rustfmt/clippy, dependency vulnerability/license scans and frontend lint/typecheck/build
are separate checks. Missing audit tooling is BLOCKED for release, not a reason to
silently remove the gate. Updates to Git, WebKitGTK or system packages require a native
compatibility check on the supported environment.
