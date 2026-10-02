import type { AccountSummary, Provider } from "@/lib/native"

/** Static, verified-by-docs copy per provider. No URL is opened from the renderer. */
export interface ProviderCopy {
  role: string
  accountRole: string
  purpose: string
  permissions: string
  scopeHelp: string
  docsUrl: string
}

export const PROVIDER_COPY: Record<Provider, ProviderCopy> = {
  github: {
    role: "Source · read only",
    accountRole: "Source account",
    purpose: "Repositories are read from GitHub. Git Backup Manager never writes to your GitHub account.",
    permissions: "Repository permissions: Contents (read-only) and Metadata (read-only).",
    scopeHelp:
      "Create a fine-grained personal access token with read-only Contents and Metadata permissions for the repositories you want to back up. Only repositories the token can read are backed up.",
    docsUrl:
      "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens",
  },
  gitlab: {
    role: "Destination · creates and updates mirrors",
    accountRole: "Backup destination",
    purpose: "Creates missing backup projects and pushes mirrored refs when GitHub changes.",
    permissions: "Token scopes: api, read_repository and write_repository.",
    scopeHelp:
      "Create a personal access token with the api, read_repository and write_repository scopes. Registry, sudo and admin scopes are not needed.",
    docsUrl: "https://docs.gitlab.com/user/profile/personal_access_tokens/",
  },
}

/** The account to show for a provider: a connected one wins over stale records. */
export function accountFor(accounts: AccountSummary[] | undefined, provider: Provider): AccountSummary | null {
  const matches = (accounts ?? []).filter((account) => account.provider === provider)
  return matches.find((account) => account.authState === "connected") ?? matches[0] ?? null
}

/** An account with a usable stored credential (not disconnected/never stored). */
export function hasCredential(account: AccountSummary | null): boolean {
  return account !== null && account.authState !== "disconnected" && account.storageMode !== "none"
}
