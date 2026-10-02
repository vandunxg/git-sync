import { useState, type MouseEvent } from "react"
import { keepPreviousData } from "@tanstack/react-query"
import { Link, useNavigate } from "react-router"
import { toast } from "sonner"

import {
  EmptyState,
  ErrorState,
  LoadingLabel,
  MIRROR_STATE,
  PageHeader,
  REPOSITORY_STATUS,
  REPOSITORY_STATUS_ORDER,
  StaleNotice,
  StatusBadge,
  destinationLabel,
} from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { keys, useNativeQuery, useScanAction } from "@/lib/api"
import { dateTime, relativeTime, splitFullName } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { RepoListRequest, RepositorySummary } from "@/lib/native"
import { cn } from "@/lib/utils"

import { useDeferredSearch, useUrlState } from "./hooks.ts"
import {
  hasActiveFilters,
  MAX_SELECTION,
  PAGE_SIZE,
  parseFilters,
  SORT_OPTIONS,
  VISIBILITY_OPTIONS,
  type ListFilters,
  type RepoStatus,
} from "./model.ts"
import { RepositoryActionsMenu } from "./RepositoryActionsMenu.tsx"
import { SyncSelectedDialog } from "./SyncSelectedDialog.tsx"

const STATUS_ITEMS = [{ value: "all", label: "All statuses" }, ...REPOSITORY_STATUS_ORDER.map((value) => ({ value, label: REPOSITORY_STATUS[value].label }))]

