import { useInfiniteQuery } from "@tanstack/react-query"
import { Link } from "react-router"

import { activityVisual, EmptyState, ErrorState, IconTile, RowSkeletons, SectionCard } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { keys, NativeCommandError, runNative, useNativePort } from "@/lib/api"
import { dateTime, relativeTime } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { ActivityList } from "@/lib/native"

const PAGE = 25

/** Recorded activity for one repository, each linked to its bounded details/logs. */
export function HistoryTab({ repositoryId }: { repositoryId: string }) {
  const port = useNativePort()
  const history = useInfiniteQuery<ActivityList, NativeCommandError>({
    queryKey: [...keys.activity, "repository-history", repositoryId],
    queryFn: ({ pageParam }) =>
      runNative(port, "gbm_activity_list", {
        cursor: (pageParam as string | null) ?? null,
        limit: PAGE,
        repositoryId,
        severity: "all",
        search: "",
      }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextCursor,
  })
  const entries = history.data?.pages.flatMap((page) => page.items) ?? []
  const detail = `/repositories/${encodeURIComponent(repositoryId)}`

  return (
    <SectionCard icon="nav.activity" title="History" description="Scans, syncs and project events recorded for this repository">
      {history.isPending ? (
        <RowSkeletons rows={5} />
      ) : history.isError && entries.length === 0 ? (
        <ErrorState error={history.error.appError} title="Could not load history" onRetry={() => void history.refetch()} retrying={history.isFetching} />
      ) : entries.length === 0 ? (
        <EmptyState className="py-6" title="No history yet" description="Events appear after the first scan or sync of this repository." />
      ) : (
        <div className="flex flex-col gap-3">
          <ol className="flex flex-col divide-y">
            {entries.map((entry) => {
              const visual = activityVisual(entry)
              return (
                <li key={entry.id} className="flex min-w-0 items-start gap-3 py-3 first:pt-0">
                  <IconTile icon={visual.icon} tone={visual.tone} size="sm" />
                  <div className="flex min-w-0 flex-1 flex-col gap-1">
                    <p className="text-sm break-words">{entry.message}</p>
                    <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                      <time dateTime={entry.occurredAt} title={dateTime(entry.occurredAt) ?? undefined}>
                        {relativeTime(entry.occurredAt) ?? "Unknown time"}
                      </time>
                      <span>{visual.label}</span>
                      {entry.errorCode ? <code className="font-mono">{entry.errorCode}</code> : null}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    {entry.jobId ? (
                      <Button variant="ghost" size="sm" nativeButton={false} render={<Link to={`${detail}/logs?job=${encodeURIComponent(entry.jobId)}`} />}>
                        Logs
                      </Button>
                    ) : null}
                    <Button variant="ghost" size="sm" nativeButton={false} render={<Link to={`/activity?event=${encodeURIComponent(entry.id)}`} />}>
                      Details
                      <Icon name="action.next" size={14} />
                    </Button>
                  </div>
                </li>
              )
            })}
          </ol>
          {history.isError ? (
            <p className="text-xs text-danger">Could not load more entries. Try again.</p>
          ) : null}
          {history.hasNextPage ? (
            <Button variant="outline" className="self-center" onClick={() => void history.fetchNextPage()} disabled={history.isFetchingNextPage}>
              {history.isFetchingNextPage ? "Loading…" : "Load more"}
            </Button>
          ) : null}
        </div>
      )}
    </SectionCard>
  )
}
