import { keepPreviousData, useInfiniteQuery, useMutation } from "@tanstack/react-query"
import { useCallback, useDeferredValue, useMemo, useRef, useState } from "react"
import { useSearchParams } from "react-router"
import { toast } from "sonner"

import { ConfirmDialog, EmptyState, ErrorState, PageHeader, RowSkeletons, SectionCard, StaleNotice } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ERROR_COPY, keys, NativeCommandError, runNative, useNativePort } from "@/lib/api"
import { plural } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { ActivityList, ActivityRequest, ExportResult } from "@/lib/native"

import { ActivityDetailSheet } from "./ActivityDetailSheet.tsx"
import { ActivityRow } from "./ActivityRow.tsx"
import { groupByDay, parseSeverity, parseType, SEVERITY_OPTIONS, TYPE_OPTIONS, type SeverityFilter, type TypeFilter } from "./timeline.ts"

const PAGE_SIZE = 50
const SEARCH_MAX = 256

const SEVERITY_LABEL: Record<SeverityFilter, string> = { all: "All", info: "Info", warning: "Warning", error: "Error" }
const TYPE_LABEL: Record<TypeFilter, string> = { all: "All", scan: "Scan", sync: "Sync", auth: "Account", project: "Project", system: "System" }

type Filters = Pick<ActivityRequest, "severity" | "search">