export function RepositoriesPage() {
  const [params, update, current] = useUrlState()
  const filters = parseFilters(params)
  const cursors = params.getAll("c")
  const selection = params.getAll("sel")
  const scan = useScanAction()
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [seen, setSeen] = useState<Map<string, RepositorySummary>>(() => new Map())

  /** Filter changes restart paging; selection (by ID) is kept. */
  const setFilter = (key: string, value: string | null) =>
    update((next) => {
      if (value === null) next.delete(key)
      else next.set(key, value)
      next.delete("c")
    })

  const search = useDeferredSearch(filters.search, (value) => setFilter("q", value === "" ? null : value))
  const request: RepoListRequest = {
    cursor: cursors.at(-1) ?? null,
    limit: PAGE_SIZE,
    search: search.deferred.trim(),
    status: filters.status,
    visibility: filters.visibility,
    selectedOnly: filters.selectedOnly,
    sort: filters.sort,
  }
  const list = useNativeQuery([...keys.repositories, "list", request], "gbm_repositories_list", request, {
    placeholderData: keepPreviousData,
  })

  const items = list.data?.items ?? []
  const selectedSet = new Set(selection)
  const pageSelected = items.filter((repo) => selectedSet.has(repo.id)).length

  const setSelected = (repos: RepositorySummary[], selected: boolean) => {
    const ids = new Set(current().getAll("sel"))
    for (const repo of repos) {
      if (selected) ids.add(repo.id)
      else ids.delete(repo.id)
    }
    if (ids.size > MAX_SELECTION) {
      toast.error(`Select at most ${MAX_SELECTION} repositories per batch`)
      return
    }
    if (selected) setSeen((prev) => new Map([...prev, ...repos.map((repo) => [repo.id, repo] as const)]))
    update((next) => {
      next.delete("sel")
      for (const id of ids) next.append("sel", id)
    })
  }
  /** Page changes start at the top of the main scroll region. */
  const pageTo = (mutate: (next: URLSearchParams) => void) => {
    update(mutate)
    document.querySelector("[data-ui-scroll-container]")?.scrollTo({ top: 0 })
  }
  const clearSelection = () => update((next) => next.delete("sel"))
  const clearFilters = () =>
    update((next) => {
      for (const key of ["q", "status", "visibility", "included", "c"]) next.delete(key)
    })

  const known = [...items, ...[...seen.values()].filter((repo) => !items.some((item) => item.id === repo.id))]

  return (
    <>
      <PageHeader
        title="Repositories"
        description="Search, filter and act on repositories."
        actions={
          <>
            <Button variant="outline" onClick={scan.run} disabled={!scan.available || scan.pending} title={scan.unavailableReason ?? undefined}>
              <Icon name="action.scan" />
              {scan.pending ? "Queuing scan…" : "Scan"}
            </Button>
            <Button
              onClick={() => setConfirmOpen(true)}
              disabled={selection.length === 0}
              aria-describedby={selection.length === 0 ? "sync-selected-reason" : undefined}
              title={selection.length === 0 ? "Select repositories with the checkboxes first." : undefined}
            >
              <Icon name="action.sync" />
              Sync selected ({selection.length})
            </Button>
            <span id="sync-selected-reason" className="sr-only">
              Select repositories with the checkboxes first.
            </span>
          </>
        }
      />

      <Toolbar filters={filters} searchDraft={search.draft} onSearch={search.setDraft} onFilter={setFilter} searching={search.pending} />

      {list.isError && list.data ? (
        <StaleNotice observedAt={null} error={list.error.appError} onRetry={() => void list.refetch()} retrying={list.isFetching} />
      ) : null}
      {list.data && !list.data.discoveryComplete ? (
        <div role="status" className="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning-soft px-4 py-2.5 text-[13px] text-warning">
          <Icon name="state.warning" className="mt-0.5" />
          <p>
            <span className="font-semibold">Discovery incomplete.</span> Some repositories may be missing from this list until the
            next scan finishes.
          </p>
        </div>
      ) : null}

      {list.isError && !list.data ? (
        <ErrorState
          error={list.error.appError}
          title="Could not load repositories"
          onRetry={() => void list.refetch()}
          retrying={list.isFetching}
        />
      ) : (
        <Card className="gap-0 overflow-hidden py-0">
          {items.length > 0 || selection.length > 0 ? (
            <SelectionBar total={selection.length} onPage={pageSelected} onClear={clearSelection} loaded={list.data !== undefined} />
          ) : null}
          {list.isPending ? (
            <TableSkeleton />
          ) : items.length === 0 ? (
            hasActiveFilters(filters) || cursors.length > 0 ? (
              <EmptyState
                icon="action.scan"
                title="No repositories match these filters"
                description="Try a different search or clear the filters."
                action={
                  <Button variant="outline" onClick={clearFilters}>
                    <Icon name="action.cancel" />
                    Clear filters
                  </Button>
                }
              />
            ) : (
              <EmptyState
                title="No repositories yet"
                description="Connect GitHub or run a scan to discover repositories."
                action={
                  scan.available ? (
                    <Button onClick={scan.run} disabled={scan.pending}>
                      <Icon name="action.scan" />
                      Scan GitHub
                    </Button>
                  ) : (
                    <Button nativeButton={false} render={<Link to="/connections" />}>
                      <Icon name="action.reconnect" />
                      Connect with token
                    </Button>
                  )
                }
              />
            )
          ) : (
            <RepositoryTable
              items={items}
              selected={selectedSet}
              revision={list.data?.revision ?? 0}
              onSelect={setSelected}
              busy={list.isPlaceholderData}
            />
          )}
        </Card>
      )}

      {list.data && items.length > 0 ? (
        <Pager
          start={cursors.length * PAGE_SIZE + 1}
          count={items.length}
          total={list.data.totalCount}
          hasPrevious={cursors.length > 0}
          nextCursor={list.data.nextCursor}
          onPrevious={() =>
            pageTo((next) => {
              const stack = next.getAll("c").slice(0, -1)
              next.delete("c")
              for (const cursor of stack) next.append("c", cursor)
            })
          }
          onNext={(cursor) => pageTo((next) => next.append("c", cursor))}
        />
      ) : null}

      <SyncSelectedDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        selectedIds={selection}
        known={known}
        onQueued={clearSelection}
      />
    </>
  )
}

