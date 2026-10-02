import type { ReactNode, RefObject } from "react"
import { Link } from "react-router"

import { ACTIVITY_TYPE_LABEL, activityVisual, EmptyState, IconTile, SEVERITY, StatusBadge } from "@/components/shared"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { JobLogViewer } from "@/features/sync"
import { ERROR_COPY } from "@/lib/api"
import { dateTime, relativeTime } from "@/lib/format"
import type { ActivityEntry } from "@/lib/native"

import { useRepositoryName } from "./ActivityRow.tsx"

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[7.5rem_minmax(0,1fr)] gap-3 py-2">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-[13px] break-words">{children}</dd>
    </div>
  )
}

function EventDetail({ entry }: { entry: ActivityEntry }) {
  const repository = useRepositoryName(entry.repositoryId)
  const when = dateTime(entry.occurredAt)
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <dl className="divide-y divide-border/70">
        <Row label="Occurred">
          {when ?? "Unknown time"}
          {when ? <span className="text-muted-foreground"> · {relativeTime(entry.occurredAt)}</span> : null}
        </Row>
        <Row label="Type">{ACTIVITY_TYPE_LABEL[entry.type]}</Row>
        <Row label="Severity">
          <StatusBadge status={SEVERITY[entry.severity]} />
        </Row>
        {entry.errorCode ? (
          <Row label="Error">
            <span className="font-mono text-xs">{entry.errorCode}</span>
            <span className="block text-muted-foreground">{ERROR_COPY[entry.errorCode]}</span>
          </Row>
        ) : null}
        <Row label="Repository">
          {entry.repositoryId && repository ? (
            <Link to={`/repositories/${encodeURIComponent(entry.repositoryId)}`} className="font-medium text-primary hover:underline" title={repository.full}>
              {repository.full}
            </Link>
          ) : (
            <span className="text-muted-foreground">Not linked to a repository</span>
          )}
        </Row>
        <Row label="Job">
          {entry.jobId ? (
            <Link to={`/sync?job=${encodeURIComponent(entry.jobId)}`} className="font-mono text-xs font-medium text-primary hover:underline">
              {entry.jobId}
            </Link>
          ) : (
            <span className="text-muted-foreground">No job</span>
          )}
        </Row>
        <Row label="Event ID">
          <span className="font-mono text-xs ui-selectable">{entry.id}</span>
        </Row>
      </dl>
      {entry.jobId ? (
        <JobLogViewer jobId={entry.jobId} />
      ) : (
        <p className="rounded-lg bg-muted/50 px-3 py-2.5 text-[13px] text-muted-foreground">This event has no job log.</p>
      )}
    </div>
  )
}

/** Event drawer driven by ?event=<id>; closing returns focus to the event row. */
export function ActivityDetailSheet({
  eventId,
  entry,
  loading,
  onClose,
  finalFocus,
}: {
  eventId: string | null
  entry: ActivityEntry | null
  loading: boolean
  onClose: () => void
  finalFocus: RefObject<HTMLElement | null>
}) {
  const visual = entry ? activityVisual(entry) : null
  return (
    <Sheet open={eventId !== null} onOpenChange={(open) => (open ? undefined : onClose())}>
      <SheetContent className="w-[min(480px,100vw)] gap-0 overflow-y-auto data-[side=right]:sm:max-w-[480px]" finalFocus={finalFocus}>
        <SheetHeader className="flex-row items-start gap-3 border-b pr-12">
          {visual ? <IconTile icon={visual.icon} tone={visual.tone} /> : null}
          <div className="min-w-0">
            <SheetTitle className="text-[15px] leading-snug font-semibold break-words">{entry ? entry.message : "Event details"}</SheetTitle>
            <SheetDescription className="text-[13px]">Event metadata and the related job log.</SheetDescription>
          </div>
        </SheetHeader>
        <div className="p-4">
          {entry ? (
            <EventDetail entry={entry} />
          ) : loading ? (
            <p className="text-[13px] text-muted-foreground" role="status">
              Loading event…
            </p>
          ) : (
            <EmptyState
              icon="status.unknown"
              title="Event not in the loaded results"
              description="It may be filtered out or older than the loaded pages. Clear filters or load more events."
            />
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}
