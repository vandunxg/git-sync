import { Link } from "react-router"

import { ACTIVE_JOB_STATES, IconTile } from "@/components/shared"
import { keys, useNativeQuery } from "@/lib/api"

/**
 * Service label for the job queue only (DD-04): it never claims "Backup healthy";
 * repository health lives on the Dashboard.
 */
export function QueueStatus() {
  const jobs = useNativeQuery(keys.jobs, "gbm_jobs_list", { cursor: null, limit: 100 })

  let title = "Queue status unknown"
  let detail = "Job list unavailable"
  let tone: "success" | "info" | "neutral" | "danger" = "neutral"
  let icon: "status.waiting" | "status.running" | "status.ready" | "status.unknown" = "status.unknown"
  let spinning = false

  if (jobs.data) {
    const running = jobs.data.items.filter((job) => job.state === "running" || job.state === "cancel_requested").length
    const review = jobs.data.items.filter((job) => job.state === "waiting_for_approval").length
    const waiting = jobs.data.items.filter((job) => ACTIVE_JOB_STATES.has(job.state)).length - running - review
    const extra = [waiting > 0 ? `${waiting} waiting` : null, review > 0 ? `${review} need review` : null].filter(Boolean).join(" · ")
    if (running > 0) {
      title = `${running} ${running === 1 ? "job" : "jobs"} running`
      detail = extra || "Nothing else queued"
      tone = "info"
      icon = "status.running"
      spinning = true
    } else if (waiting + review > 0) {
      title = "No job running"
      detail = extra
      icon = "status.waiting"
    } else {
      title = "Queue idle"
      detail = "No jobs running"
      tone = "success"
      icon = "status.ready"
    }
  } else if (jobs.isPending) {
    title = "Checking queue"
    detail = "Loading jobs…"
  }

  return (
    <Link
      to="/sync"
      className="flex items-center gap-2.5 rounded-lg border border-sidebar-border bg-card px-2.5 py-2 outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <IconTile icon={icon} tone={tone} size="sm" spinning={spinning} />
      <span className="min-w-0 leading-tight">
        <span className="block truncate text-[13px] font-semibold">{title}</span>
        <span className="block text-xs text-muted-foreground">{detail}</span>
      </span>
    </Link>
  )
}