function Toolbar({
  filters,
  searchDraft,
  onSearch,
  onFilter,
  searching,
}: {
  filters: ListFilters
  searchDraft: string
  onSearch: (value: string) => void
  onFilter: (key: string, value: string | null) => void
  searching: boolean
}) {
  return (
    <div role="search" aria-label="Filter repositories" className="flex flex-wrap items-center gap-2">
      <div className="relative min-w-56 flex-1 basis-64">
        <Icon name="action.scan" className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted-foreground" />
        <Input
          type="search"
          value={searchDraft}
          onChange={(event) => onSearch(event.target.value)}
          placeholder="Search repositories"
          aria-label="Search repositories"
          className="pl-9 text-[13px] md:text-[13px]"
          aria-busy={searching}
        />
      </div>
      <Select
        items={STATUS_ITEMS}
        value={filters.status ?? "all"}
        onValueChange={(value) => onFilter("status", value === null || value === "all" ? null : (value as RepoStatus))}
      >
        <SelectTrigger aria-label="Status" className="w-40">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {STATUS_ITEMS.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select
        items={VISIBILITY_OPTIONS}
        value={filters.visibility}
        onValueChange={(value) => onFilter("visibility", value === null || value === "all" ? null : value)}
      >
        <SelectTrigger aria-label="Visibility" className="w-36">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {VISIBILITY_OPTIONS.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select
        items={SORT_OPTIONS}
        value={filters.sort}
        onValueChange={(value) => onFilter("sort", value === null || value === "updated_desc" ? null : value)}
      >
        <SelectTrigger aria-label="Sort" className="w-44">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {SORT_OPTIONS.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <div className="flex h-9 items-center gap-2 rounded-lg border border-input bg-card px-3">
        <Switch
          id="included-only"
          checked={filters.selectedOnly}
          onCheckedChange={(checked) => onFilter("included", checked ? "1" : null)}
        />
        <Label htmlFor="included-only" className="text-[13px] font-normal whitespace-nowrap">
          Selected only
        </Label>
      </div>
    </div>
  )
}

function SelectionBar({ total, onPage, onClear, loaded }: { total: number; onPage: number; onClear: () => void; loaded: boolean }) {
  return (
    <div className="flex min-h-11 flex-wrap items-center justify-between gap-2 border-b bg-muted/40 px-4 py-1.5 text-[13px]">
      <p aria-live="polite" className="text-muted-foreground">
        {total === 0 ? (
          "Select repositories with the checkboxes to sync them together."
        ) : (
          <>
            <span className="font-medium text-foreground">{onPage.toLocaleString("en")} selected</span> on this page
            {loaded ? <> · <span className="font-medium text-foreground">{total.toLocaleString("en")}</span> total selected</> : null}
          </>
        )}
      </p>
      {total > 0 ? (
        <Button variant="ghost" size="sm" onClick={onClear}>
          <Icon name="action.cancel" />
          Clear selection
        </Button>
      ) : null}
    </div>
  )
}

function RepositoryTable({
  items,
  selected,
  revision,
  onSelect,
  busy,
}: {
  items: RepositorySummary[]
  selected: Set<string>
  revision: number
  onSelect: (repos: RepositorySummary[], selected: boolean) => void
  busy: boolean
}) {
  const navigate = useNavigate()
  const pageCount = items.filter((repo) => selected.has(repo.id)).length
  const all = pageCount === items.length
  const stop = (event: MouseEvent) => event.stopPropagation()

  return (
    <Table className="min-w-[720px] table-fixed text-[13px]" aria-busy={busy}>
      <colgroup>
        <col className="w-10" />
        <col />
        <col className="w-[10%]" />
        <col className="w-[11%]" />
        <col className="w-[11%]" />
        <col className="w-[12%]" />
        <col className="w-36" />
        <col className="w-12" />
      </colgroup>
      <TableHeader className="text-xs text-muted-foreground">
        <TableRow className="hover:bg-transparent">
          <TableHead className="pl-4">
            <Checkbox
              aria-label="Select all repositories on this page"
              checked={all}
              indeterminate={pageCount > 0 && !all}
              onCheckedChange={(checked) => onSelect(items, checked)}
            />
          </TableHead>
          <TableHead className="font-medium text-muted-foreground">Repository</TableHead>
          <TableHead className="font-medium text-muted-foreground">Branch</TableHead>
          <TableHead className="font-medium text-muted-foreground">Local mirror</TableHead>
          <TableHead className="font-medium text-muted-foreground">GitLab</TableHead>
          <TableHead className="font-medium text-muted-foreground">Observed</TableHead>
          <TableHead className="font-medium text-muted-foreground">Status</TableHead>
          <TableHead className="pr-4">
            <span className="sr-only">Actions</span>
          </TableHead>
        </TableRow>
      </TableHeader>
      <TableBody className={cn(busy && "opacity-60 transition-opacity")}>
        {items.map((repo) => {
          const isSelected = selected.has(repo.id)
          const detail = `/repositories/${encodeURIComponent(repo.id)}`
          const observed = relativeTime(repo.observedAt)
          return (
            <TableRow
              key={repo.id}
              data-state={isSelected ? "selected" : undefined}
              className="h-14 cursor-pointer data-[state=selected]:bg-primary-soft/50"
              onClick={() => navigate(detail)}
            >
              <TableCell className="pl-4" onClick={stop}>
                <Checkbox
                  aria-label={`Select ${repo.fullName}`}
                  checked={isSelected}
                  onCheckedChange={(checked) => onSelect([repo], checked)}
                />
              </TableCell>
              <TableCell>
                <div className="flex min-w-0 items-center gap-2.5">
                  <Icon name="nav.repositories" className="text-muted-foreground" />
                  <div className="flex min-w-0 flex-col">
                    <Link
                      to={detail}
                      onClick={stop}
                      title={repo.fullName}
                      className="min-w-0 truncate rounded-sm font-medium outline-none hover:underline focus-visible:ring-3 focus-visible:ring-ring/50"
                    >
                      {splitFullName(repo.fullName).name}
                    </Link>
                    <span className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                      <span className="min-w-8 shrink truncate" title={repo.fullName}>
                        {splitFullName(repo.fullName).owner}
                      </span>
                      <RepoFlags repo={repo} />
                    </span>
                  </div>
                </div>
              </TableCell>
              <TableCell>
                {repo.defaultBranch ? (
                  <span className="flex min-w-0 items-center gap-1.5" title={repo.defaultBranch}>
                    <Icon name="metadata.branch" size={14} className="text-muted-foreground" />
                    <span className="truncate">{repo.defaultBranch}</span>
                  </span>
                ) : (
                  <span className="text-muted-foreground">No branch</span>
                )}
              </TableCell>
              <TableCell className="truncate">{MIRROR_STATE[repo.mirrorState]}</TableCell>
              <TableCell className="truncate" title={destinationLabel(repo)}>
                {destinationLabel(repo)}
              </TableCell>
              <TableCell className="truncate text-muted-foreground" title={dateTime(repo.observedAt) ?? undefined}>
                {observed ?? "Never"}
              </TableCell>
              <TableCell>
                <StatusBadge status={REPOSITORY_STATUS[repo.primaryStatus]} />
              </TableCell>
              <TableCell className="pr-4 text-right" onClick={stop}>
                <RepositoryActionsMenu repo={repo} listRevision={revision} />
              </TableCell>
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}

function RepoFlags({ repo }: { repo: RepositorySummary }) {
  const flags = [repo.isPrivate && "Private", repo.isFork && "Fork", repo.isArchived && "Archived", !repo.selected && "Excluded"].filter(
    (flag): flag is string => typeof flag === "string",
  )
  if (flags.length === 0) return null
  return (
    <span className="flex min-w-0 items-center gap-1 overflow-hidden">
      {flags.map((flag) => (
        <span key={flag} className="shrink-0 rounded border px-1 text-xs leading-4">
          {flag}
        </span>
      ))}
    </span>
  )
}

function TableSkeleton() {
  return (
    <div aria-busy="true" className="flex flex-col">
      <LoadingLabel label="Loading repositories" />
      <div className="h-10 border-b" />
      {Array.from({ length: 6 }, (_, index) => (
        <div key={index} className="flex h-14 items-center gap-4 border-b px-4 last:border-0">
          <Skeleton className="size-4 rounded" />
          <Skeleton className="h-3.5 w-1/4" />
          <Skeleton className="h-3.5 w-16" />
          <Skeleton className="h-3.5 w-14" />
          <Skeleton className="h-3.5 w-16" />
          <Skeleton className="ml-auto h-6 w-24 rounded-md" />
        </div>
      ))}
    </div>
  )
}

function Pager({
  start,
  count,
  total,
  hasPrevious,
  nextCursor,
  onPrevious,
  onNext,
}: {
  start: number
  count: number
  total: number | null
  hasPrevious: boolean
  nextCursor: string | null
  onPrevious: () => void
  onNext: (cursor: string) => void
}) {
  return (
    <nav aria-label="Repository pages" className="flex flex-wrap items-center justify-between gap-3">
      <p className="text-xs text-muted-foreground" aria-live="polite">
        Showing {start.toLocaleString("en")}–{(start + count - 1).toLocaleString("en")} of{" "}
        {total === null ? "unknown" : total.toLocaleString("en")}
      </p>
      <div className="flex items-center gap-2">
        <Button variant="outline" onClick={onPrevious} disabled={!hasPrevious}>
          <Icon name="action.previous" />
          Previous
        </Button>
        <Button variant="outline" onClick={() => nextCursor && onNext(nextCursor)} disabled={nextCursor === null}>
          Next
          <Icon name="action.next" />
        </Button>
      </div>
    </nav>
  )
}
