import { IconTile, JOB_KIND, JOB_STAGE, JOB_STATE, StatusBadge } from "@/components/shared"
import { Progress } from "@/components/ui/progress"
import { relativeTime } from "@/lib/format"
import type { IconKey } from "@/lib/icons"
import type { Job } from "@/lib/native"
import { cn } from "@/lib/utils"

import { JobActionButton } from "./JobActionButton.tsx"
import { useJobName } from "./useJobName.ts"

export const KIND_ICON: Record<Job["kind"], IconKey> = {
  scan: "action.scan",
  plan: "status.update",
  sync: "action.sync",
  restore_test: "job.verify",
}

/** Progress text only from reported values; null percent while running is indeterminate. */
function ProgressCell({ job, queuePaused }: { job: Job; queuePaused: boolean | undefined }) {
  if (job.state === "running" || job.state === "cancel_requested") {
    if (job.progressPercent === null) return <span className="text-xs text-muted-foreground">In progress</span>
    const percent = Math.max(0, Math.min(100, Math.round(job.progressPercent)))
    return (
      <span className="flex w-full items-center gap-2">
        <Progress value={percent} aria-label={`Stage progress ${percent}%`} className="min-w-0 flex-1" />
        <span className="w-9 text-right text-xs font-medium tabular-nums">{percent}%</span>
      </span>
    )
  }
  if (job.state === "queued") {
    return <span className="text-xs text-muted-foreground">{queuePaused ? "Held — queue paused" : "Waiting to start"}</span>
  }
  if (job.state === "waiting_for_retry" && job.error?.retryAt) {
    return <span className="text-xs text-muted-foreground">Retries {relativeTime(job.error.retryAt)}</span>
  }
  return null
}

export function JobRow({
  job,
  selected,
  queuePaused,
  onSelect,
  onRetried,
  registerRef,
}: {
  job: Job
  selected: boolean
  queuePaused: boolean | undefined
  onSelect: (jobId: string) => void
  onRetried: (jobId: string) => void
  registerRef: (jobId: string, element: HTMLButtonElement | null) => void
}) {
  const title = useJobName(job)
  const state = JOB_STATE[job.state]
  const subtitle = job.repositoryId === null ? JOB_STAGE[job.stage] : `${JOB_KIND[job.kind]} · ${JOB_STAGE[job.stage]}`

  return (
    <li
      className={cn(
        "flex min-w-0 items-center gap-2 rounded-lg border border-transparent pr-2 transition-colors",
        selected ? "border-primary/25 bg-primary-soft" : "hover:bg-muted/60",
      )}
    >
      <button
        type="button"
        ref={(element) => registerRef(job.id, element)}
        aria-current={selected ? "true" : undefined}
        onClick={() => onSelect(job.id)}
        className="flex min-h-14 min-w-0 flex-1 items-center gap-3 rounded-lg px-2 py-2 text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <IconTile icon={KIND_ICON[job.kind]} tone={state.tone === "danger" ? "danger" : state.tone === "info" ? "info" : "neutral"} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-semibold" title={title.full}>
            {title.name}
          </span>
          <span className="block truncate text-xs text-muted-foreground" title={subtitle}>
            {subtitle}
          </span>
          <span className="flex max-w-56 empty:hidden @[44rem]:hidden">
            <ProgressCell job={job} queuePaused={queuePaused} />
          </span>
        </span>
        <span className="hidden w-36 shrink-0 @[44rem]:flex">
          <ProgressCell job={job} queuePaused={queuePaused} />
        </span>
        <span className="flex shrink-0 @[44rem]:w-32">
          <StatusBadge status={state} />
        </span>
      </button>
      <span className="flex shrink-0 justify-end empty:hidden @[44rem]:flex @[44rem]:w-36">
        <JobActionButton job={job} name={title.name} onRetried={onRetried} />
      </span>
    </li>
  )
}
