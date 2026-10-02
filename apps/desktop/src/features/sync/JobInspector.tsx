import { useId, useState, type ReactNode } from "react"

import { IconTile, JOB_KIND, JOB_STAGE, JOB_STATE, StatusBadge, type StatusDescriptor } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { ERROR_COPY } from "@/lib/api"
import { formatBytes, formatDuration, relativeTime } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { Coverage, Job } from "@/lib/native"
import { cn } from "@/lib/utils"

import { JobActionButton } from "./JobActionButton.tsx"
import { JobLogViewer } from "./JobLogViewer.tsx"
import { KIND_ICON } from "./JobRow.tsx"
import { stageSteps, type StepStatus } from "./jobs.ts"
import { useJobName } from "./useJobName.ts"

const STEP: Record<Exclude<StepStatus, "warning">, StatusDescriptor> = {
  done: { label: "Done", icon: "status.synced", tone: "success" },
  running: { label: "Running", icon: "status.running", tone: "info", spinning: true },
  next: { label: "Next", icon: "status.waiting", tone: "neutral" },
  waiting: { label: "Needs review", icon: "status.blocked", tone: "warning" },
  failed: { label: "Failed", icon: "status.failed", tone: "danger" },
  stopped: { label: "Canceled", icon: "status.no_changes", tone: "neutral" },
  pending: { label: "Pending", icon: "status.waiting", tone: "neutral" },
  not_run: { label: "Not run", icon: "status.no_changes", tone: "neutral" },
}

const TONE_TEXT: Record<StatusDescriptor["tone"], string> = {
  success: "text-success",
  warning: "text-warning",
  danger: "text-danger",
  info: "text-primary",
  neutral: "text-muted-foreground",
}

const GIT_CHECK: Record<Coverage["gitRefs"], string> = { not_checked: "Not checked", verified: "Verified", failed: "Failed" }
const LFS: Record<Coverage["lfs"], string> = {
  not_applicable: "Not used",
  not_checked: "Not checked",
  local_verified: "Local copy verified",
  uploaded: "Uploaded",
  restore_verified: "Restore verified",
  incomplete: "Incomplete",
}
const SUBMODULES: Record<Coverage["submodules"], string> = {
  none: "None",
  pointers_only: "Pointers only",
  separately_covered: "Covered separately",
  incomplete: "Incomplete",
}

function Detail({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 items-baseline justify-between gap-3 py-1.5">
      <dt className="shrink-0 text-xs text-muted-foreground">{label}</dt>
      <dd className="min-w-0 truncate text-right text-[13px]">{children}</dd>
    </div>
  )
}

function Transfer({ job }: { job: Job }) {
  const transferred = formatBytes(job.transferredBytes)
  const total = formatBytes(job.totalBytes)
  const eta = formatDuration(job.etaSeconds)
  if (transferred === null && total === null && eta === null) return null
  const percent =
    job.transferredBytes !== null && job.totalBytes !== null && job.totalBytes > 0
      ? Math.min(100, Math.round((job.transferredBytes / job.totalBytes) * 100))
      : null
  return (
    <div className="flex flex-col gap-2 rounded-lg border bg-muted/30 px-3 py-2.5">
      <p className="text-[13px]">
        {transferred !== null && total !== null
          ? `${transferred} of ${total} transferred`
          : transferred !== null
            ? `${transferred} transferred · total not reported`
            : `Total ${total}`}
      </p>
      {percent !== null && job.state === "running" ? <Progress value={percent} aria-label={`Transferred ${percent}% of reported total`} /> : null}
      {eta !== null && job.state === "running" ? (
        <p className="text-xs text-muted-foreground">About {eta} remaining (estimate reported by Git)</p>
      ) : null}
    </div>
  )
}

function JobError({ error }: { error: NonNullable<Job["error"]> }) {
  return (
    <div role="group" aria-label="Job error" className="flex flex-col gap-1.5 rounded-lg border border-danger/30 bg-danger-soft px-3 py-2.5 text-danger">
      <p className="flex items-center gap-2 text-[13px] font-semibold">
        <Icon name="state.error" />
        {ERROR_COPY[error.code]}
      </p>
      {error.message !== ERROR_COPY[error.code] ? <p className="text-[13px] text-danger/90">{error.message}</p> : null}
      {error.details ? (
        <pre className="max-h-32 overflow-auto rounded-md bg-card/60 p-2 font-mono text-xs break-words whitespace-pre-wrap text-foreground ui-selectable">
          {error.details}
        </pre>
      ) : null}
      <p className="font-mono text-xs text-danger/75 ui-selectable">
        {error.code} · ref {error.correlationId.slice(0, 8)}
        {error.retryAt ? ` · retry ${relativeTime(error.retryAt)}` : ""}
      </p>
    </div>
  )
}

