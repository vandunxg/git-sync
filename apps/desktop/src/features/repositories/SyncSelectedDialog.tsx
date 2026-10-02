import { useQuery } from "@tanstack/react-query"

import { ConfirmDialog, LoadingLabel, REPOSITORY_STATUS, StatusBadge } from "@/components/shared"
import { Skeleton } from "@/components/ui/skeleton"
import { runNative, SYNC_ELIGIBLE, useNativePort, useSyncRepositoriesAction } from "@/lib/api"
import { plural } from "@/lib/format"
import type { RepositorySummary } from "@/lib/native"

import { describeError } from "./hooks.ts"

/**
 * Confirms a bulk sync with exact counts. Summaries come from the native side for every
 * selected ID (not only the visible page); blocked/running items are excluded and listed.
 */
export function SyncSelectedDialog({
  open,
  onOpenChange,
  selectedIds,
  known,
  onQueued,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  selectedIds: string[]
  known: RepositorySummary[]
  onQueued: () => void
}) {
  const port = useNativePort()
  const sync = useSyncRepositoriesAction()
  const knownById = new Map(known.map((repo) => [repo.id, repo]))
  const missing = selectedIds.filter((id) => !knownById.has(id))

  const fetched = useQuery({
    queryKey: ["repository", "batch", ...missing],
    queryFn: () => Promise.all(missing.map((repositoryId) => runNative(port, "gbm_repository_get", { repositoryId }))),
    enabled: open && missing.length > 0,
  })

  const resolved = [...knownById.values(), ...(fetched.data ?? [])].filter((repo) => selectedIds.includes(repo.id))
  const loading = missing.length > 0 && fetched.isPending
  const eligible = resolved.filter((repo) => SYNC_ELIGIBLE.has(repo.primaryStatus))
  const review = resolved.filter((repo) => repo.primaryStatus === "blocked")
  const running = resolved.filter((repo) => repo.primaryStatus === "running")

  const confirm = () => {
    sync.run(
      eligible.map((repo) => repo.id),
      {
        onSuccess: (outcome) => {
          if (outcome.queued > 0) onQueued()
          onOpenChange(false)
        },
      },
    )
  }

  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      icon="action.sync"
      title={loading ? "Sync selected repositories" : `Sync ${plural(eligible.length, "repository", "repositories")}?`}
      description={
        loading ? (
          <p>Checking the current state of {plural(selectedIds.length, "selected repository", "selected repositories")}…</p>
        ) : (
          <p>
            A sync plan is queued for each eligible repository. Fast-forward updates proceed; plans that change existing
            history wait for your review and are never applied by a bulk sync.
          </p>
        )
      }
      confirmLabel={`Queue ${plural(eligible.length, "sync", "syncs")}`}
      pendingLabel="Queuing…"
      pending={sync.pending}
      disabled={loading || fetched.isError || eligible.length === 0}
      onConfirm={confirm}
    >
      {loading ? (
        <div className="flex flex-col gap-2" aria-busy="true">
          <LoadingLabel label="Loading selected repositories" />
          <Skeleton className="h-4 w-2/3" />
          <Skeleton className="h-4 w-1/2" />
        </div>
      ) : fetched.isError ? (
        <p className="text-danger">Could not read the selected repositories: {describeError(fetched.error)}</p>
      ) : (
        <div className="flex flex-col gap-3">
          <p>
            <span className="font-medium">{plural(eligible.length, "repository", "repositories")}</span> will be queued
            {review.length + running.length > 0 ? `; ${review.length + running.length} excluded.` : "."}
          </p>
          <ExcludedList title="Needs review — open each repository to review its plan" items={review} />
          <ExcludedList title="Already running" items={running} />
        </div>
      )}
    </ConfirmDialog>
  )
}

function ExcludedList({ title, items }: { title: string; items: RepositorySummary[] }) {
  if (items.length === 0) return null
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-xs font-medium text-muted-foreground">{title}</p>
      <ul className="flex flex-col gap-1.5">
        {items.map((repo) => (
          <li key={repo.id} className="flex min-w-0 items-center justify-between gap-2">
            <span className="truncate" title={repo.fullName}>
              {repo.fullName}
            </span>
            <StatusBadge status={REPOSITORY_STATUS[repo.primaryStatus]} />
          </li>
        ))}
      </ul>
    </div>
  )
}
