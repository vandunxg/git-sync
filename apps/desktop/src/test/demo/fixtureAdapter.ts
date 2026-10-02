// TEST/DEMO ONLY: an in-memory NativePort labeled `kind: "fixture"`. It mirrors the
// contract shapes so screens can be reviewed before the native commands exist. It does
// not simulate time-based progress; state changes only in response to user commands.

import type { NativePort } from "@/lib/api"
import type { AppError, CommandMap, CommandName, CommandResult, Job, RepositorySummary, SyncPlan } from "@/lib/native"

import {
  ago,
  buildAccounts,
  buildActivity,
  buildDiagnostics,
  buildJobs,
  buildRepositories,
  LOG_LINES,
  NOW,
  SETTINGS,
  type Scenario,
} from "./data.ts"

type Handler<C extends CommandName> = (args: CommandMap[C]["request"]) => CommandMap[C]["response"]
type Handlers = { [C in CommandName]: Handler<C> }

class Failure extends Error {
  readonly error: AppError
  constructor(error: AppError) {
    super(error.message)
    this.error = error
  }
}

let counter = 0
const nextId = (prefix: string) => `${prefix}-${(counter += 1).toString().padStart(4, "0")}`

function fail(code: AppError["code"], message: string, details: string | null = null): never {
  throw new Failure({ code, message, retryable: code === "NETWORK_UNAVAILABLE" || code === "TIMEOUT", retryAt: null, correlationId: nextId("corr"), details })
}

