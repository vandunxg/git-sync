import { describe, expect, it } from "vitest"

import type { Job } from "@/lib/native"

import { countJobs, groupJobs, jobAction, stageSteps, visibleLogText } from "./jobs.ts"

function job(partial: Partial<Job>): Job {
  return {
    id: "job-1",
    repositoryId: "r-1",
    attempt: 1,
    state: "queued",
    stage: "preflight",
    progressPercent: null,
    transferredBytes: null,
    totalBytes: null,
    etaSeconds: null,
    error: null,
    coverage: { gitRefs: "not_checked", localObjects: "not_checked", lfs: "not_checked", submodules: "none" },
    revision: 1,
    accountId: null,
    planId: null,
    kind: "sync",
    ...partial,
  }
}

describe("sync center derivations", () => {
  it("puts active jobs before recent terminal jobs and keeps identity", () => {
    const groups = groupJobs([job({ id: "a", state: "succeeded" }), job({ id: "b", state: "running" }), job({ id: "c", state: "failed" })])
    expect(groups.active.map((item) => item.id)).toEqual(["b"])
    expect(groups.recent.map((item) => item.id)).toEqual(["a", "c"])
  })

  it("counts only reported states", () => {
    const counts = countJobs([
      job({ state: "running" }),
      job({ state: "queued" }),
      job({ state: "waiting_for_approval" }),
      job({ state: "failed" }),
      job({ state: "succeeded" }),
    ])
    expect(counts).toEqual({ running: 1, waiting: 1, review: 1, failed: 1 })
  })

  it("confirms cancel only while pushing and never offers fake pause/start", () => {
    expect(jobAction(job({ state: "running", stage: "push" }))).toEqual({ kind: "cancel", confirm: true })
    expect(jobAction(job({ state: "running", stage: "fetch" }))).toEqual({ kind: "cancel", confirm: false })
    expect(jobAction(job({ state: "cancel_requested" }))).toEqual({ kind: "canceling" })
    expect(jobAction(job({ state: "partial" }))).toEqual({ kind: "retry" })
    expect(jobAction(job({ state: "waiting_for_approval", planId: "p" }))).toEqual({ kind: "review", planId: "p" })
    expect(jobAction(job({ state: "waiting_for_approval" }))).toEqual({ kind: "none" })
    expect(jobAction(job({ state: "succeeded" }))).toEqual({ kind: "none" })
  })

  it("derives stage steps relative to the reported stage", () => {
    const running = stageSteps(job({ state: "running", stage: "push" }))
    expect(running.find((step) => step.stage === "fetch")?.status).toBe("done")
    expect(running.find((step) => step.stage === "push")?.status).toBe("running")
    expect(running.find((step) => step.stage === "verify")?.status).toBe("pending")
    const failed = stageSteps(job({ state: "failed", stage: "push" }))
    expect(failed.find((step) => step.stage === "push")?.status).toBe("failed")
    expect(failed.find((step) => step.stage === "verify")?.status).toBe("not_run")
    expect(stageSteps(job({ state: "succeeded", stage: "finalize" })).every((step) => step.status === "done")).toBe(true)
    expect(stageSteps(job({ kind: "scan", state: "running", stage: "observe_source" }))).toEqual([{ stage: "observe_source", status: "running" }])
  })

  it("shows control characters as visible symbols", () => {
    expect(visibleLogText("\u001b[31mred\u001b[0m\tok")).toBe("␛[31mred␛[0m\tok")
  })
})
