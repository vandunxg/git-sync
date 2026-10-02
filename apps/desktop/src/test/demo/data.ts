// TEST/DEMO ONLY. Deterministic fixture records for the labeled demo entry (demo.html)
// and Playwright. Never imported by production modules (TEST-NO-FAKES).

import type {
  AccountSummary,
  ActivityEntry,
  Coverage,
  Diagnostics,
  Job,
  RepositorySummary,
  Settings,
} from "@/lib/native"

export type Scenario = "attention" | "healthy" | "empty" | "onboarding" | "unsupported"

export const NOW = Date.now()
export const ago = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString()

const VERIFIED: Coverage = { gitRefs: "verified", localObjects: "verified", lfs: "not_applicable", submodules: "none" }

function repo(partial: Partial<RepositorySummary> & Pick<RepositorySummary, "id" | "fullName">): RepositorySummary {
  return {
    providerRepositoryId: `gh-${partial.id}`,
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
    observedAt: ago(2),
    verifiedAt: ago(2),
    lastErrorCode: null,
    coverage: VERIFIED,
    ...partial,
  }
}

const NAMED: RepositorySummary[] = [
  repo({ id: "r-javaguide", fullName: "vandunxg/JavaGuide", observedAt: ago(2), verifiedAt: ago(2) }),
  repo({ id: "r-learn-postgresql", fullName: "vandunxg/learn-postgresql", isPrivate: true, observedAt: ago(60), verifiedAt: ago(60) }),
  repo({
    id: "r-effective-java",
    fullName: "vandunxg/effective-java",
    destinationState: "behind",
    primaryStatus: "update",
    pendingRefChanges: 3,
    sourceState: "changed",
    observedAt: ago(180),
    verifiedAt: ago(26 * 60),
  }),
  repo({
    id: "r-file-processing",
    fullName: "vandunxg/file_processing",
    isPrivate: true,
    destinationState: "inaccessible",
    primaryStatus: "failed",
    pendingRefChanges: 3,
    lastErrorCode: "AUTH_REQUIRED",
    observedAt: ago(28),
    verifiedAt: ago(180),
    workspaceState: "dirty",
    coverage: { gitRefs: "verified", localObjects: "verified", lfs: "local_verified", submodules: "pointers_only" },
  }),
  repo({
    id: "r-advanced-java",
    fullName: "vandunxg/advanced-java",
    mirrorState: "missing",
    destinationState: "missing",
    primaryStatus: "missing",
    pendingRefChanges: null,
    observedAt: ago(24 * 60),
    verifiedAt: null,
    coverage: { gitRefs: "not_checked", localObjects: "not_checked", lfs: "not_checked", submodules: "none" },
  }),
  repo({ id: "r-spring-boot", fullName: "vandunxg/spring-boot", isFork: true, observedAt: ago(2 * 24 * 60), verifiedAt: ago(2 * 24 * 60) }),
  repo({
    id: "r-rewrite",
    fullName: "acme-platform/infrastructure-terraform-modules-with-a-deliberately-long-repository-name-for-truncation-checks",
    isPrivate: true,
    primaryStatus: "blocked",
    destinationState: "diverged",
    pendingRefChanges: 2,
    lastErrorCode: "APPROVAL_REQUIRED",
    observedAt: ago(45),
    verifiedAt: ago(3 * 24 * 60),
  }),
  repo({
    id: "r-stale",
    fullName: "người-dùng/kho-mã-tiếng-việt",
    primaryStatus: "stale",
    sourceState: "stale",
    observedAt: ago(9 * 24 * 60),
    verifiedAt: ago(9 * 24 * 60),
  }),
  repo({
    id: "r-empty",
    fullName: "vandunxg/empty-repo",
    defaultBranch: null,
    primaryStatus: "unknown",
    destinationState: "unknown",
    mirrorState: "unknown",
    pendingRefChanges: null,
    observedAt: null,
    verifiedAt: null,
    coverage: { gitRefs: "not_checked", localObjects: "not_checked", lfs: "not_checked", submodules: "none" },
  }),
]

const WORDS = ["api", "web", "docs", "infra", "mobile", "sdk", "cli", "worker", "auth", "billing", "search", "notes"]
const SUFFIX = ["service", "client", "toolkit", "playground", "lab", "kit", "core", "app"]

