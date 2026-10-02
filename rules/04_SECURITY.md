# R-SEC - trust boundaries and credentials

## Threat model

Untrusted inputs include Git refs, repository names, descriptions, paths, URLs,
.gitattributes/.gitmodules/.lfsconfig, provider responses and subprocess output. Also
consider a compromised renderer, forged native commands, another local process,
interrupted writes and inadvertent credential leakage. An OS keyring is not protection
against a fully compromised unlocked user session; do not promise otherwise.

The renderer receives app-owned IDs, sanitized DTOs and safe actions. Native code
re-resolves IDs, validates current state and authorizes operations. Do not expose
`run_shell`, `execute_git(args)`, unrestricted file read/write, or arbitrary URL fetch.
Constrain custom commands as well as plugins: Tauri capability files alone do not
make a permissive Rust command safe [S02]. Do not allow remote origins/iframes to invoke
native capabilities. Use a production CSP and no remote scripts/fonts.

## Secrets

OS keyring is the persistent secret store; SQLite stores credential references only.
If the keyring is unavailable/locked, show explicit guidance or an opt-in session-only
credential mode. Never silently fall back to plaintext. PAT entry necessarily exists
briefly in renderer memory; do not claim it never enters the renderer. Do not export
it, preserve it in form state after use, or send it on telemetry/events.

The Git auth bridge must avoid tokens in remote URLs, argv, logs, temporary plaintext
files and environment variables. Use a dedicated credential helper with job-scoped,
origin/path-bound authorization and a short-lived native broker. P03 requires a
small verified implementation spike and security review; do not invent a crate API.
For Linux the proposed transport is a restrictive runtime-directory Unix socket;
validate peer identity, endpoint binding, one-job lifetime and cleanup. Portable
transports are behind a boundary and not claimed until tested.

## Execution restrictions

No shell interpolation of input; invoke fixed binaries with argument arrays. Resolve
and record trusted Git/Git LFS binary paths. Reject command-like remotes, option
injection, `ext::`, userinfo URLs and unsupported protocols. V1 production network
origins are HTTPS github.com/api.github.com and gitlab.com, with tightly scoped approved
provider upload/download hosts. Never forward auth headers across origin changes.
Local filesystem remotes are test-only. Enterprise/custom hosts require an extension
review, not an arbitrary URL textbox that bypasses the allowlist.

Run Git with app-managed configuration. Disable repository hooks and unrelated
credential helpers; do not inherit hostile global/system Git settings, URL rewrites,
filter drivers or templates. Allow only reviewed Git LFS filter behavior. Do not
execute builds/hooks/scripts from backed-up source. Validate provider LFS endpoints
and redirects; a .lfsconfig file must not cause credentials to be sent elsewhere.

## Authorization and diagnostics

GitHub is read-only for this application. Request only capabilities needed by actual
endpoints; never sudo/admin/registry scopes for GitLab backups. GitLab create API and
Git push permissions are separate checks. Disconnect removes local credentials; it
does not delete backups or imply provider token revocation.

Logs must redact secrets before storage or UI exposure; include request/job IDs, codes
and bounded messages. Exports are user-initiated and reviewed for private names/paths.
No telemetry in V1. Release builds cannot silently include the template's debug panel.
