# Credentials, account setup and application lifecycle

## V1 PAT flow

1. Select provider; display minimum capabilities needed and their purpose.
2. Enter token in a masked field. No token-shaped placeholder, sample real token or
   clipboard auto-read. Do not put the token in URL query parameters.
3. Submit through a dedicated secret-bearing command exempted from parameter logging.
4. Native code tests identity and relevant capabilities without attempting a write
   to the user's real repositories. Return a safe account summary, not the token.
5. Store persistently only after a usable OS keyring is confirmed. Otherwise offer a
   clearly labeled session-only choice; explain that restarting requires reconnection.
6. Clear renderer input. SQLite stores keyring reference, account ID and safe metadata.

Avoid over-claiming token permissions. GitHub fine-grained repository read/metadata
permissions are preferred where they meet the selected scope; some account/org access
needs another grant and must be surfaced, not hidden. GitLab API operations and Git
read/write are verified independently. Do not request registry/sudo scopes for backups.
Actual provider scope/version constraints are verified at implementation time [S05, S12].

## Rotation and disconnect

Test replacement credentials before swapping references. Use a per-account lock to
avoid two concurrent updates. Handle failure to persist/revoke cleanly. Disconnect
cancels or pauses affected jobs, removes local keyring entries, marks the account
unavailable and preserves all backups. Local disconnect is not provider-side token
revocation; instruct the user where appropriate. Never expose stored tokens via a
"show token" command. Auth expiry is unknown unless provided/verified, not fabricated.

## Browser authorization extension

OAuth is deferred (P11), not a fake primary action. If V1 contains a button, label it
`Connect with token`; an unconfigured future browser flow is hidden or disabled with
an explicit reason. GitHub App user/device flow and GitLab PKCE/device flows require
registered application IDs and current provider-specific token/refresh behavior
[S06, S15]. A GitHub App private key or OAuth confidential-client secret must not be
bundled in a public desktop binary. Successful user authorization is not the same as
all repository installations being accessible.

## Native capabilities under Hyprland

Probe Git/Git LFS, keyring availability/lock state, browser opener, folder dialog,
notification service and tray host separately. Surface missing tools with retryable
setup diagnostics. Do not infer service availability from the distro or desktop name.
Use an explicit quit action. Close-to-tray is allowed only when tray/reopen capability
is verified; otherwise retain an accessible window or ask the user to quit safely.

Auto scan/sync runs only while the application process is alive. Closing a window,
minimizing, system suspend and quitting have distinct semantics. On resume coalesce
missed polls, revalidate credentials and observations, and do not launch a burst of
missed-interval jobs. V1 does not install a daemon or promise sync after process exit.

App shutdown requests cancellation, stops scheduling, waits for bounded child cleanup,
persists interrupted jobs and exits. Restart reconciles those jobs before continuing.
Changing backup root after repositories are configured is read-only in V1: relocation
needs a separate migration design, not an unimplemented Browse button.
