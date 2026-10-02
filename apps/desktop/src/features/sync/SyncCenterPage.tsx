import { skipToken, useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query"
import { useCallback, useId, useMemo, useRef, useSyncExternalStore } from "react"
import { useSearchParams } from "react-router"
import { toast } from "sonner"

import { EmptyState, ErrorState, PageHeader, RowSkeletons, SectionCard, StaleNotice, StatusBadge } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { ERROR_COPY, keys, runNative, useNativeMutation, useNativePort, useScanAction, type NativeCommandError } from "@/lib/api"
import { plural } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { Job, JobList } from "@/lib/native"

import { JobInspector } from "./JobInspector.tsx"
import { JobRow } from "./JobRow.tsx"
import { countJobs, groupJobs } from "./jobs.ts"

const PAGE_SIZE = 100
const WIDE_QUERY = "(min-width: 1280px)"

function subscribeWide(onChange: () => void) {
  const mql = window.matchMedia(WIDE_QUERY)
  mql.addEventListener("change", onChange)
  return () => mql.removeEventListener("change", onChange)
}

/** Side-by-side inspector at >= 1280 px; a drawer below that (UI-06 compact layout). */
function useWideLayout() {
  return useSyncExternalStore(subscribeWide, () => window.matchMedia(WIDE_QUERY).matches, () => true)
}

function useJobs() {
  const port = useNativePort()
  return useInfiniteQuery<JobList, NativeCommandError, { pages: JobList[] }, readonly unknown[], string | null>({
    queryKey: [...keys.jobs, "pages"],
    queryFn: ({ pageParam }) => runNative(port, "gbm_jobs_list", { cursor: pageParam, limit: PAGE_SIZE }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextCursor ?? undefined,
  })
}

/**
 * The last pause value the native side confirmed in this session. No DTO reports the
 * paused state, so an unknown value (fresh start) is treated as "not known to be paused".
 */
function useQueuePause() {
  const client = useQueryClient()
  const paused = useQuery<boolean>({ queryKey: keys.queuePaused, queryFn: skipToken, staleTime: Infinity, gcTime: Infinity })
  const mutation = useNativeMutation("gbm_queue_set_paused", [keys.jobs])
  const toggle = () => {
    const next = paused.data !== true
    mutation.mutate(
      { paused: next },
      {
        onSuccess: () => {
          client.setQueryData(keys.queuePaused, next)
          if (next) toast.success("New jobs paused", { description: "Running jobs continue until they finish or you cancel them." })
          else toast.success("Queue resumed", { description: "Queued jobs can start again." })
        },
        onError: (error) =>
          toast.error(next ? "Queue was not paused" : "Queue was not resumed", {
            description: `${ERROR_COPY[error.appError.code]} (${error.appError.code})`,
          }),
      },
    )
  }
  return { paused: paused.data, toggle, pending: mutation.isPending }
}

function SummaryPills({ jobs, paused, partial }: { jobs: Job[]; paused: boolean | undefined; partial: boolean }) {
  const counts = countJobs(jobs)
  if (jobs.length === 0) {
    return paused === true ? <StatusBadge status={{ label: "Paused — running jobs continue", icon: "action.pause", tone: "warning" }} /> : null
  }
  return (
    <>
      <StatusBadge status={{ label: `${counts.running} running`, icon: "status.running", tone: counts.running > 0 ? "info" : "neutral", spinning: counts.running > 0 }} />
      <StatusBadge status={{ label: `${counts.waiting} waiting`, icon: "status.waiting", tone: "neutral" }} />
      <StatusBadge status={{ label: `${counts.review} need review`, icon: "status.blocked", tone: counts.review > 0 ? "warning" : "neutral" }} />
      <StatusBadge status={{ label: `${counts.failed} failed`, icon: "status.failed", tone: counts.failed > 0 ? "danger" : "neutral" }} />
      {paused === true ? <StatusBadge status={{ label: "Paused — running jobs continue", icon: "action.pause", tone: "warning" }} /> : null}
      {partial ? <span className="text-xs text-muted-foreground">Counts cover the {plural(jobs.length, "loaded job")}</span> : null}
    </>
  )
}

export function SyncCenterPage() {
  const [params, setParams] = useSearchParams()
  const selectedId = params.get("job")
  const wide = useWideLayout()
  const jobs = useJobs()
  const pause = useQueuePause()
  const scan = useScanAction()
  const scanReasonId = useId()
  const rowRefs = useRef(new Map<string, HTMLButtonElement>())
  const finalFocusRef = useRef<HTMLButtonElement | null>(null)

  const items = useMemo(() => (jobs.data?.pages ?? []).flatMap((page) => page.items), [jobs.data])
  const groups = useMemo(() => groupJobs(items), [items])
  const selected = selectedId === null ? null : (items.find((job) => job.id === selectedId) ?? null)

  const select = useCallback(
    (jobId: string | null) => {
      finalFocusRef.current = jobId === null ? null : (rowRefs.current.get(jobId) ?? null)
      setParams(
        (current) => {
          const next = new URLSearchParams(current)
          if (jobId === null) next.delete("job")
          else next.set("job", jobId)
          return next
        },
        { replace: jobId === null },
      )
    },
    [setParams],
  )
  const registerRef = useCallback((jobId: string, element: HTMLButtonElement | null) => {
    if (element) rowRefs.current.set(jobId, element)
    else rowRefs.current.delete(jobId)
  }, [])

  const showInspector = wide && (jobs.isPending || items.length > 0 || selectedId !== null)
  const observedAt = jobs.dataUpdatedAt > 0 ? new Date(jobs.dataUpdatedAt).toISOString() : null

  const renderRows = (list: Job[]) => (
    <ul className="flex flex-col gap-1">
      {list.map((job) => (
        <JobRow
          key={job.id}
          job={job}
          selected={job.id === selectedId}
          queuePaused={pause.paused}
          onSelect={select}
          onRetried={select}
          registerRef={registerRef}
        />
      ))}
    </ul>
  )

  let queueBody
  if (jobs.isPending) {
    queueBody = (
      <div aria-busy="true">
        <RowSkeletons rows={5} />
        <span className="sr-only" role="status">Loading jobs…</span>
      </div>
    )
  } else if (jobs.isError && items.length === 0) {
    queueBody = <ErrorState error={jobs.error.appError} title="Could not load the job queue" onRetry={() => void jobs.refetch()} retrying={jobs.isFetching} />
  } else if (items.length === 0) {
    queueBody = (
      <EmptyState
        icon="state.empty"
        title="No jobs queued"
        description="Scans and syncs you start appear here. Jobs keep running in the background when you leave this page."
      />
    )
  } else {
    queueBody = (
      <div className="@container flex min-w-0 flex-col gap-4">
        <section aria-label="Active jobs" className="flex flex-col gap-1">
          {groups.active.length > 0 ? (
            renderRows(groups.active)
          ) : (
            <p className="rounded-lg bg-muted/50 px-3 py-2.5 text-[13px] text-muted-foreground">No jobs running or waiting.</p>
          )}
        </section>
        {groups.recent.length > 0 ? (
          <section aria-labelledby="sync-recent-heading" className="flex flex-col gap-1.5 border-t pt-4">
            <h3 id="sync-recent-heading" className="px-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
              Recent
            </h3>
            {renderRows(groups.recent)}
          </section>
        ) : null}
        {jobs.hasNextPage ? (
          <Button variant="outline" className="self-center" onClick={() => void jobs.fetchNextPage()} disabled={jobs.isFetchingNextPage}>
            {jobs.isFetchingNextPage ? "Loading…" : "Load older jobs"}
          </Button>
        ) : null}
      </div>
    )
  }

  const missing = (
    <EmptyState
      icon="status.unknown"
      title="Job not found"
      description="This job is not in the loaded queue. It may be older than the loaded list or no longer exist."
      action={
        <Button variant="outline" onClick={() => select(null)}>
          Clear selection
        </Button>
      }
    />
  )

  const inspector =
    selectedId === null ? (
      <EmptyState icon="nav.sync" title="Select a job" description="Choose a job in the queue to see its stages, coverage and logs." />
    ) : jobs.isPending ? (
      <RowSkeletons rows={4} />
    ) : selected === null ? (
      missing
    ) : (
      <JobInspector key={selected.id} job={selected} onRetried={select} />
    )

  return (
    <>
      <PageHeader
        title="Sync Center"
        description="Background jobs with their real stages and controls."
        actions={
          <>
            <Button variant="outline" onClick={pause.toggle} disabled={pause.pending}>
              <Icon name={pause.paused === true ? "action.sync" : "action.pause"} />
              {pause.pending ? "Saving…" : pause.paused === true ? "Resume new jobs" : "Pause new jobs"}
            </Button>
            <Button
              onClick={scan.run}
              disabled={!scan.available || scan.pending}
              title={scan.unavailableReason ?? undefined}
              aria-describedby={scan.unavailableReason ? scanReasonId : undefined}
            >
              <Icon name="action.scan" />
              {scan.pending ? "Queuing scan…" : "Scan now"}
            </Button>
            {scan.unavailableReason ? (
              <span id={scanReasonId} className="sr-only">
                {scan.unavailableReason}
              </span>
            ) : null}
          </>
        }
        meta={items.length > 0 || pause.paused === true ? <SummaryPills jobs={items} paused={pause.paused} partial={jobs.hasNextPage} /> : null}
      />

      {jobs.isError && items.length > 0 ? (
        <StaleNotice observedAt={observedAt} error={jobs.error.appError} onRetry={() => void jobs.refetch()} retrying={jobs.isFetching} />
      ) : null}

      <div className={showInspector ? "grid min-w-0 grid-cols-[minmax(0,1fr)_minmax(340px,400px)] items-start gap-[18px]" : "min-w-0"}>
        <SectionCard icon="nav.sync" tone="info" title="Queue" description="Active jobs first, then recent results">
          {queueBody}
        </SectionCard>
        {showInspector ? (
          <Card className="sticky top-0" aria-label="Selected job">
            <CardContent>{inspector}</CardContent>
          </Card>
        ) : null}
      </div>

      {!wide ? (
        <Sheet open={selectedId !== null} onOpenChange={(open) => (open ? undefined : select(null))}>
          <SheetContent className="w-[min(440px,100vw)] gap-0 overflow-y-auto data-[side=right]:sm:max-w-[440px]" finalFocus={finalFocusRef}>
            <SheetHeader className="border-b pr-12">
              <SheetTitle className="text-[15px] font-semibold">Job details</SheetTitle>
              <SheetDescription className="text-[13px]">Stages, coverage and logs for the selected job.</SheetDescription>
            </SheetHeader>
            <div className="p-4">{selectedId === null ? null : jobs.isPending ? <RowSkeletons rows={4} /> : selected === null ? missing : <JobInspector key={selected.id} job={selected} onRetried={select} />}</div>
          </SheetContent>
        </Sheet>
      ) : null}
    </>
  )
}