export function buildRepositories(scenario: Scenario): RepositorySummary[] {
  if (scenario === "empty" || scenario === "onboarding") return []
  const generated = Array.from({ length: 119 }, (_, index) =>
    repo({
      id: `r-gen-${index}`,
      fullName: `${index % 4 === 0 ? "acme-platform" : "vandunxg"}/${WORDS[index % WORDS.length]}-${SUFFIX[index % SUFFIX.length]}-${index}`,
      isPrivate: index % 3 === 0,
      selected: index % 17 !== 0,
      observedAt: ago(5 + index * 37),
      verifiedAt: ago(5 + index * 37),
    }),
  )
  const all = [...NAMED, ...generated]
  if (scenario === "healthy") {
    return all.map((item) => ({
      ...item,
      primaryStatus: "synced",
      destinationState: "current",
      mirrorState: "current",
      sourceState: "current",
      lastErrorCode: null,
      pendingRefChanges: 0,
      workspaceState: "disabled",
      defaultBranch: item.defaultBranch ?? "main",
      verifiedAt: item.verifiedAt ?? ago(30),
      observedAt: item.observedAt ?? ago(30),
      coverage: VERIFIED,
    }))
  }
  return all
}

export function buildAccounts(scenario: Scenario): AccountSummary[] {
  if (scenario === "empty" || scenario === "onboarding") return []
  return [
    {
      id: "acc-github",
      provider: "github",
      username: "vandunxg",
      authState: "connected",
      storageMode: "keyring",
      expiresAt: null,
      capabilitySummary: ["Read repository metadata", "Read contents of 127 repositories", "Organization repositories: acme-platform"],
      verifiedAt: ago(12),
    },
    {
      id: "acc-gitlab",
      provider: "gitlab",
      username: "vandunxg",
      authState: scenario === "healthy" ? "connected" : "expired",
      storageMode: "keyring",
      expiresAt: scenario === "healthy" ? new Date(NOW + 40 * 86_400_000).toISOString() : ago(35),
      capabilitySummary: ["api", "read_repository", "write_repository"],
      verifiedAt: ago(35),
    },
  ]
}

export function buildJobs(scenario: Scenario): Job[] {
  if (scenario !== "attention") return []
  const base = { coverage: VERIFIED, accountId: "acc-github", planId: null, error: null, attempt: 1, revision: 1, etaSeconds: null }
  return [
    { ...base, id: "job-101", repositoryId: "r-javaguide", kind: "sync", state: "running", stage: "push", progressPercent: 42, transferredBytes: 48_234_496, totalBytes: 114_843_648, etaSeconds: 130, planId: "plan-javaguide" },
    { ...base, id: "job-102", repositoryId: "r-learn-postgresql", kind: "sync", state: "running", stage: "fetch", progressPercent: null, transferredBytes: 12_582_912, totalBytes: null },
    { ...base, id: "job-103", repositoryId: "r-rewrite", kind: "plan", state: "waiting_for_approval", stage: "plan", progressPercent: null, transferredBytes: null, totalBytes: null, planId: "plan-rewrite" },
    { ...base, id: "job-104", repositoryId: "r-effective-java", kind: "sync", state: "queued", stage: "preflight", progressPercent: null, transferredBytes: null, totalBytes: null },
    { ...base, id: "job-105", repositoryId: "r-advanced-java", kind: "sync", state: "queued", stage: "preflight", progressPercent: null, transferredBytes: null, totalBytes: null },
    {
      ...base,
      id: "job-099",
      repositoryId: "r-file-processing",
      kind: "sync",
      state: "failed",
      stage: "push",
      progressPercent: null,
      transferredBytes: null,
      totalBytes: null,
      attempt: 2,
      error: {
        code: "AUTH_REQUIRED",
        message: "GitLab rejected the stored token (401).",
        retryable: false,
        retryAt: null,
        correlationId: "c0ffee00-1111-4222-8333-944445555666",
        details: "remote: HTTP Basic: Access denied. The provided password or token is incorrect or it's expired.",
      },
    },
    { ...base, id: "job-098", repositoryId: "r-spring-boot", kind: "sync", state: "succeeded", stage: "finalize", progressPercent: 100, transferredBytes: 2_097_152, totalBytes: 2_097_152 },
    { ...base, id: "job-097", repositoryId: null, kind: "scan", state: "succeeded", stage: "finalize", progressPercent: null, transferredBytes: null, totalBytes: null },
  ]
}

