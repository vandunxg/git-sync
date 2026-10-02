import { useQuery } from "@tanstack/react-query"

import { ConfirmDialog, ErrorState, REPOSITORY_STATUS, StatusBadge } from "@/components/shared"
import { Skeleton } from "@/components/ui/skeleton"
import { collectSelectedRepositories, keys, NativeCommandError, useNativePort, useSyncRepositoriesAction } from "@/lib/api"
import { plural } from "@/lib/format"

import { breakdownSyncAll } from "./attention.ts"

/**
 * Sync all states exact counts read from every selected repository before anything is
 * queued. It only creates sync plans: history-changing plans still wait for review and
 * running or blocked repositories are excluded. Nothing is auto-approved.
 */
export function SyncAllDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const port = useNativePort()
  const sync = useSyncRepositoriesAction()
  const selection = useQuery({
    queryKey: [...keys.repositories, "sync-all-selection"],
    queryFn: () => collectSelectedRepositories(port),
    enabled: open,
    staleTime: 0,
    gcTime: 0,
  })

  const breakdown = selection.data ? breakdownSyncAll(selection.data.items) : null
  const count = breakdown?.included.length ?? 0

  const confirm = () => {
    if (!breakdown || count === 0) return
    sync.run(
      breakdown.included.map((item) => item.id),
      { onSettled: () => onOpenChange(false) },
    )
  }

  let body
  if (selection.isPending) {
    body = (
      <div className="flex flex-col gap-2" aria-busy="true">
        <span className="sr-only" role="status">
          Reading selected repositories…
        </span>
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    )
  } else if (selection.isError) {
    const appError = selection.error instanceof NativeCommandError ? selection.error.appError : null
    body = (
      <ErrorState
        error={appError}
        title="Could not read selected repositories"
        onRetry={() => void selection.refetch()}
        retrying={selection.isFetching}
      />
    )
  } else if (breakdown) {
    body = (
      <div className="flex flex-col gap-3">
        <section aria-labelledby="sync-all-included">
          <h3 id="sync-all-included" className="mb-1.5 text-xs font-semibold text-muted-foreground">
            Included ({count.toLocaleString("en")})
          </h3>
          {breakdown.includedByStatus.length > 0 ? (
            <ul className="flex flex-col gap-1.5">
              {breakdown.includedByStatus.map(({ status, count: n }) => (
                <li key={status} className="flex items-center justify-between gap-3">
                  <StatusBadge status={REPOSITORY_STATUS[status]} />
                  <span className="tabular-nums">{n.toLocaleString("en")}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">No selected repository needs a sync right now.</p>
          )}
        </section>
        <section aria-labelledby="sync-all-excluded">
          <h3 id="sync-all-excluded" className="mb-1.5 text-xs font-semibold text-muted-foreground">
            Not included
          </h3>
          <ul className="flex flex-col gap-1 text-muted-foreground">
            <li>
              {plural(breakdown.needsReview, "repository", "repositories")} {breakdown.needsReview === 1 ? "needs" : "need"} review — open
              each one to review its changes.
            </li>
            <li>{plural(breakdown.running, "repository", "repositories")} already running.</li>
            <li>{plural(breakdown.upToDate, "repository", "repositories")} already up to date.</li>
          </ul>
        </section>
        {!selection.data.complete ? (
          <p className="rounded-md bg-warning-soft px-3 py-2 text-xs text-warning">
            Discovery is incomplete; repositories that have not been discovered yet are not included.
          </p>
        ) : null}
      </div>
    )
  }

  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      icon="action.sync"
      title="Sync selected repositories?"
      description={
        breakdown
          ? count > 0
            ? `This queues a sync plan for ${plural(count, "repository", "repositories")}. Plans that change existing history wait for your review; nothing is approved automatically.`
            : "There is nothing to queue."
          : "Reading every selected repository to state exact counts."
      }
      confirmLabel={count > 0 ? `Queue ${plural(count, "sync", "syncs")}` : "Queue sync"}
      pendingLabel="Queuing…"
      onConfirm={confirm}
      pending={sync.pending}
      disabled={!breakdown || count === 0}
    >
      {body}
    </ConfirmDialog>
  )
}
