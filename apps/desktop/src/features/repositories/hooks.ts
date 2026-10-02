import { useCallback, useDeferredValue, useEffect, useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useSearchParams } from "react-router"
import { toast } from "sonner"

import { ACTIVE_JOB_STATES } from "@/components/shared"
import { ERROR_COPY, keys, NativeCommandError, repositoryStateKeys, runNative, useNativePort, useNativeQuery } from "@/lib/api"
import type { Provider, RepositorySummary } from "@/lib/native"
import { PROVIDER_LABEL } from "@/lib/icons"

export function describeError(error: unknown): string {
  if (error instanceof NativeCommandError) return `${ERROR_COPY[error.appError.code]} (${error.appError.code})`
  return error instanceof Error ? error.message : "Unexpected error."
}

/**
 * URL search params with updates that compose: router navigations commit later, so
 * consecutive updates (fast checkbox clicks) build on the last written value instead of
 * the rendered one. External navigation takes over once no write is pending.
 */
export function useUrlState() {
  const [params, setParams] = useSearchParams()
  const latest = useRef<{ params: URLSearchParams; pending: string | null }>({ params, pending: null })
  useEffect(() => {
    const value = params.toString()
    if (latest.current.pending === null || latest.current.pending === value) latest.current = { params, pending: null }
  }, [params])
  const update = useCallback(
    (mutate: (next: URLSearchParams) => void) => {
      const next = new URLSearchParams(latest.current.params)
      mutate(next)
      latest.current = { params: next, pending: next.toString() }
      setParams(next, { replace: true })
    },
    [setParams],
  )
  return [params, update, () => latest.current.params] as const
}

/**
 * Search text that renders immediately in the input while the expensive list query
 * follows the deferred value, so typing never waits on the table.
 */
export function useDeferredSearch(committed: string, commit: (value: string) => void) {
  const [draft, setDraft] = useState(committed)
  const [lastCommitted, setLastCommitted] = useState(committed)
  // External changes (Clear filters, back navigation) replace the draft during render.
  if (committed !== lastCommitted) {
    setLastCommitted(committed)
    setDraft(committed)
  }
  const deferred = useDeferredValue(committed)
  return {
    draft,
    deferred,
    pending: deferred !== committed,
    setDraft: (value: string) => {
      setDraft(value)
      setLastCommitted(value)
      commit(value)
    },
  }
}

/** The plan of the repository's job that is waiting for approval, if the native side reported one. */
export function usePendingPlan(repositoryId: string) {
  const jobs = useNativeQuery([...keys.jobs, "repository-plans"], "gbm_jobs_list", { cursor: null, limit: 100 })
  const forRepo = (jobs.data?.items ?? []).filter((job) => job.repositoryId === repositoryId)
  const waiting = forRepo.find((job) => job.state === "waiting_for_approval" && job.planId !== null)
  const preparing = forRepo.find((job) => job.kind === "plan" && ACTIVE_JOB_STATES.has(job.state) && job.state !== "waiting_for_approval")
  // Only the first page is inspected; when more jobs exist a waiting plan may be unseen,
  // so callers must not queue a second plan in that case.
  const complete = jobs.data !== undefined && jobs.data.nextCursor === null
  return { planId: waiting?.planId ?? null, preparingJobId: preparing?.id ?? null, complete, query: jobs }
}

/** Row/detail commands. Each toast reports what the native side confirmed, nothing more. */
export function useRepositoryCommands() {
  const port = useNativePort()
  const client = useQueryClient()
  const invalidate = () => Promise.all(repositoryStateKeys.map((queryKey) => client.invalidateQueries({ queryKey })))

  const openWorkspace = useMutation({
    mutationFn: (repositoryId: string) => runNative(port, "gbm_repository_open_workspace", { repositoryId }),
    onError: (error) => toast.error("Could not open the folder", { description: describeError(error) }),
  })

  const openProvider = useMutation({
    mutationFn: (args: { repositoryId: string; provider: Provider }) => runNative(port, "gbm_repository_open_provider", args),
    onError: (error, args) => toast.error(`Could not open ${PROVIDER_LABEL[args.provider]}`, { description: describeError(error) }),
  })

  const setIncluded = useMutation({
    mutationFn: (args: { repositories: Pick<RepositorySummary, "id" | "fullName">[]; selected: boolean; expectedRevision: number }) =>
      runNative(port, "gbm_repositories_select", {
        repositoryIds: args.repositories.map((repo) => repo.id),
        selected: args.selected,
        expectedRevision: args.expectedRevision,
      }),
    onSuccess: async (_ack, args) => {
      const name = args.repositories.length === 1 ? args.repositories[0]?.fullName : `${args.repositories.length} repositories`
      toast.success(args.selected ? "Included in backup" : "Excluded from backup", { description: name })
      await invalidate()
    },
    onError: (error) => toast.error("Backup selection was not changed", { description: describeError(error) }),
  })

  const createPlan = useMutation({
    mutationFn: (repositoryId: string) => runNative(port, "gbm_sync_plan_create", { repositoryId }),
    onSuccess: async (result) => {
      toast.success("Sync plan queued", {
        description: `Job ${result.jobId}. Plans that change history wait for your review in Sync Center.`,
      })
      await invalidate()
    },
    onError: (error) => toast.error("Sync was not queued", { description: describeError(error) }),
  })

  return { openWorkspace, openProvider, setIncluded, createPlan }
}