export function buildActivity(scenario: Scenario): ActivityEntry[] {
  if (scenario === "empty" || scenario === "onboarding") return []
  const entries: ActivityEntry[] = [
    { id: "ev-1", repositoryId: "r-javaguide", jobId: "job-101", occurredAt: ago(4), severity: "info", type: "sync", message: "JavaGuide · Sync started — mirroring 3 ref changes", errorCode: null },
    { id: "ev-2", repositoryId: "r-file-processing", jobId: "job-099", occurredAt: ago(28), severity: "error", type: "sync", message: "file_processing · Sync failed — GitLab authentication expired", errorCode: "AUTH_REQUIRED" },
    { id: "ev-3", repositoryId: "r-effective-java", jobId: null, occurredAt: ago(52), severity: "info", type: "project", message: "effective-java · GitLab project created (private)", errorCode: null },
    { id: "ev-4", repositoryId: null, jobId: null, occurredAt: ago(71), severity: "warning", type: "auth", message: "GitLab · Token expired; backups to GitLab are paused", errorCode: "AUTH_REQUIRED" },
    { id: "ev-5", repositoryId: "r-advanced-java", jobId: "job-097", occurredAt: ago(112), severity: "info", type: "scan", message: "Scan completed — 128 repositories, 1 new", errorCode: null },
    { id: "ev-6", repositoryId: "r-spring-boot", jobId: "job-098", occurredAt: ago(26 * 60), severity: "info", type: "sync", message: "spring-boot · Sync completed — 2 ref changes verified", errorCode: null },
    { id: "ev-7", repositoryId: "r-rewrite", jobId: "job-103", occurredAt: ago(27 * 60), severity: "warning", type: "sync", message: "infrastructure-terraform-modules… · Plan changes existing history; review required", errorCode: "APPROVAL_REQUIRED" },
    { id: "ev-8", repositoryId: null, jobId: null, occurredAt: ago(3 * 24 * 60), severity: "info", type: "system", message: "Application started; keyring available", errorCode: null },
  ]
  return scenario === "healthy" ? entries.filter((entry) => entry.severity === "info") : entries
}

export const SETTINGS: Settings = {
  scanIntervalMinutes: 30,
  autoScan: true,
  autoSync: false,
  maxConcurrentJobs: 3,
  includePrivate: true,
  includeForks: false,
  includeArchived: false,
  includeCollaborations: false,
  includeOrganizations: true,
  workspaceEnabled: false,
  notifyErrors: true,
  notifySuccess: false,
  startMinimized: false,
  closeBehavior: "ask",
  theme: "light",
  retentionCleanup: "manual",
  telemetry: false,
}

export function buildDiagnostics(): Diagnostics {
  return {
    observedAt: ago(1),
    items: [
      { key: "git", status: "available", detail: "git version 2.54.0" },
      { key: "git-lfs", status: "missing", detail: "git-lfs was not found on PATH; LFS payloads cannot be backed up." },
      { key: "keyring", status: "available", detail: "Secret Service (gnome-keyring) unlocked" },
      { key: "storage-root", status: "available", detail: "~/GitBackups · writable · 412 GB free" },
      { key: "portal", status: "available", detail: "xdg-desktop-portal file chooser" },
      { key: "tray", status: "unsupported", detail: "No StatusNotifier host in this session; close-to-tray is unavailable." },
    ],
  }
}

export const LOG_LINES = [
  "[preflight] git 2.54.0 · mirror ~/GitBackups/.mirrors/vandunxg/file_processing.git",
  "[observe_source] 14 refs observed on GitHub (3 changed)",
  "[fetch] fetched refs/heads/main 4f1c2a9e..9b7d3e10",
  "[plan] 3 fast-forward updates, 0 rewrites",
  "[push] POST https://gitlab.com/vandunxg/file_processing.git/git-receive-pack",
  "[push] remote: HTTP Basic: Access denied. token=[REDACTED]",
  "<script>alert('provider error payloads are shown as text')</script>",
  "\u001b[31mANSI sequences are displayed literally, never interpreted\u001b[0m",
  "[push] error: failed to push some refs (AUTH_REQUIRED)",
]
