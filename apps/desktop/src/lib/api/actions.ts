// Cross-screen user actions. Each reports exactly what the native side confirmed:
// "queued" is never presented as completed backup (R-FE async rules).

import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import type { AppError, RepoListResult, RepositorySummary } from "@/lib/native"

import { ERROR_COPY, NativeCommandError } from "./errors.ts"
import { runNative, useNativeQuery } from "./hooks.ts"
import { keys, repositoryStateKeys } from "./keys.ts"
import { useNativePort, type NativePort } from "./port.tsx"

function errorText(error: unknown): string {
  return error instanceof NativeCommandError ? `${ERROR_COPY[error.appError.code]} (${error.appError.code})` : "Unexpected error."
}

export function useAccounts() {
  return useNativeQuery(keys.accounts, "gbm_accounts_list", {})
}

/** Queue a GitHub discovery scan for the connected source account. */
export function useScanAction() {
  const port = useNativePort()
  const client = useQueryClient()
  const accounts = useAccounts()
  const source = accounts.data?.items.find((account) => account.provider === "github" && account.authState === "connected")
  const mutation = useMutation<unknown, NativeCommandError | Error>({
    mutationFn: async () => {
      if (!source) throw new Error("Connect GitHub before scanning.")
      return runNative(port, "gbm_source_scan_enqueue", { accountId: source.id, reconcileRefs: true })
    },
    onSuccess: async () => {
      toast.success("Scan queued", { description: "Progress is shown in Sync Center." })
      await Promise.all(repositoryStateKeys.map((queryKey) => client.invalidateQueries({ queryKey })))
    },
    onError: (error) => toast.error("Scan was not queued", { description: error instanceof NativeCommandError ? errorText(error) : error.message }),
  })
  return {
    run: () => mutation.mutate(),
    pending: mutation.isPending,
    available: source !== undefined,
    unavailableReason: accounts.isPending ? "Checking connections…" : source ? null : "Connect GitHub to scan repositories.",
  }
}

export interface BulkOutcome {
  queued: number
  failed: { repositoryId: string; error: AppError | null }[]
}

/** Queue sync plans for explicit repository IDs; one summary toast per batch. */
export function useSyncRepositoriesAction() {
  const port = useNativePort()
  const client = useQueryClient()
  const mutation = useMutation<BulkOutcome, Error, string[]>({
    mutationFn: async (repositoryIds) => {
      const outcome: BulkOutcome = { queued: 0, failed: [] }
      for (const repositoryId of repositoryIds) {
        try {
          await runNative(port, "gbm_sync_plan_create", { repositoryId })
          outcome.queued += 1
        } catch (error) {
          outcome.failed.push({ repositoryId, error: error instanceof NativeCommandError ? error.appError : null })
        }
      }
      return outcome
    },
    onSuccess: async (outcome) => {
      if (outcome.queued > 0) {
        toast.success(outcome.queued === 1 ? "Sync queued" : `Sync queued for ${outcome.queued} repositories`, {
          description:
            outcome.failed.length > 0
              ? `${outcome.failed.length} could not be queued. Plans that change history wait for review.`
              : "Plans that change history wait for your review.",
        })
      } else if (outcome.failed.length > 0) {
        const first = outcome.failed[0]?.error
        toast.error("Sync was not queued", { description: first ? `${ERROR_COPY[first.code]} (${first.code})` : undefined })
      }
      await Promise.all(repositoryStateKeys.map((queryKey) => client.invalidateQueries({ queryKey })))
    },
  })
  return { run: mutation.mutate, runAsync: mutation.mutateAsync, pending: mutation.isPending }
}

/** Statuses that a plain "Sync" may queue; blocked/running need their own flow. */
export const SYNC_ELIGIBLE: ReadonlySet<RepositorySummary["primaryStatus"]> = new Set(["update", "failed", "missing", "stale", "unknown", "synced"])

/** Reads every selected repository page so "Sync all" states exact counts. */
export async function collectSelectedRepositories(port: NativePort): Promise<{ items: RepositorySummary[]; complete: boolean }> {
  const items: RepositorySummary[] = []
  let cursor: string | null = null
  for (let page = 0; page < 50; page += 1) {
    const result: RepoListResult = await runNative(port, "gbm_repositories_list", {
      cursor,
      limit: 200,
      search: "",
      status: null,
      visibility: "all",
      selectedOnly: true,
      sort: "name_asc",
    })
    items.push(...result.items)
    cursor = result.nextCursor
    if (cursor === null) return { items, complete: result.discoveryComplete }
  }
  return { items, complete: false }
}
