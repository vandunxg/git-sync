import { describe, expect, it } from "vitest"

import type { RepositorySummary } from "@/lib/native"

import { breakdownSyncAll, describeAttention } from "./attention.ts"

function repo(partial: Partial<RepositorySummary>): RepositorySummary {
  return {
    id: "r-1",
    providerRepositoryId: "gh-1",
    fullName: "owner/name",
    defaultBranch: "main",
    isPrivate: false,
    isFork: false,
    isArchived: false,
    selected: true,
    sourceState: "current",
    mirrorState: "current",
    destinationState: "current",
    workspaceState: "disabled",
    primaryStatus: "synced",
    pendingRefChanges: 0,
    observedAt: null,
    verifiedAt: null,
    lastErrorCode: null,
    coverage: { gitRefs: "verified", localObjects: "verified", lfs: "not_applicable", submodules: "none" },
    ...partial,
  }
}

describe("describeAttention", () => {
  it("asks to reconnect before retrying an auth failure", () => {
    const view = describeAttention(repo({ primaryStatus: "failed", lastErrorCode: "AUTH_REQUIRED", destinationState: "inaccessible" }))
    expect(view.action).toEqual({ kind: "reconnect", label: "Reconnect GitLab" })
  })

  it("routes history-changing plans to review, never to a direct sync", () => {
    const view = describeAttention(repo({ primaryStatus: "blocked", lastErrorCode: "APPROVAL_REQUIRED" }))
    expect(view.action?.kind).toBe("review")
  })

  it("maps missing, update, failed and stale to their specific actions", () => {
    expect(describeAttention(repo({ primaryStatus: "missing", mirrorState: "missing" })).action?.label).toBe("Create local copy")
    expect(describeAttention(repo({ primaryStatus: "update", pendingRefChanges: 3 })).problem).toBe("3 ref changes awaiting backup")
    expect(describeAttention(repo({ primaryStatus: "update", pendingRefChanges: null })).problem).toBe("Ref changes awaiting backup")
    expect(describeAttention(repo({ primaryStatus: "failed" })).action?.label).toBe("Retry")
    expect(describeAttention(repo({ primaryStatus: "stale" })).action?.label).toBe("Retry scan")
  })

  it("offers no action for a running repository", () => {
    expect(describeAttention(repo({ primaryStatus: "running" })).action).toBeNull()
  })
})

describe("breakdownSyncAll", () => {
  it("excludes up-to-date, blocked and running repositories with exact counts", () => {
    const items = [
      repo({ id: "a", primaryStatus: "update" }),
      repo({ id: "b", primaryStatus: "failed" }),
      repo({ id: "c", primaryStatus: "synced" }),
      repo({ id: "d", primaryStatus: "blocked" }),
      repo({ id: "e", primaryStatus: "running" }),
      repo({ id: "f", primaryStatus: "unknown" }),
    ]
    const result = breakdownSyncAll(items)
    expect(result.included.map((item) => item.id)).toEqual(["a", "b", "f"])
    expect(result).toMatchObject({ needsReview: 1, running: 1, upToDate: 1 })
  })
})
