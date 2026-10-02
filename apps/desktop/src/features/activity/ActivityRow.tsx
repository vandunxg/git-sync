import { Link } from "react-router"

import { ACTIVITY_TYPE_LABEL, activityVisual, IconTile, SEVERITY, StatusBadge } from "@/components/shared"
import { keys, useNativeQuery } from "@/lib/api"
import { clockTime, dateTime, splitFullName } from "@/lib/format"
import type { ActivityEntry } from "@/lib/native"
import { cn } from "@/lib/utils"

/** Repository display name for a known ID; unavailable records degrade to a neutral label. */
export function useRepositoryName(repositoryId: string | null): { name: string; full: string } | null {
  const repository = useNativeQuery(keys.repository(repositoryId ?? ""), "gbm_repository_get", { repositoryId: repositoryId ?? "" }, {
    enabled: repositoryId !== null,
    staleTime: 60_000,
  })
  if (repositoryId === null) return null
  if (repository.data) return { name: splitFullName(repository.data.fullName).name, full: repository.data.fullName }
  if (repository.isPending) return { name: "Repository", full: repositoryId }
  return { name: "Repository unavailable", full: `Repository ${repositoryId} could not be loaded` }
}

const contextLink =
  "relative z-10 rounded-sm font-medium text-primary underline-offset-2 outline-none hover:underline focus-visible:ring-3 focus-visible:ring-ring/50"

export function ActivityRow({
  entry,
  selected,
  onOpen,
  registerRef,
}: {
  entry: ActivityEntry
  selected: boolean
  onOpen: (eventId: string) => void
  registerRef: (eventId: string, element: HTMLButtonElement | null) => void
}) {
  const visual = activityVisual(entry)
  const repository = useRepositoryName(entry.repositoryId)
  const time = clockTime(entry.occurredAt)

  return (
    <li
      className={cn(
        "relative grid min-w-0 grid-cols-[3rem_2rem_minmax(0,1fr)] items-start gap-x-3 rounded-lg px-2 py-2.5 transition-colors @[40rem]:grid-cols-[3rem_2rem_minmax(0,1fr)_auto]",
        selected ? "bg-primary-soft" : "hover:bg-muted/60",
      )}
    >
      <time dateTime={entry.occurredAt} title={dateTime(entry.occurredAt) ?? undefined} className="pt-1.5 text-xs text-muted-foreground tabular-nums">
        {time ?? "—"}
      </time>
      <IconTile icon={visual.icon} tone={visual.tone} />
      <div className="min-w-0">
        <button
          type="button"
          ref={(element) => registerRef(entry.id, element)}
          aria-current={selected ? "true" : undefined}
          onClick={() => onOpen(entry.id)}
          className="block w-full truncate rounded-sm text-left text-sm font-semibold outline-none after:absolute after:inset-0 after:rounded-lg focus-visible:after:ring-3 focus-visible:after:ring-ring/50"
          title={entry.message}
        >
          {entry.message}
        </button>
        <p className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
          <span>{ACTIVITY_TYPE_LABEL[entry.type]}</span>
          {repository && entry.repositoryId ? (
            <Link to={`/repositories/${encodeURIComponent(entry.repositoryId)}`} className={cn(contextLink, "max-w-48 truncate")} title={repository.full}>
              {repository.name}
            </Link>
          ) : null}
          {entry.jobId ? (
            <Link to={`/sync?job=${encodeURIComponent(entry.jobId)}`} className={contextLink}>
              View job
            </Link>
          ) : null}
        </p>
      </div>
      {entry.severity !== "info" || entry.errorCode ? (
        <div className="col-start-3 mt-1.5 flex flex-wrap items-center gap-1.5 @[40rem]:col-start-4 @[40rem]:mt-0 @[40rem]:justify-end">
          {entry.severity !== "info" ? <StatusBadge status={SEVERITY[entry.severity]} /> : null}
          {entry.errorCode ? (
            <span className="inline-flex h-6 items-center rounded-md border px-2 font-mono text-xs text-muted-foreground">{entry.errorCode}</span>
          ) : null}
        </div>
      ) : null}
    </li>
  )
}
