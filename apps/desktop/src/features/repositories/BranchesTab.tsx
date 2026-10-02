import { EmptyState, ErrorState, RowSkeletons, SectionCard, StatusBadge } from "@/components/shared"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { keys, useNativeQuery } from "@/lib/api"
import { dateTime, plural, relativeTime, shortOid } from "@/lib/format"
import { Icon } from "@/lib/icons"

import { CopyButton } from "./CopyButton.tsx"
import { compareRefs, REF_COMPARISON } from "./model.ts"

/** Read-only comparison of observed refs; not a branch editor (UI-05). */
export function BranchesTab({ repositoryId }: { repositoryId: string }) {
  const refs = useNativeQuery(keys.refs(repositoryId), "gbm_repository_refs", { repositoryId })
  const rows = refs.data ? compareRefs(refs.data) : []
  const differing = rows.filter((row) => row.comparison !== "same").length
  const observed = relativeTime(refs.data?.observedAt)

  return (
    <SectionCard
      icon="metadata.branch"
      title="Branches and tags"
      description={
        refs.data
          ? `${plural(rows.length, "ref")} · ${differing === 0 ? "all match" : `${differing} not matching`} · observed ${observed ?? "at an unknown time"}`
          : "Source and destination refs"
      }
    >
      {refs.isPending ? (
        <RowSkeletons rows={4} />
      ) : refs.isError ? (
        <ErrorState error={refs.error.appError} title="Could not load refs" onRetry={() => void refs.refetch()} retrying={refs.isFetching} />
      ) : (
        <div className="flex flex-col gap-3">
          {!refs.data.observationComplete ? (
            <p role="status" className="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning-soft px-3.5 py-2.5 text-[13px] text-warning">
              <Icon name="state.warning" className="mt-0.5" />
              Observation incomplete: some refs could not be read, so this comparison may be missing entries.
            </p>
          ) : null}
          {rows.length === 0 ? (
            <EmptyState className="py-6" icon="metadata.branch" title="No refs observed" description="An empty repository has no branches or tags yet." />
          ) : (
            <div className="rounded-lg border">
              <Table className="min-w-[640px] table-fixed text-[13px]">
                <colgroup>
                  <col />
                  <col className="w-40" />
                  <col className="w-40" />
                  <col className="w-44" />
                </colgroup>
                <TableHeader className="text-xs">
                  <TableRow className="hover:bg-transparent">
                    <TableHead className="pl-3 font-medium text-muted-foreground">Ref</TableHead>
                    <TableHead className="font-medium text-muted-foreground">GitHub</TableHead>
                    <TableHead className="font-medium text-muted-foreground">GitLab</TableHead>
                    <TableHead className="pr-3 font-medium text-muted-foreground">Comparison</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((row) => (
                    <TableRow key={row.name} className="h-11 hover:bg-transparent">
                      <TableCell className="truncate pl-3 font-mono text-xs" title={row.name}>
                        {row.name}
                      </TableCell>
                      <TableCell>
                        <OidCell oid={row.sourceOid} label={`Copy GitHub object ID for ${row.name}`} />
                      </TableCell>
                      <TableCell>
                        <OidCell oid={row.destinationOid} label={`Copy GitLab object ID for ${row.name}`} />
                      </TableCell>
                      <TableCell className="pr-3">
                        <StatusBadge status={REF_COMPARISON[row.comparison]} />
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
          {refs.data.observedAt ? (
            <p className="text-xs text-muted-foreground">Observed {dateTime(refs.data.observedAt)}. Ref changes are made only by reviewed sync plans.</p>
          ) : null}
        </div>
      )}
    </SectionCard>
  )
}

function OidCell({ oid, label }: { oid: string | null; label: string }) {
  if (oid === null) return <span className="text-muted-foreground">Not present</span>
  return (
    <span className="inline-flex items-center gap-1">
      <code className="font-mono text-xs ui-selectable" title={oid}>
        {shortOid(oid)}
      </code>
      <CopyButton value={oid} label={label} />
    </span>
  )
}