export function createFixturePort(scenario: Scenario): NativePort {
  let revision = 1
  const bump = () => (revision += 1)
  let onboardingComplete = scenario !== "onboarding"
  let repositories = buildRepositories(scenario)
  let accounts = buildAccounts(scenario)
  let jobs = buildJobs(scenario)
  const activity = buildActivity(scenario)
  let settings = { ...SETTINGS }
  let settingsRevision = 4
  const plans = new Map<string, SyncPlan>()
  const approvals = new Map<string, string>()

  const findRepo = (id: string) => repositories.find((item) => item.id === id) ?? fail("SOURCE_UNAVAILABLE", `Repository ${id} is not known.`)
  const findJob = (id: string) => jobs.find((item) => item.id === id) ?? fail("INTERNAL", `Job ${id} is not known.`)
  const patchRepo = (id: string, patch: Partial<RepositorySummary>) => {
    repositories = repositories.map((item) => (item.id === id ? { ...item, ...patch } : item))
  }

  function planFor(repositoryId: string): SyncPlan {
    const rewrite = repositoryId === "r-rewrite"
    const plan: SyncPlan = {
      id: rewrite ? "plan-rewrite" : nextId("plan"),
      repositoryId,
      sourceSnapshotId: nextId("snap"),
      sourceDigest: "sha256:5f0c8a6d0b31",
      destinationMappingId: `map-${repositoryId}`,
      destinationDigest: "sha256:91ac44e2d7f0",
      policyRevision: 7,
      planDigest: `sha256:${repositoryId.length.toString(16)}e7b9c02f4a1d`,
      operations: rewrite
        ? [
            { kind: "rewrite", refName: "refs/heads/main", expectedDestinationOid: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678", sourceOid: "9f8e7d6c5b4a39281706f5e4d3c2b1a098765432" },
            { kind: "move_tag", refName: "refs/tags/v2.3.0", expectedDestinationOid: "1111111111111111111111111111111111111111", sourceOid: "2222222222222222222222222222222222222222" },
            { kind: "fast_forward", refName: "refs/heads/release", expectedDestinationOid: "3333333333333333333333333333333333333333", sourceOid: "4444444444444444444444444444444444444444" },
          ]
        : [{ kind: "fast_forward", refName: "refs/heads/main", expectedDestinationOid: "4f1c2a9e00000000000000000000000000000000", sourceOid: "9b7d3e1000000000000000000000000000000000" }],
      requiresApproval: rewrite,
      preChangeSnapshotRequired: rewrite,
      expiresAt: new Date(NOW + 30 * 60_000).toISOString(),
    }
    plans.set(plan.id, plan)
    return plan
  }
  planFor("r-rewrite")

  function queueJob(partial: Pick<Job, "kind" | "repositoryId"> & Partial<Job>): Job {
    const job: Job = {
      id: nextId("job"),
      attempt: 1,
      state: "queued",
      stage: "preflight",
      progressPercent: null,
      transferredBytes: null,
      totalBytes: null,
      etaSeconds: null,
      error: null,
      coverage: { gitRefs: "not_checked", localObjects: "not_checked", lfs: "not_checked", submodules: "none" },
      revision: bump(),
      accountId: "acc-github",
      planId: null,
      ...partial,
    }
    jobs = [job, ...jobs]
    return job
  }

  const dashboard = () => {
    const selected = repositories.filter((item) => item.selected)
    const upToDate = selected.filter((item) => item.primaryStatus === "synced").length
    const attentionItems = selected.filter((item) => ["failed", "blocked", "missing", "update", "stale"].includes(item.primaryStatus))
    return {
      tracked: selected.length,
      upToDate,
      needsAttention: attentionItems.length,
      other: selected.length - upToDate - attentionItems.length,
      discoveryComplete: true,
      attentionItems: attentionItems.slice(0, 6),
      observedAt: selected.length > 0 ? ago(2) : null,
      revision,
    }
  }

  const handlers: Handlers = {
    gbm_app_bootstrap: () => ({ onboardingComplete, accounts, settings, diagnostics: buildDiagnostics(), revision }),
    gbm_accounts_list: () => ({ items: accounts, revision }),
    gbm_credentials_validate_store: ({ provider, token, storageMode }) => {
      const trimmed = token.trim()
      if (trimmed.length < 8) fail("AUTH_REQUIRED", "The token was rejected by the provider.")
      if (trimmed.includes("readonly") && provider === "gitlab") fail("ACCESS_DENIED", "The token lacks the write_repository scope.")
      const account = {
        id: provider === "github" ? "acc-github" : "acc-gitlab",
        provider,
        username: "vandunxg",
        authState: "connected" as const,
        storageMode,
        expiresAt: null,
        capabilitySummary: provider === "github" ? ["Read repository metadata", "Read contents of 127 repositories"] : ["api", "read_repository", "write_repository"],
        verifiedAt: new Date().toISOString(),
      }
      accounts = [...accounts.filter((item) => item.provider !== provider), account]
      if (provider === "gitlab") {
        repositories = repositories.map((item) => (item.lastErrorCode === "AUTH_REQUIRED" ? { ...item, lastErrorCode: null, destinationState: "behind", primaryStatus: "update" } : item))
      }
      return { account, revision: bump() }
    },
    gbm_accounts_disconnect: ({ accountId }) => {
      accounts = accounts.map((item) => (item.id === accountId ? { ...item, authState: "disconnected", storageMode: "none", verifiedAt: null } : item))
      return { revision: bump() }
    },
    gbm_storage_pick_root: () => ({ outcome: "selected", candidate: { candidateId: "cand-1", displayPath: "~/GitBackups", valid: true, errors: [] } }),
    gbm_storage_commit_root: ({ workspaceEnabled }) => ({ id: "root-1", displayPath: "~/GitBackups", ownershipVerified: true, availableBytes: 442_381_000_000, workspaceEnabled }),
    gbm_repositories_list: ({ cursor, limit, search, status, visibility, selectedOnly, sort }) => {
      const needle = search.trim().toLowerCase()
      let items = repositories.filter(
        (item) =>
          (!needle || item.fullName.toLowerCase().includes(needle)) &&
          (status === null || item.primaryStatus === status) &&
          (visibility === "all" || (visibility === "private") === item.isPrivate) &&
          (!selectedOnly || item.selected),
      )
      items = [...items].sort((a, b) =>
        sort === "name_asc" ? a.fullName.localeCompare(b.fullName) : (b.observedAt ?? "").localeCompare(a.observedAt ?? ""),
      )
      const start = cursor === null ? 0 : Number(cursor)
      const page = items.slice(start, start + limit)
      const next = start + limit < items.length ? String(start + limit) : null
      return { items: page, nextCursor: next, totalCount: items.length, discoveryComplete: true, revision }
    },
    gbm_repository_get: ({ repositoryId }) => findRepo(repositoryId),
    gbm_repositories_select: ({ repositoryIds, selected }) => {
      repositories = repositories.map((item) => (repositoryIds.includes(item.id) ? { ...item, selected } : item))
      return { revision: bump() }
    },
    gbm_source_scan_enqueue: ({ accountId }) => {
      const job = queueJob({ kind: "scan", repositoryId: null, accountId })
      return { jobId: job.id, state: "queued", revision }
    },
    gbm_namespaces_list: () => ({
      items: [
        { id: "ns-user", fullPath: "vandunxg", canCreateProject: true },
        { id: "ns-group", fullPath: "vandunxg-backups", canCreateProject: true },
        { id: "ns-org", fullPath: "acme-platform/mirrors", canCreateProject: null },
      ],
      nextCursor: null,
    }),
    gbm_destination_preview: ({ repositoryIds, namespaceId }) => {
      const prefix = namespaceId === "ns-org" ? "acme-platform/mirrors" : namespaceId === "ns-group" ? "vandunxg-backups" : "vandunxg"
      const ids = repositoryIds.length > 0 ? repositoryIds : ["r-javaguide", "r-learn-postgresql", "r-effective-java", "r-file-processing"]
      return {
        previewId: nextId("preview"),
        digest: "sha256:preview",
        expiresAt: new Date(Date.now() + 15 * 60_000).toISOString(),
        items: ids.map((id, index) => ({
          repositoryId: id,
          intendedPath: `${prefix}/${id.replace(/^r-/, "")}`,
          existingProjectId: index === 1 ? "gl-8812" : null,
          needsCreation: index !== 1,
          canAdopt: index === 1,
          warnings: index === 1 ? ["A project already exists at this path; it will be adopted only after you confirm it is a backup of this repository."] : [],
        })),
      }
    },
    gbm_destination_commit: () => {
      onboardingComplete = true
      if (repositories.length === 0) repositories = buildRepositories("healthy").slice(0, 12)
      if (accounts.length === 0) accounts = buildAccounts("healthy")
      return { revision: bump() }
    },
    gbm_sync_plan_create: ({ repositoryId }) => {
      const repository = findRepo(repositoryId)
      if (repository.lastErrorCode === "AUTH_REQUIRED") fail("AUTH_REQUIRED", "GitLab authentication expired.")
      const plan = planFor(repositoryId)
      const job = queueJob({ kind: "plan", repositoryId, planId: plan.id, state: plan.requiresApproval ? "waiting_for_approval" : "queued", stage: "plan" })
      patchRepo(repositoryId, { primaryStatus: plan.requiresApproval ? "blocked" : "running" })
      return { jobId: job.id, state: "queued", revision }
    },
    gbm_sync_plan_get: ({ planId }) => plans.get(planId) ?? fail("PLAN_STALE", "This plan expired or was replaced."),
    gbm_sync_plan_approve: ({ planId, expectedPlanDigest }) => {
      const plan = plans.get(planId) ?? fail("PLAN_STALE", "This plan expired or was replaced.")
      if (plan.planDigest !== expectedPlanDigest) fail("PLAN_STALE", "The plan changed since it was reviewed.")
      const approvalId = nextId("approval")
      approvals.set(approvalId, planId)
      return { approvalId, planId, expiresAt: new Date(Date.now() + 10 * 60_000).toISOString(), revision: bump() }
    },
    gbm_sync_apply_enqueue: ({ planId, approvalId }) => {
      const plan = plans.get(planId) ?? fail("PLAN_STALE", "This plan expired or was replaced.")
      if (plan.requiresApproval && (approvalId === null || approvals.get(approvalId) !== planId)) fail("APPROVAL_REQUIRED", "Approve this plan before applying it.")
      jobs = jobs.map((job) => (job.planId === planId && job.state === "waiting_for_approval" ? { ...job, state: "succeeded", stage: "finalize" } : job))
      const job = queueJob({ kind: "sync", repositoryId: plan.repositoryId, planId })
      patchRepo(plan.repositoryId, { primaryStatus: "running", lastErrorCode: null })
      return { jobId: job.id, state: "queued", revision }
    },
    gbm_jobs_list: ({ cursor, limit }) => {
      const start = cursor === null ? 0 : Number(cursor)
      return { items: jobs.slice(start, start + limit), nextCursor: start + limit < jobs.length ? String(start + limit) : null, revision }
    },
    gbm_job_cancel: ({ jobId }) => {
      const job = findJob(jobId)
      const updated: Job = { ...job, state: job.state === "queued" ? "canceled" : "cancel_requested", revision: bump() }
      jobs = jobs.map((item) => (item.id === jobId ? updated : item))
      return updated
    },
    gbm_job_retry: ({ jobId }) => {
      const job = findJob(jobId)
      if (job.error?.code === "AUTH_REQUIRED" && accounts.some((item) => item.provider === "gitlab" && item.authState !== "connected")) {
        fail("AUTH_REQUIRED", "Reconnect GitLab before retrying.")
      }
      const retry = queueJob({ kind: job.kind, repositoryId: job.repositoryId, attempt: job.attempt + 1 })
      return { jobId: retry.id, state: "queued", revision }
    },
    gbm_queue_set_paused: () => ({ revision: bump() }),
    gbm_dashboard_get: dashboard,
    gbm_repository_refs: ({ repositoryId }) => {
      findRepo(repositoryId)
      const source = [
        { name: "refs/heads/main", oid: "9b7d3e10c4a2f1e0d9c8b7a6f5e4d3c2b1a09876" },
        { name: "refs/heads/develop", oid: "77aa12bc34de56f078901234abcdef5678901234" },
        { name: "refs/heads/feature/streaming-parser", oid: "5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f" },
        { name: "refs/tags/v1.4.0", oid: "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567" },
      ]
      const destination = [
        { name: "refs/heads/main", oid: "4f1c2a9e7b6d5c4b3a2918f7e6d5c4b3a2918f7e" },
        { name: "refs/heads/develop", oid: "77aa12bc34de56f078901234abcdef5678901234" },
        { name: "refs/tags/v1.4.0", oid: "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567" },
      ]
      return { repositoryId, source, destination, observationComplete: true, observedAt: ago(28), revision }
    },
    gbm_activity_list: ({ cursor, limit, repositoryId, severity, search }) => {
      const needle = search.trim().toLowerCase()
      const items = activity.filter(
        (entry) =>
          (repositoryId === null || entry.repositoryId === repositoryId) &&
          (severity === "all" || entry.severity === severity) &&
          (!needle || entry.message.toLowerCase().includes(needle)),
      )
      const start = cursor === null ? 0 : Number(cursor)
      return { items: items.slice(start, start + limit), nextCursor: start + limit < items.length ? String(start + limit) : null, revision }
    },
    gbm_logs_read: ({ cursor, limit }) => {
      const start = cursor === null ? 0 : Number(cursor)
      return { lines: LOG_LINES.slice(start, start + limit), nextCursor: start + limit < LOG_LINES.length ? String(start + limit) : null, truncated: false }
    },
    gbm_activity_export: () => ({ outcome: "saved", displayPath: "~/Downloads/git-backup-activity.jsonl" }),
    gbm_settings_get: () => ({ settings, revision: settingsRevision }),
    gbm_settings_update: ({ settings: next, expectedRevision }) => {
      if (expectedRevision !== settingsRevision) fail("PLAN_STALE", "Settings changed elsewhere. Reload to see the latest values.")
      if (next.maxConcurrentJobs < 1 || next.maxConcurrentJobs > 8) fail("PATH_REJECTED", "Max concurrent jobs must be between 1 and 8.")
      settings = { ...next }
      settingsRevision += 1
      return { settings, revision: settingsRevision }
    },
    gbm_repository_open_workspace: ({ repositoryId }) => {
      findRepo(repositoryId)
      return { revision }
    },
    gbm_repository_open_provider: () => ({ revision }),
    gbm_environment_diagnostics: buildDiagnostics,
  }

  return {
    kind: "fixture",
    async call<C extends CommandName>(command: C, args: CommandMap[C]["request"]): Promise<CommandResult<C>> {
      const requestId = nextId("req")
      if (scenario === "unsupported") {
        return { ok: false, requestId, error: { code: "UNSUPPORTED", message: `${command} is not implemented yet.`, retryable: false, retryAt: null, correlationId: requestId, details: null } }
      }
      try {
        const handler = handlers[command] as Handler<C>
        return { ok: true, requestId, data: structuredClone(handler(args)) }
      } catch (error) {
        if (error instanceof Failure) return { ok: false, requestId, error: error.error }
        throw error
      }
    },
  }
}
