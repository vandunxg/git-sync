// Query keys for native read models. Mutations invalidate by these prefixes.
export const keys = {
  bootstrap: ["bootstrap"] as const,
  dashboard: ["dashboard"] as const,
  accounts: ["accounts"] as const,
  repositories: ["repositories"] as const,
  repository: (id: string) => ["repository", id] as const,
  refs: (id: string) => ["refs", id] as const,
  jobs: ["jobs"] as const,
  activity: ["activity"] as const,
  logs: (jobId: string) => ["logs", jobId] as const,
  settings: ["settings"] as const,
  diagnostics: ["diagnostics"] as const,
  namespaces: (accountId: string) => ["namespaces", accountId] as const,
  plan: (planId: string) => ["plan", planId] as const,
  queuePaused: ["queue-paused"] as const,
}

/** Everything that reflects repository or job state. */
export const repositoryStateKeys = [keys.dashboard, keys.repositories, ["repository"], keys.jobs, keys.activity]