function useActivity(filters: Filters) {
  const port = useNativePort()
  return useInfiniteQuery<ActivityList, NativeCommandError, { pages: ActivityList[] }, readonly unknown[], string | null>({
    queryKey: [...keys.activity, "timeline", filters],
    queryFn: ({ pageParam }) =>
      runNative(port, "gbm_activity_list", { cursor: pageParam, limit: PAGE_SIZE, repositoryId: null, severity: filters.severity, search: filters.search }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextCursor ?? undefined,
    placeholderData: keepPreviousData,
  })
}

function ExportButton({ filters, typeFilterActive }: { filters: Filters; typeFilterActive: boolean }) {
  const port = useNativePort()
  const [open, setOpen] = useState(false)
  const exporter = useMutation<ExportResult, NativeCommandError | Error>({
    mutationFn: () => runNative(port, "gbm_activity_export", { cursor: null, limit: 200, repositoryId: null, severity: filters.severity, search: filters.search }),
    onSuccess: (result) => {
      setOpen(false)
      if (result.outcome === "saved") toast.success("Activity exported", { description: result.displayPath ?? "The file was saved." })
      else toast("Export canceled", { description: "No file was written." })
    },
    onError: (error) =>
      toast.error("Export failed", {
        description: error instanceof NativeCommandError ? `${ERROR_COPY[error.appError.code]} (${error.appError.code})` : error.message,
      }),
  })
  const scope = [
    filters.severity === "all" ? "all severities" : `${SEVERITY_LABEL[filters.severity].toLowerCase()} events`,
    filters.search.trim() ? `matching “${filters.search.trim()}”` : null,
  ]
    .filter(Boolean)
    .join(", ")

  return (
    <>
      <Button variant="outline" onClick={() => setOpen(true)}>
        <Icon name="action.export" />
        Export
      </Button>
      <ConfirmDialog
        open={open}
        onOpenChange={setOpen}
        icon="action.export"
        title="Export activity"
        description={
          <>
            <p>Exports events for {scope}. You choose where to save the file.</p>
            <p className="mt-2">
              The file can include private repository names and local folder paths. Review it before sharing it with anyone.
            </p>
            {typeFilterActive ? <p className="mt-2">The Type filter only narrows loaded events on screen and is not applied to the export.</p> : null}
          </>
        }
        confirmLabel="Choose file…"
        pendingLabel="Waiting for save dialog…"
        pending={exporter.isPending}
        onConfirm={() => exporter.mutate()}
      />
    </>
  )
}

export function ActivityPage() {
  const [params, setParams] = useSearchParams()
  const search = params.get("q") ?? ""
  const severity = parseSeverity(params.get("severity"))
  const type = parseType(params.get("type"))
  const eventId = params.get("event")
  const deferredSearch = useDeferredValue(search)
  const filters = useMemo<Filters>(() => ({ severity, search: deferredSearch }), [severity, deferredSearch])
  const activity = useActivity(filters)
  const rowRefs = useRef(new Map<string, HTMLButtonElement>())
  const finalFocusRef = useRef<HTMLElement | null>(null)

  const update = useCallback(
    (patch: Record<string, string | null>, replace = true) =>
      setParams(
        (current) => {
          const next = new URLSearchParams(current)
          for (const [key, value] of Object.entries(patch)) {
            if (value === null || value === "" || value === "all") next.delete(key)
            else next.set(key, value)
          }
          return next
        },
        { replace },
      ),
    [setParams],
  )

  const loaded = useMemo(() => (activity.data?.pages ?? []).flatMap((page) => page.items), [activity.data])
  const visible = useMemo(() => (type === "all" ? loaded : loaded.filter((entry) => entry.type === type)), [loaded, type])
  const groups = useMemo(() => groupByDay(visible), [visible])
  const selected = eventId === null ? null : (loaded.find((entry) => entry.id === eventId) ?? null)
  const serverFiltered = severity !== "all" || deferredSearch.trim() !== ""
  const anyFilter = serverFiltered || type !== "all"
  const updating = search !== deferredSearch || (activity.isPlaceholderData && activity.isFetching)

  const registerRef = useCallback((id: string, element: HTMLButtonElement | null) => {
    if (element) rowRefs.current.set(id, element)
    else rowRefs.current.delete(id)
  }, [])
  const openEvent = (id: string) => {
    finalFocusRef.current = rowRefs.current.get(id) ?? null
    update({ event: id }, false)
  }
  const clearFilters = () => update({ q: null, severity: null, type: null })

  let body
  if (activity.isPending) {
    body = (
      <div aria-busy="true">
        <RowSkeletons rows={6} />
        <span className="sr-only" role="status">Loading activity…</span>
      </div>
    )
  } else if (activity.isError && loaded.length === 0) {
    body = <ErrorState error={activity.error.appError} title="Could not load activity" onRetry={() => void activity.refetch()} retrying={activity.isFetching} />
  } else if (visible.length === 0) {
    body = anyFilter ? (
      <EmptyState
        icon="action.scan"
        title="No events match these filters"
        description={
          type !== "all" && activity.hasNextPage
            ? `None of the ${plural(loaded.length, "loaded event")} are ${TYPE_LABEL[type]} events. Load more or clear filters.`
            : "Try a different search or severity."
        }
        action={
          <div className="flex flex-wrap justify-center gap-2">
            <Button variant="outline" onClick={clearFilters}>
              Clear filters
            </Button>
            {type !== "all" && activity.hasNextPage ? (
              <Button variant="outline" onClick={() => void activity.fetchNextPage()} disabled={activity.isFetchingNextPage}>
                Load more
              </Button>
            ) : null}
          </div>
        }
      />
    ) : (
      <EmptyState icon="nav.activity" title="No activity yet" description="Scans, syncs and account changes are recorded here as they happen." />
    )
  } else {
    body = (
      <div className="@container flex min-w-0 flex-col gap-5" aria-busy={updating || undefined}>
        {groups.map((group) => (
          <section key={group.key} aria-labelledby={`day-${group.key}`} className="flex flex-col gap-1">
            <h3 id={`day-${group.key}`} className="flex items-baseline gap-2 px-2 text-[13px] font-semibold">
              {group.heading}
              <span className="text-xs font-normal text-muted-foreground">{plural(group.entries.length, "event")}</span>
            </h3>
            <ul className="flex flex-col gap-0.5">
              {group.entries.map((entry) => (
                <ActivityRow key={entry.id} entry={entry} selected={entry.id === eventId} onOpen={openEvent} registerRef={registerRef} />
              ))}
            </ul>
          </section>
        ))}
        {activity.hasNextPage ? (
          <Button variant="outline" className="self-center" onClick={() => void activity.fetchNextPage()} disabled={activity.isFetchingNextPage}>
            {activity.isFetchingNextPage ? "Loading…" : "Load more"}
          </Button>
        ) : null}
      </div>
    )
  }

  return (
    <>
      <PageHeader
        title="Activity"
        description="Readable audit trail for scans, syncs and errors."
        actions={<ExportButton filters={filters} typeFilterActive={type !== "all"} />}
      />

      <div role="search" className="flex flex-wrap items-center gap-2.5">
        <div className="relative w-full max-w-sm min-w-56 flex-1">
          <Icon name="action.scan" className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted-foreground" />
          <label htmlFor="activity-search" className="sr-only">
            Search events
          </label>
          <Input
            id="activity-search"
            type="search"
            value={search}
            maxLength={SEARCH_MAX}
            placeholder="Search events…"
            className="pl-9 text-[13px] md:text-[13px]"
            onChange={(event) => update({ q: event.target.value })}
          />
        </div>
        <Select
          value={severity}
          items={SEVERITY_OPTIONS.map((value) => ({ value, label: SEVERITY_LABEL[value] }))}
          onValueChange={(value) => update({ severity: value })}
        >
          <SelectTrigger aria-label="Severity">
            <span className="text-muted-foreground">Severity:</span>
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false} align="start">
            {SEVERITY_OPTIONS.map((value) => (
              <SelectItem key={value} value={value}>
                {SEVERITY_LABEL[value]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select
          value={type}
          items={TYPE_OPTIONS.map((value) => ({ value, label: TYPE_LABEL[value] }))}
          onValueChange={(value) => update({ type: value })}
        >
          <SelectTrigger aria-label="Type, filters loaded events">
            <span className="text-muted-foreground">Type:</span>
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false} align="start">
            {TYPE_OPTIONS.map((value) => (
              <SelectItem key={value} value={value}>
                {TYPE_LABEL[value]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {anyFilter ? (
          <Button variant="ghost" onClick={clearFilters}>
            Clear filters
          </Button>
        ) : null}
        {type !== "all" && loaded.length > 0 ? (
          <p className="w-full text-xs text-muted-foreground">
            Type narrows the {plural(loaded.length, "loaded event")}; it is applied on this screen only.
          </p>
        ) : null}
      </div>

      {activity.isError && loaded.length > 0 ? (
        <StaleNotice
          observedAt={activity.dataUpdatedAt > 0 ? new Date(activity.dataUpdatedAt).toISOString() : null}
          error={activity.error.appError}
          onRetry={() => void activity.refetch()}
          retrying={activity.isFetching}
        />
      ) : null}

      <SectionCard icon="nav.activity" tone="info" title="Timeline" description="Most recent first">
        {body}
      </SectionCard>

      <ActivityDetailSheet
        eventId={eventId}
        entry={selected}
        loading={activity.isPending}
        onClose={() => update({ event: null })}
        finalFocus={finalFocusRef}
      />
    </>
  )
}