/** Inspector for the selected job: real stage sequence, scope and observed transfer data. */
export function JobInspector({
  job,
  onRetried,
  titleId,
  hideTitle = false,
}: {
  job: Job
  onRetried: (jobId: string) => void
  titleId?: string
  hideTitle?: boolean
}) {
  const name = useJobName(job)
  const [showLogs, setShowLogs] = useState(false)
  const stepsId = useId()
  const state = JOB_STATE[job.state]
  const steps = stageSteps(job)

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <div className={cn("flex min-w-0 items-start gap-3", hideTitle && "sr-only")}>
        <IconTile icon={KIND_ICON[job.kind]} tone={state.tone === "danger" ? "danger" : "info"} />
        <div className="min-w-0 flex-1">
          <h2 id={titleId} className="truncate text-[15px] leading-snug font-semibold" title={name.full}>
            {name.name}
          </h2>
          <p className="truncate text-[13px] text-muted-foreground">{JOB_STAGE[job.stage]}</p>
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <StatusBadge status={state} />
        <JobActionButton job={job} name={name.name} onRetried={onRetried} />
      </div>

      <dl className="divide-y divide-border/70">
        <Detail label="Type">{JOB_KIND[job.kind]}</Detail>
        <Detail label="Attempt">{job.attempt}</Detail>
        {job.repositoryId !== null && name.full !== name.name ? <Detail label="Repository">{name.full}</Detail> : null}
        <Detail label="Job ID">
          <span className="font-mono text-xs ui-selectable">{job.id}</span>
        </Detail>
      </dl>

      <section aria-labelledby={stepsId} className="flex flex-col gap-1">
        <h3 id={stepsId} className="text-[13px] font-semibold">
          {job.kind === "sync" ? "Stages" : "Current stage"}
        </h3>
        <ol className="flex flex-col">
          {steps.map((step) => {
            const descriptor: StatusDescriptor = step.status === "warning" ? JOB_STATE[job.state] : STEP[step.status]
            const current = step.stage === job.stage
            return (
              <li key={step.stage} className="flex min-h-9 items-center gap-2.5" aria-current={current ? "step" : undefined}>
                <IconTile icon={descriptor.icon} tone={descriptor.tone} size="sm" spinning={descriptor.spinning} />
                <span className={cn("min-w-0 flex-1 truncate text-[13px]", current ? "font-semibold" : "text-foreground/85")}>
                  {JOB_STAGE[step.stage]}
                </span>
                <span className={cn("shrink-0 text-xs font-medium", TONE_TEXT[descriptor.tone])}>{descriptor.label}</span>
              </li>
            )
          })}
        </ol>
        {job.state === "running" && job.progressPercent === null ? (
          <p className="text-xs text-muted-foreground">Git has not reported a total for this stage, so progress is not estimated.</p>
        ) : null}
      </section>

      <Transfer job={job} />

      <section aria-label="Backup coverage" className="flex flex-col gap-1">
        <h3 className="text-[13px] font-semibold">Coverage</h3>
        <dl className="divide-y divide-border/70">
          <Detail label="Git refs">{GIT_CHECK[job.coverage.gitRefs]}</Detail>
          <Detail label="Local objects">{GIT_CHECK[job.coverage.localObjects]}</Detail>
          <Detail label="LFS">{LFS[job.coverage.lfs]}</Detail>
          <Detail label="Submodules">{SUBMODULES[job.coverage.submodules]}</Detail>
        </dl>
      </section>

      {job.error ? <JobError error={job.error} /> : null}

      {showLogs ? (
        <JobLogViewer jobId={job.id} />
      ) : (
        <Button variant="outline" className="self-start" onClick={() => setShowLogs(true)}>
          <Icon name="nav.activity" />
          View logs
        </Button>
      )}
    </div>
  )
}
