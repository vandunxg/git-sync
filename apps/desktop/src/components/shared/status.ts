// Single source of status presentation. Business dimensions stay separate: a
// repository's primary status, a job's state and an account's auth state never merge.

import type { AccountSummary, ActivityEntry, Diagnostic, Job, RepositorySummary } from "@/lib/native"

import type { StatusDescriptor } from "./StatusBadge.tsx"

export const REPOSITORY_STATUS: Record<RepositorySummary["primaryStatus"], StatusDescriptor> = {
  synced: { label: "Synced", icon: "status.synced", tone: "success" },
  update: { label: "Update pending", icon: "status.update", tone: "warning" },
  failed: { label: "Failed", icon: "status.failed", tone: "danger" },
  missing: { label: "Local missing", icon: "status.local_missing", tone: "neutral" },
  blocked: { label: "Needs review", icon: "status.blocked", tone: "danger" },
  running: { label: "Running", icon: "status.running", tone: "info", spinning: true },
  unknown: { label: "Unknown", icon: "status.unknown", tone: "neutral" },
  stale: { label: "Stale", icon: "status.stale", tone: "warning" },
}

export const REPOSITORY_STATUS_ORDER = Object.keys(REPOSITORY_STATUS) as RepositorySummary["primaryStatus"][]

export const MIRROR_STATE: Record<RepositorySummary["mirrorState"], string> = {
  missing: "Missing",
  staging: "Creating",
  current: "Ready",
  invalid: "Invalid",
  unknown: "Unknown",
}

export function destinationLabel(repo: Pick<RepositorySummary, "destinationState" | "pendingRefChanges">): string {
  switch (repo.destinationState) {
    case "missing":
      return "Not created"
    case "current":
      return "Ready"
    case "behind":
      return repo.pendingRefChanges === null ? "Behind" : `Behind ${repo.pendingRefChanges} ${repo.pendingRefChanges === 1 ? "ref" : "refs"}`
    case "diverged":
      return "Diverged"
    case "inaccessible":
      return "No access"
    case "unknown":
      return "Unknown"
  }
}

export const WORKSPACE_STATE: Record<RepositorySummary["workspaceState"], string> = {
  disabled: "Disabled",
  current: "Current",
  behind: "Behind",
  dirty: "Local edits",
  ahead: "Ahead",
  detached: "Detached",
  diverged: "Diverged",
  missing: "Missing",
}

export const JOB_STATE: Record<Job["state"], StatusDescriptor> = {
  queued: { label: "Queued", icon: "status.waiting", tone: "neutral" },
  running: { label: "Running", icon: "status.running", tone: "info", spinning: true },
  waiting_for_approval: { label: "Needs review", icon: "status.blocked", tone: "warning" },
  waiting_for_retry: { label: "Retry scheduled", icon: "status.waiting", tone: "warning" },
  cancel_requested: { label: "Canceling", icon: "status.waiting", tone: "warning" },
  succeeded: { label: "Succeeded", icon: "status.synced", tone: "success" },
  partial: { label: "Partial", icon: "state.warning", tone: "warning" },
  failed: { label: "Failed", icon: "status.failed", tone: "danger" },
  canceled: { label: "Canceled", icon: "status.no_changes", tone: "neutral" },
  interrupted: { label: "Interrupted", icon: "state.warning", tone: "warning" },
}

export const ACTIVE_JOB_STATES: ReadonlySet<Job["state"]> = new Set([
  "queued",
  "running",
  "waiting_for_approval",
  "waiting_for_retry",
  "cancel_requested",
])

export const JOB_STAGE: Record<Job["stage"], string> = {
  preflight: "Preflight checks",
  observe_source: "Reading GitHub refs",
  fetch: "Fetching from GitHub",
  plan: "Planning ref updates",
  snapshot: "Recovery snapshot",
  transfer_lfs: "Transferring LFS",
  push: "Mirroring to GitLab",
  verify: "Verifying backup",
  workspace: "Updating workspace",
  finalize: "Finalizing",
}

export const JOB_STAGE_ORDER = Object.keys(JOB_STAGE) as Job["stage"][]

export const JOB_KIND: Record<Job["kind"], string> = {
  scan: "Repository scan",
  plan: "Sync plan",
  sync: "Sync",
  restore_test: "Restore test",
}

export const AUTH_STATE: Record<AccountSummary["authState"], StatusDescriptor> = {
  connected: { label: "Connected", icon: "status.connected", tone: "success" },
  expired: { label: "Token expired", icon: "status.auth_error", tone: "danger" },
  forbidden: { label: "Access denied", icon: "status.blocked", tone: "danger" },
  disconnected: { label: "Disconnected", icon: "status.no_changes", tone: "neutral" },
  unknown: { label: "Not verified", icon: "status.unknown", tone: "neutral" },
}

export const STORAGE_MODE: Record<AccountSummary["storageMode"], string> = {
  keyring: "Stored in the system keyring",
  session_only: "Session only — cleared when the app quits",
  none: "No credential stored",
}

export const DIAGNOSTIC_STATUS: Record<Diagnostic["status"], StatusDescriptor> = {
  available: { label: "Available", icon: "status.ready", tone: "success" },
  missing: { label: "Missing", icon: "status.failed", tone: "danger" },
  locked: { label: "Locked", icon: "status.auth_error", tone: "warning" },
  unsupported: { label: "Unsupported", icon: "status.blocked", tone: "neutral" },
  unknown: { label: "Unknown", icon: "status.unknown", tone: "neutral" },
}

export const SEVERITY: Record<ActivityEntry["severity"], StatusDescriptor> = {
  info: { label: "Info", icon: "status.ready", tone: "neutral" },
  warning: { label: "Warning", icon: "state.warning", tone: "warning" },
  error: { label: "Error", icon: "state.error", tone: "danger" },
}

/** Semantic icon/tone per activity type, overridden by severity for problems. */
export function activityVisual(entry: Pick<ActivityEntry, "type" | "severity">): StatusDescriptor {
  if (entry.severity === "error") return { label: "Error", icon: "status.failed", tone: "danger" }
  if (entry.severity === "warning") return { label: "Warning", icon: entry.type === "auth" ? "event.auth" : "state.warning", tone: "warning" }
  switch (entry.type) {
    case "scan":
      return { label: "Scan", icon: "event.scan", tone: "neutral" }
    case "sync":
      return { label: "Sync", icon: "status.synced", tone: "success" }
    case "auth":
      return { label: "Account", icon: "event.auth", tone: "neutral" }
    case "project":
      return { label: "Project", icon: "event.created", tone: "success" }
    case "system":
      return { label: "System", icon: "setting.advanced", tone: "neutral" }
  }
}

export const ACTIVITY_TYPE_LABEL: Record<ActivityEntry["type"], string> = {
  scan: "Scan",
  sync: "Sync",
  auth: "Account",
  project: "Project",
  system: "System",
}
