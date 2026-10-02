import { useInfiniteQuery } from "@tanstack/react-query"
import { useId, useMemo, useState } from "react"
import { toast } from "sonner"

import { ErrorState } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { keys, runNative, useNativePort, type NativeCommandError } from "@/lib/api"
import { plural } from "@/lib/format"
import type { LogResult } from "@/lib/native"
import { cn } from "@/lib/utils"

import { visibleLogText } from "./jobs.ts"

const PAGE_SIZE = 200

/**
 * Bounded, redacted job log (gbm_logs_read). Lines render as escaped plain text; control
 * characters are shown as visible symbols, never interpreted. Paging is explicit.
 */
export function JobLogViewer({ jobId, className }: { jobId: string; className?: string }) {
  const port = useNativePort()
  const [wrap, setWrap] = useState(true)
  const labelId = useId()
  const logs = useInfiniteQuery<LogResult, NativeCommandError, { pages: LogResult[] }, readonly unknown[], string | null>({
    queryKey: [...keys.logs(jobId), "pages"],
    queryFn: ({ pageParam }) => runNative(port, "gbm_logs_read", { jobId, cursor: pageParam, limit: PAGE_SIZE }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextCursor ?? undefined,
  })

  const lines = useMemo(() => (logs.data?.pages ?? []).flatMap((page) => page.lines.map(visibleLogText)), [logs.data])
  const truncated = logs.data?.pages.some((page) => page.truncated) ?? false

  async function copy() {
    try {
      await navigator.clipboard.writeText(lines.join("\n"))
      toast.success(`Copied ${plural(lines.length, "log line")}`)
    } catch {
      toast.error("Could not copy the log", { description: "Clipboard access was denied." })
    }
  }

  return (
    <section aria-labelledby={labelId} className={cn("flex min-w-0 flex-col gap-2", className)}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 id={labelId} className="text-[13px] font-semibold">
          Job log
          {logs.data ? <span className="ml-1.5 font-normal text-muted-foreground">{plural(lines.length, "line")} loaded</span> : null}
        </h3>
        <div className="flex items-center gap-1.5">
          <Button size="sm" variant="outline" aria-pressed={wrap} onClick={() => setWrap((value) => !value)}>
            {wrap ? "Wrap on" : "Wrap off"}
          </Button>
          <Button size="sm" variant="outline" onClick={() => void copy()} disabled={lines.length === 0}>
            Copy
          </Button>
        </div>
      </div>
      {logs.isPending ? (
        <div className="flex flex-col gap-1.5 rounded-lg border bg-muted/40 p-3" aria-busy="true">
          <Skeleton className="h-3 w-4/5" />
          <Skeleton className="h-3 w-3/5" />
          <Skeleton className="h-3 w-2/3" />
          <span className="sr-only" role="status">Loading log…</span>
        </div>
      ) : logs.isError && lines.length === 0 ? (
        <ErrorState
          error={logs.error.appError}
          title="Could not read the job log"
          onRetry={() => void logs.refetch()}
          retrying={logs.isFetching}
        />
      ) : lines.length === 0 ? (
        <p className="rounded-lg border bg-muted/40 px-3 py-2.5 text-[13px] text-muted-foreground">This job has not written any log lines.</p>
      ) : (
        <pre
          tabIndex={0}
          aria-label="Job log lines"
          className={cn(
            "max-h-72 min-w-0 overflow-auto rounded-lg border bg-muted/40 p-3 font-mono text-xs leading-relaxed ui-selectable focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none",
            wrap ? "break-words whitespace-pre-wrap" : "whitespace-pre",
          )}
        >
          {lines.join("\n")}
        </pre>
      )}
      {truncated ? <p className="text-xs text-muted-foreground">Some long lines were shortened by the log reader.</p> : null}
      {logs.isError && lines.length > 0 ? (
        <p className="text-xs text-danger" role="alert">
          Loading more lines failed ({logs.error.appError.code}).
        </p>
      ) : null}
      {logs.hasNextPage ? (
        <Button size="sm" variant="outline" className="self-start" onClick={() => void logs.fetchNextPage()} disabled={logs.isFetchingNextPage}>
          {logs.isFetchingNextPage ? "Loading…" : "Load more lines"}
        </Button>
      ) : null}
    </section>
  )
}
