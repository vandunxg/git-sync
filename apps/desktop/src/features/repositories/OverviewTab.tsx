import type { ReactNode } from "react"
import { Link } from "react-router"

import {
  activityVisual,
  destinationLabel,
  EmptyState,
  ErrorState,
  MIRROR_STATE,
  RowSkeletons,
  SectionCard,
  StatusBadge,
  WORKSPACE_STATE,
} from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { keys, useNativeQuery } from "@/lib/api"
import { dateTime, relativeTime } from "@/lib/format"
import { BrandMark } from "@/lib/icons"
import type { RepositorySummary } from "@/lib/native"

import { SOURCE_STATE } from "./model.ts"

export function OverviewTab({ repo }: { repo: RepositorySummary }) {
  return (
    <div className="grid gap-5 xl:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]">
      <ConfigCard repo={repo} />
      <RecentAttempts repositoryId={repo.id} />
    </div>
  )
}

function ConfigCard({ repo }: { repo: RepositorySummary }) {
  const workspaceNote =
    repo.workspaceState === "dirty"
      ? "Local edits are never stashed or reset; the workspace update is skipped."
      : repo.workspaceState === "disabled"
        ? null
        : "A current mirror does not imply the working tree is current."
  return (
    <SectionCard icon="setting.backup" title="Repository" description="Source, destination and local copies">
      <dl className="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-6 gap-y-3 text-[13px]">
        <Row term="Source">
          <span className="flex min-w-0 items-center gap-2">
            <BrandMark provider="github" size={14} />
            <span className="truncate font-medium" title={repo.fullName}>
              {repo.fullName}
            </span>
          </span>
          <span className="text-xs text-muted-foreground">{SOURCE_STATE[repo.sourceState]}</span>
        </Row>
        <Row term="Destination">
          <span className="flex items-center gap-2">
            <BrandMark provider="gitlab" size={14} />
            <span className="font-medium">GitLab · {destinationLabel(repo)}</span>
          </span>
          {repo.pendingRefChanges !== null && repo.pendingRefChanges > 0 ? (
            <span className="text-xs text-muted-foreground">{repo.pendingRefChanges} ref changes awaiting backup</span>
          ) : null}
        </Row>
        <Row term="Local mirror">{MIRROR_STATE[repo.mirrorState]}</Row>
        <Row term="Workspace">
          {WORKSPACE_STATE[repo.workspaceState]}
          {workspaceNote ? <span className="text-xs text-muted-foreground">{workspaceNote}</span> : null}
        </Row>
        <Row term="Default branch">{repo.defaultBranch ?? <span className="text-muted-foreground">No branch</span>}</Row>
        <Row term="Last verified">
          <Time iso={repo.verifiedAt} />
        </Row>
        <Row term="Last observed">
          <Time iso={repo.observedAt} />
        </Row>
        <Row term="Backup selection">{repo.selected ? "Included" : "Excluded"}</Row>
      </dl>
    </SectionCard>
  )
}

function Row({ term, children }: { term: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-muted-foreground">{term}</dt>
      <dd className="flex min-w-0 flex-col gap-0.5">{children}</dd>
    </>
  )
}

function Time({ iso }: { iso: string | null }) {
  const relative = relativeTime(iso)
  if (relative === null) return <span className="text-muted-foreground">Never</span>
  return (
    <time dateTime={iso ?? undefined} title={dateTime(iso) ?? undefined} className="font-medium">
      {relative}
    </time>
  )
}

function RecentAttempts({ repositoryId }: { repositoryId: string }) {
  const request = { cursor: null, limit: 5, repositoryId, severity: "all" as const, search: "" }
  const activity = useNativeQuery([...keys.activity, "repository", request], "gbm_activity_list", request)
  return (
    <SectionCard
      icon="nav.activity"
      title="Recent attempts"
      description="Latest recorded events and outcomes"
      action={
        <Button variant="ghost" size="sm" nativeButton={false} render={<Link to={`/repositories/${encodeURIComponent(repositoryId)}/history`} />}>
          View history
        </Button>
      }
    >
      {activity.isPending ? (
        <RowSkeletons rows={3} />
      ) : activity.isError ? (
        <ErrorState error={activity.error.appError} title="Could not load attempts" onRetry={() => void activity.refetch()} retrying={activity.isFetching} />
      ) : activity.data.items.length === 0 ? (
        <EmptyState className="py-6" title="No attempts recorded yet" description="Sync and scan events for this repository appear here." />
      ) : (
        <Table className="table-fixed text-[13px]">
          <colgroup>
            <col className="w-28" />
            <col className="w-28" />
            <col />
          </colgroup>
          <TableHeader className="text-xs">
            <TableRow className="hover:bg-transparent">
              <TableHead className="font-medium text-muted-foreground">Time</TableHead>
              <TableHead className="font-medium text-muted-foreground">Result</TableHead>
              <TableHead className="font-medium text-muted-foreground">Message</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {activity.data.items.map((entry) => (
              <TableRow key={entry.id} className="h-12 hover:bg-transparent">
                <TableCell className="font-medium" title={dateTime(entry.occurredAt) ?? undefined}>
                  {relativeTime(entry.occurredAt) ?? "Unknown"}
                </TableCell>
                <TableCell>
                  <StatusBadge status={activityVisual(entry)} />
                </TableCell>
                <TableCell className="truncate text-muted-foreground" title={entry.message}>
                  {entry.message}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </SectionCard>
  )
}
