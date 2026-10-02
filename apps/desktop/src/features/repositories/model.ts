// Pure view helpers for the repositories feature. Every label maps a contract value;
// nothing here derives backup health beyond what the native DTO states.

import type { StatusDescriptor } from "@/components/shared"
import type { Coverage, RefEntry, RefList, RefOperation, RepoListRequest, RepositorySummary } from "@/lib/native"

export type RepoStatus = RepositorySummary["primaryStatus"]
export type Visibility = RepoListRequest["visibility"]
export type SortOrder = RepoListRequest["sort"]

export const PAGE_SIZE = 25
/** V1 selection is explicit IDs in bounded batches (UI-04). */
export const MAX_SELECTION = 200

export const VISIBILITY_OPTIONS: { value: Visibility; label: string }[] = [
  { value: "all", label: "All visibility" },
  { value: "private", label: "Private" },
  { value: "public", label: "Public" },
]

export const SORT_OPTIONS: { value: SortOrder; label: string }[] = [
  { value: "updated_desc", label: "Recently observed" },
  { value: "name_asc", label: "Name A–Z" },
]

const STATUSES: readonly RepoStatus[] = ["synced", "update", "failed", "missing", "blocked", "running", "unknown", "stale"]

export interface ListFilters {
  search: string
  status: RepoStatus | null
  visibility: Visibility
  sort: SortOrder
  selectedOnly: boolean
}

export function parseFilters(params: URLSearchParams): ListFilters {
  const status = params.get("status")
  const visibility = params.get("visibility")
  const sort = params.get("sort")
  return {
    search: params.get("q") ?? "",
    status: STATUSES.find((value) => value === status) ?? null,
    visibility: visibility === "private" || visibility === "public" ? visibility : "all",
    sort: sort === "name_asc" ? "name_asc" : "updated_desc",
    selectedOnly: params.get("included") === "1",
  }
}

export function hasActiveFilters(filters: ListFilters): boolean {
  return filters.search.trim() !== "" || filters.status !== null || filters.visibility !== "all" || filters.selectedOnly
}

// --- Coverage: four separate dimensions, never merged into one "healthy" claim.

type Chip = StatusDescriptor

const GIT_REFS: Record<Coverage["gitRefs"], Chip> = {
  verified: { label: "Git refs verified", icon: "status.synced", tone: "success" },
  failed: { label: "Git refs failed", icon: "status.failed", tone: "danger" },
  not_checked: { label: "Git refs not checked", icon: "status.unknown", tone: "neutral" },
}

const LOCAL_OBJECTS: Record<Coverage["localObjects"], Chip> = {
  verified: { label: "Local objects verified", icon: "status.synced", tone: "success" },
  failed: { label: "Local objects failed", icon: "status.failed", tone: "danger" },
  not_checked: { label: "Local objects not checked", icon: "status.unknown", tone: "neutral" },
}

const LFS: Record<Coverage["lfs"], Chip> = {
  not_applicable: { label: "No LFS", icon: "metadata.lfs", tone: "neutral" },
  not_checked: { label: "LFS not checked", icon: "metadata.lfs", tone: "neutral" },
  local_verified: { label: "LFS verified locally", icon: "metadata.lfs", tone: "info" },
  uploaded: { label: "LFS uploaded", icon: "metadata.lfs", tone: "success" },
  restore_verified: { label: "LFS restore verified", icon: "metadata.lfs", tone: "success" },
  incomplete: { label: "LFS incomplete", icon: "state.warning", tone: "warning" },
}

const SUBMODULES: Record<Coverage["submodules"], Chip> = {
  none: { label: "No submodules", icon: "status.no_changes", tone: "neutral" },
  pointers_only: { label: "Submodule pointers only", icon: "state.warning", tone: "warning" },
  separately_covered: { label: "Submodules covered", icon: "status.synced", tone: "success" },
  incomplete: { label: "Submodules incomplete", icon: "state.warning", tone: "warning" },
}

