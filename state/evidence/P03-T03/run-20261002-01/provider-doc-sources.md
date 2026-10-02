# Provider API facts used by P03-T03 (official docs fetched 2026-10-02)

| Fact | Source |
|---|---|
| `GET /user`; headers `Authorization: Bearer`, `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2026-03-10`; 200/304/401/403; `id` int64, `login` | https://docs.github.com/en/rest/users/users#get-the-authenticated-user |
| `X-OAuth-Scopes` lists scopes the token has; `X-Accepted-OAuth-Scopes`; `repo` = full access to public and private repos; `public_repo` = public repos only | https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps |
| `retry-after` header; `x-ratelimit-remaining: 0` on primary rate limit; `X-Accepted-GitHub-Permissions` for fine-grained | https://docs.github.com/en/rest/using-the-rest-api/troubleshooting-the-rest-api |
| `GET /personal_access_tokens/self`, `PRIVATE-TOKEN` header, fields id/name/revoked/created_at/scopes/user_id/last_used_at/active/expires_at; 401 if token does not exist / no access | https://docs.gitlab.com/api/personal_access_tokens/ |
| Scope meanings; `api` for a PAT also grants read/write Git-over-HTTP; `write_repository` = pull+push; `sudo` = admin impersonation | https://docs.gitlab.com/security/tokens/access_token_scopes/ |
| 401/403/404/429 semantics, `Retry-After` on 429 | https://docs.gitlab.com/api/rest/troubleshooting/ |

Not verified (therefore not used): a GitHub token-expiration response header; GitLab error body for
expired/revoked tokens. `GET /api/v4/user` response fields id/username are the standard user API fields
used by the GitLab `read_user` scope description; not separately re-fetched.
