// Pure view derivations for the Sync Center. Everything is computed from Job DTOs; no
// timestamps, speeds or totals are invented when the native side does not report them.

import { ACTIVE_JOB_STATES, JOB_STAGE_ORDER } from "@/components/shared"
import type { Job } from "@/lib/native"

export interface JobGroups {
  active: Job[]
  recent: Job[]
}

/** Active jobs first (native order preserved), then terminal jobs as "Recent". */
export function groupJobs(jobs: readonly Job[]): JobGroups {
  const active: Job[] = []
  const recent: Job[] = []
  for (const job of jobs) (ACTIVE_JOB_STATES.has(job.state) ? active : recent).push(job)
  return { active, recent }
}

export interface JobCounts {
  running: number
  waiting: number
  review: number
  failed: number
}

/** Counts over the loaded job list only. */
export function countJobs(jobs: readonly Job[]): JobCounts {
  const counts: JobCounts = { running: 0, waiting: 0, review: 0, failed: 0 }
  for (const job of jobs) {
    if (job.state === "running" || job.state === "cancel_requested") counts.running += 1
    else if (job.state === "queued" || job.state === "waiting_for_retry") counts.waiting += 1
    else if (job.state === "waiting_for_approval") counts.review += 1
    else if (job.state === "failed") counts.failed += 1
  }
  return counts
}

export type JobAction =
  | { kind: "cancel"; confirm: boolean }
  | { kind: "canceling" }
  | { kind: "retry" }
  | { kind: "review"; planId: string }
  | { kind: "none" }

/** The single contextual action a job row offers; only commands from the contract. */
export function jobAction(job: Pick<Job, "state" | "stage" | "planId">): JobAction {
  switch (job.state) {
    case "queued":
    case "running":
    case "waiting_for_retry":
      // Canceling a write in progress may leave the destination partially updated.
      return { kind: "cancel", confirm: job.state === "running" && job.stage === "push" }
    case "cancel_requested":
      return { kind: "canceling" }
    case "failed":
    case "interrupted":
    case "partial":
      return { kind: "retry" }
    case "waiting_for_approval":
      return job.planId ? { kind: "review", planId: job.planId } : { kind: "none" }
    case "succeeded":
    case "canceled":
      return { kind: "none" }
  }
}

export type StepStatus = "done" | "running" | "next" | "waiting" | "failed" | "warning" | "stopped" | "pending" | "not_run"

export interface StageStep {
  stage: Job["stage"]
  status: StepStatus
}

function currentStatus(state: Job["state"]): StepStatus {
  switch (state) {
    case "running":
    case "cancel_requested":
      return "running"
    case "queued":
    case "waiting_for_retry":
      return "next"
    case "waiting_for_approval":
      return "waiting"
    case "succeeded":
      return "done"
    case "failed":
      return "failed"
    case "partial":
    case "interrupted":
      return "warning"
    case "canceled":
      return "stopped"
  }
}

/**
 * Stage sequence relative to the job's reported stage. Only sync jobs run the full
 * pipeline; other kinds report the current stage alone so no stage is implied.
 */
export function stageSteps(job: Pick<Job, "kind" | "stage" | "state">): StageStep[] {
  const current = JOB_STAGE_ORDER.indexOf(job.stage)
  const status = currentStatus(job.state)
  if (job.kind !== "sync") return [{ stage: job.stage, status }]
  const terminal = !ACTIVE_JOB_STATES.has(job.state)
  return JOB_STAGE_ORDER.map((stage, index) => {
    if (index < current) return { stage, status: "done" }
    if (index === current) return { stage, status }
    // A finished job never reached later stages; they are not shown as pending work.
    return { stage, status: terminal ? "not_run" : "pending" }
  })
}

export { visibleLogText } from "@/lib/format"