export function coverageChips(coverage: Coverage): Chip[] {
  return [GIT_REFS[coverage.gitRefs], LOCAL_OBJECTS[coverage.localObjects], LFS[coverage.lfs], SUBMODULES[coverage.submodules]]
}

export const SOURCE_STATE: Record<RepositorySummary["sourceState"], string> = {
  current: "Current",
  changed: "Changed since last sync",
  unavailable: "Unavailable",
  stale: "Not observed recently",
  unknown: "Unknown",
}

// --- Ref comparison: purely set comparison of observed names/OIDs.

export type RefComparison = "same" | "differs" | "missing_destination" | "only_destination"

export interface RefRow {
  name: string
  sourceOid: string | null
  destinationOid: string | null
  comparison: RefComparison
}

export const REF_COMPARISON: Record<RefComparison, StatusDescriptor> = {
  same: { label: "Same", icon: "status.synced", tone: "success" },
  differs: { label: "Differs", icon: "status.update", tone: "warning" },
  missing_destination: { label: "Missing on GitLab", icon: "status.local_missing", tone: "warning" },
  only_destination: { label: "Only on GitLab", icon: "state.warning", tone: "neutral" },
}

export function compareRefs(list: Pick<RefList, "source" | "destination">): RefRow[] {
  const destination = new Map<string, RefEntry>(list.destination.map((entry) => [entry.name, entry]))
  const rows: RefRow[] = list.source.map((entry) => {
    const other = destination.get(entry.name)
    destination.delete(entry.name)
    return {
      name: entry.name,
      sourceOid: entry.oid,
      destinationOid: other?.oid ?? null,
      comparison: other === undefined ? "missing_destination" : other.oid === entry.oid ? "same" : "differs",
    }
  })
  for (const entry of destination.values()) {
    rows.push({ name: entry.name, sourceOid: null, destinationOid: entry.oid, comparison: "only_destination" })
  }
  return rows
}

// --- Plan operations.

export const OPERATION_KIND: Record<RefOperation["kind"], StatusDescriptor & { destructive: boolean }> = {
  create: { label: "Create", icon: "event.created", tone: "info", destructive: false },
  fast_forward: { label: "Fast-forward", icon: "status.update", tone: "success", destructive: false },
  rewrite: { label: "Rewrite history", icon: "status.blocked", tone: "danger", destructive: true },
  move_tag: { label: "Move tag", icon: "status.blocked", tone: "danger", destructive: true },
  delete_ref: { label: "Delete ref", icon: "status.blocked", tone: "danger", destructive: true },
}

// --- Primary action for a repository, chosen from its native state only.

export type PrimaryAction = "reconnect" | "review" | "retry" | "sync" | "running"

export function primaryActionFor(repo: Pick<RepositorySummary, "primaryStatus" | "lastErrorCode">): PrimaryAction {
  if (repo.lastErrorCode === "AUTH_REQUIRED") return "reconnect"
  switch (repo.primaryStatus) {
    case "blocked":
      return "review"
    case "running":
      return "running"
    case "failed":
      return "retry"
    default:
      return "sync"
  }
}

export function workspaceOpenBlocker(repo: Pick<RepositorySummary, "workspaceState">): string | null {
  if (repo.workspaceState === "disabled") return "Workspace checkouts are turned off in Settings."
  if (repo.workspaceState === "missing") return "No workspace folder exists for this repository yet."
  return null
}

export function gitlabOpenBlocker(repo: Pick<RepositorySummary, "destinationState">): string | null {
  return repo.destinationState === "missing" ? "The GitLab project has not been created yet." : null
}

/** Whether a native expiry timestamp has passed, checked when the view renders. */
export function isExpired(iso: string): boolean {
  const time = Date.parse(iso)
  return Number.isNaN(time) || time <= Date.now()
}
