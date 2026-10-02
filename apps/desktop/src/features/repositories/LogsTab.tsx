import { useState } from "react"
import { useInfiniteQuery } from "@tanstack/react-query"
import { useSearchParams } from "react-router"

import { EmptyState, ErrorState, RowSkeletons, SectionCard } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { keys, NativeCommandError, runNative, useNativePort, useNativeQuery } from "@/lib/api"
import { dateTime, visibleLogText } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { ActivityEntry, LogResult } from "@/lib/native"

import { CopyButton } from "./CopyButton.tsx"

const LINES_PER_PAGE = 200

/**
 * Redacted job logs, read on demand and paged. Lines are React text nodes inside <pre>:
 * HTML and ANSI sequences are shown literally, never interpreted.
 */
export function LogsTab({ repositoryId }: { repositoryId: string }) {
  const [params, setParams] = useSearchParams()
  const request = { cursor: null, limit: 100, repositoryId, severity: "all" as const, search: "" }
  const activity = useNativeQuery([...keys.activity, "repository", request], "gbm_activity_list", request)

  const jobs: { jobId: string; entry: ActivityEntry }[] = []
  for (const entry of activity.data?.items ?? []) {
    if (entry.jobId && !jobs.some((job) => job.jobId === entry.jobId)) jobs.push({ jobId: entry.jobId, entry })
  }
  const requested = params.get("job")
  const jobId = jobs.find((job) => job.jobId === requested)?.jobId ?? requested ?? jobs[0]?.jobId ?? null
  const items = jobs.map((job) => ({
    value: job.jobId,
    label: `${job.jobId} · ${dateTime(job.entry.occurredAt) ?? "unknown time"}`,
  }))
  if (jobId !== null && !items.some((item) => item.value === jobId)) items.unshift({ value: jobId, label: jobId })

  const selectJob = (value: string) =>
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev)
        next.set("job", value)
        return next
      },
      { replace: true },
    )

  return (
    <SectionCard
      icon="nav.activity"
      title="Logs"
      description="Redacted output of one job, loaded on demand"
      action={
        items.length > 0 ? (
          <Select items={items} value={jobId} onValueChange={(value) => value !== null && selectJob(value)}>
            <SelectTrigger aria-label="Job" className="w-72 max-w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {items.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null
      }
    >
      {activity.isPending ? (
        <RowSkeletons rows={3} />
      ) : activity.isError && jobId === null ? (
        <ErrorState error={activity.error.appError} title="Could not list jobs" onRetry={() => void activity.refetch()} retrying={activity.isFetching} />
      ) : jobId === null ? (
        <EmptyState className="py-6" icon="nav.activity" title="No job logs yet" description="Logs become available after a scan or sync job runs for this repository." />
      ) : (
        <JobLog key={jobId} jobId={jobId} />
      )}
    </SectionCard>
  )
}

function JobLog({ jobId }: { jobId: string }) {
  const port = useNativePort()
  const [wrap, setWrap] = useState(true)
  const log = useInfiniteQuery<LogResult, NativeCommandError>({
    queryKey: keys.logs(jobId),
    queryFn: ({ pageParam }) => runNative(port, "gbm_logs_read", { jobId, cursor: (pageParam as string | null) ?? null, limit: LINES_PER_PAGE }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextCursor,
  })
  const lines = log.data?.pages.flatMap((page) => page.lines.map(visibleLogText)) ?? []
  const truncated = log.data?.pages.some((page) => page.truncated) ?? false
  const text = lines.join("\n")

  if (log.isPending) return <RowSkeletons rows={4} />
  if (log.isError && lines.length === 0) {
    return <ErrorState error={log.error.appError} title="Could not read this log" onRetry={() => void log.refetch()} retrying={log.isFetching} />
  }

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {lines.length.toLocaleString("en")} {lines.length === 1 ? "line" : "lines"} loaded
          {log.hasNextPage ? " · more available" : ""}
        </p>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <Switch id={`wrap-${jobId}`} checked={wrap} onCheckedChange={setWrap} />
            <Label htmlFor={`wrap-${jobId}`} className="text-[13px] font-normal">
              Wrap lines
            </Label>
          </div>
          <CopyButton value={text} label="Copy loaded log lines">
            Copy
          </CopyButton>
        </div>
      </div>
      {truncated ? (
        <p role="status" className="flex items-center gap-2 text-xs text-warning">
          <Icon name="state.warning" size={14} />
          The native side truncated this log; the oldest or longest lines are not shown.
        </p>
      ) : null}
      {lines.length === 0 ? (
        <EmptyState className="py-6" icon="nav.activity" title="This job wrote no log lines" />
      ) : (
        <pre
          aria-label={`Log for job ${jobId}`}
          tabIndex={0}
          className={
            "max-h-[28rem] overflow-auto rounded-lg border bg-muted/40 p-3 font-mono text-xs leading-5 ui-selectable focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none " +
            (wrap ? "break-all whitespace-pre-wrap" : "whitespace-pre")
          }
        >
          {text}
        </pre>
      )}
      {log.isError ? <p className="text-xs text-danger">Could not load more lines. Try again.</p> : null}
      {log.hasNextPage ? (
        <Button variant="outline" className="self-center" onClick={() => void log.fetchNextPage()} disabled={log.isFetchingNextPage}>
          {log.isFetchingNextPage ? "Loading…" : "Load more"}
        </Button>
      ) : null}
    </div>
  )
}
