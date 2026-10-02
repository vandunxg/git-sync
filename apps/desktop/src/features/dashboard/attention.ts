// Pure derivation for one "Needs attention" row: the concise problem, a semantic tile and
// exactly one specific next action. Everything comes from RepositorySummary fields; no
// field is invented and unknown values stay unknown.

import { destinationLabel, MIRROR_STATE, REPOSITORY_STATUS, type StatusDescriptor } from "@/components/shared"
import { ERROR_COPY } from "@/lib/api"
import type { ErrorCode, RepositorySummary } from "@/lib/native"

export type AttentionActionKind = "reconnect" | "check_access" | "review" | "create_local" | "sync" | "retry" | "scan" | "details"

export interface AttentionAction {
  kind: AttentionActionKind
  label: string
}

export interface AttentionView {
  problem: string
  /** Secondary context: which copy is affected, in the same words as Repository details. */
  context: string
  visual: StatusDescriptor
  action: AttentionAction | null
}

const RECONNECT_CODES: ReadonlySet<ErrorCode> = new Set(["AUTH_REQUIRED", "ACCESS_DENIED"])
const REVIEW_CODES: ReadonlySet<ErrorCode> = new Set(["APPROVAL_REQUIRED", "DESTINATION_DIVERGED", "PLAN_STALE", "PROTECTED_REF"])

function pendingText(count: number | null): string {
  if (count === null) return "Ref changes awaiting backup"
  return `${count.toLocaleString("en")} ${count === 1 ? "ref change" : "ref changes"} awaiting backup`
}

function contextFor(repo: RepositorySummary): string {
  return `Local mirror: ${MIRROR_STATE[repo.mirrorState]} · GitLab: ${destinationLabel(repo)}`
}

export function describeAttention(repo: RepositorySummary): AttentionView {
  const code = repo.lastErrorCode
  const status = REPOSITORY_STATUS[repo.primaryStatus]
  const context = contextFor(repo)

  // Credentials first: a sync cannot succeed until the connection is fixed.
  if ((code !== null && RECONNECT_CODES.has(code)) || repo.destinationState === "inaccessible") {
    const side = repo.destinationState === "inaccessible" ? "GitLab" : repo.sourceState === "unavailable" ? "GitHub" : null
    return {
      problem:
        side !== null
          ? `${side} connection needs attention. ${code !== null ? ERROR_COPY[code] : "The project is not accessible."}`
          : ERROR_COPY[code ?? "AUTH_REQUIRED"],
      context,
      visual: { label: "Connection problem", icon: "status.auth_error", tone: "danger" },
      action: { kind: "reconnect", label: side !== null ? `Reconnect ${side}` : "Reconnect" },
    }
  }
  if (code === "SOURCE_UNAVAILABLE" || repo.sourceState === "unavailable") {
    return {
      problem: ERROR_COPY.SOURCE_UNAVAILABLE,
      context,
      visual: { label: "Source unavailable", icon: "state.warning", tone: "warning" },
      action: { kind: "check_access", label: "Check access" },
    }
  }
  if (repo.primaryStatus === "blocked" || (code !== null && REVIEW_CODES.has(code))) {
    return {
      problem: code !== null ? ERROR_COPY[code] : ERROR_COPY.APPROVAL_REQUIRED,
      context,
      visual: REPOSITORY_STATUS.blocked,
      action: { kind: "review", label: "Review changes" },
    }
  }

  switch (repo.primaryStatus) {
    case "missing":
      return {
        problem: repo.mirrorState === "missing" ? "Local mirror has not been created." : "A backup copy has not been created yet.",
        context,
        visual: status,
        action: { kind: "create_local", label: "Create local copy" },
      }
    case "update":
      return { problem: pendingText(repo.pendingRefChanges), context, visual: status, action: { kind: "sync", label: "Sync now" } }
    case "failed":
      return {
        problem: code !== null ? ERROR_COPY[code] : "The last sync did not complete.",
        context,
        visual: status,
        action: { kind: "retry", label: "Retry" },
      }
    case "stale":
      return { problem: "The last scan is out of date.", context, visual: status, action: { kind: "scan", label: "Retry scan" } }
    case "running":
      return { problem: "A job is running for this repository.", context, visual: status, action: null }
    case "unknown":
    case "synced":
      return {
        problem: code !== null ? ERROR_COPY[code] : "State has not been verified yet.",
        context,
        visual: status,
        action: { kind: "details", label: "View details" },
      }
  }
}

/** Selected repositories a plain "Sync all" may queue; up-to-date, blocked and running are excluded. */
export const SYNC_ALL_INCLUDED: ReadonlySet<RepositorySummary["primaryStatus"]> = new Set([
  "update",
  "failed",
  "missing",
  "stale",
  "unknown",
])

export interface SyncAllBreakdown {
  included: RepositorySummary[]
  includedByStatus: { status: RepositorySummary["primaryStatus"]; count: number }[]
  needsReview: number
  running: number
  upToDate: number
}

export function breakdownSyncAll(items: RepositorySummary[]): SyncAllBreakdown {
  const included = items.filter((item) => SYNC_ALL_INCLUDED.has(item.primaryStatus))
  const includedByStatus = [...SYNC_ALL_INCLUDED]
    .map((status) => ({ status, count: included.filter((item) => item.primaryStatus === status).length }))
    .filter((entry) => entry.count > 0)
  return {
    included,
    includedByStatus,
    needsReview: items.filter((item) => item.primaryStatus === "blocked").length,
    running: items.filter((item) => item.primaryStatus === "running").length,
    upToDate: items.filter((item) => item.primaryStatus === "synced").length,
  }
}
