import type { ReactNode } from "react"
import { Link, useNavigate, useParams, useSearchParams } from "react-router"

import {
  destinationLabel,
  ErrorState,
  LoadingLabel,
  MIRROR_STATE,
  PageHeader,
  REPOSITORY_STATUS,
  StaleNotice,
  StatusBadge,
  type StatusDescriptor,
  type Tone,
} from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ERROR_COPY, keys, useNativeQuery } from "@/lib/api"
import { splitFullName } from "@/lib/format"
import { Icon, type IconKey } from "@/lib/icons"
import type { RepositorySummary } from "@/lib/native"
import { cn } from "@/lib/utils"

import { BranchesTab } from "./BranchesTab.tsx"
import { HistoryTab } from "./HistoryTab.tsx"
import { usePendingPlan, useRepositoryCommands } from "./hooks.ts"
import { LogsTab } from "./LogsTab.tsx"
import { coverageChips, primaryActionFor, workspaceOpenBlocker } from "./model.ts"
import { OverviewTab } from "./OverviewTab.tsx"
import { PlanReviewDialog } from "./PlanReviewDialog.tsx"

const TABS = [
  { value: "overview", label: "Overview" },
  { value: "branches", label: "Branches" },
  { value: "history", label: "History" },
  { value: "logs", label: "Logs" },
] as const
type TabValue = (typeof TABS)[number]["value"]

export function RepositoryDetailPage() {
  const { repositoryId = "", tab } = useParams()
  const navigate = useNavigate()
  const activeTab: TabValue = TABS.find((item) => item.value === tab)?.value ?? "overview"
  const repo = useNativeQuery(keys.repository(repositoryId), "gbm_repository_get", { repositoryId })

  const back = (
    <Link
      to="/repositories"
      className="inline-flex w-fit items-center gap-1 rounded-sm text-[13px] text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <Icon name="action.previous" size={14} />
      Repositories
    </Link>
  )

  if (repo.isPending) {
    return (
      <div className="flex flex-col gap-5" aria-busy="true">
        <LoadingLabel label="Loading repository" />
        {back}
        <Skeleton className="h-8 w-72" />
        <Skeleton className="h-6 w-96" />
        <Skeleton className="h-16 w-full rounded-lg" />
        <Skeleton className="h-64 w-full rounded-xl" />
      </div>
    )
  }
  if (repo.isError && !repo.data) {
    return (
      <div className="flex flex-col gap-5">
        {back}
        <ErrorState
          error={repo.error.appError}
          title="Could not load this repository"
          onRetry={() => void repo.refetch()}
          retrying={repo.isFetching}
        />
      </div>
    )
  }

  const data = repo.data
  return (
    <div className="flex flex-col gap-5">
      {back}
      <DetailHeader repo={data} />
      {repo.isError ? (
        <StaleNotice observedAt={data.observedAt} error={repo.error.appError} onRetry={() => void repo.refetch()} retrying={repo.isFetching} />
      ) : null}
      <BlockerBanner repo={data} onViewLogs={() => navigate(`/repositories/${encodeURIComponent(data.id)}/logs`)} />
      <Tabs
        value={activeTab}
        onValueChange={(value) => navigate(`/repositories/${encodeURIComponent(data.id)}/${String(value)}`)}
        className="gap-4"
      >
        <TabsList className="h-9 group-data-horizontal/tabs:h-9">
          {TABS.map((item) => (
            <TabsTrigger key={item.value} value={item.value} className="px-3 text-[13px]">
              {item.label}
            </TabsTrigger>
          ))}
        </TabsList>
        <TabsContent value="overview">
          <OverviewTab repo={data} />
        </TabsContent>
        <TabsContent value="branches">
          <BranchesTab repositoryId={data.id} />
        </TabsContent>
        <TabsContent value="history">
          <HistoryTab repositoryId={data.id} />
        </TabsContent>
        <TabsContent value="logs">
          <LogsTab repositoryId={data.id} />
        </TabsContent>
      </Tabs>
    </div>
  )
}

function DetailHeader({ repo }: { repo: RepositorySummary }) {
  const [params, setParams] = useSearchParams()
  const commands = useRepositoryCommands()
  const pending = usePendingPlan(repo.id)
  const action = primaryActionFor(repo)
  const folderBlocker = workspaceOpenBlocker(repo)
  const { owner, name } = splitFullName(repo.fullName)
  const reviewOpen = params.get("review") === "1" && pending.planId !== null

  const setReview = (open: boolean) =>
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev)
        if (open) next.set("review", "1")
        else next.delete("review")
        return next
      },
      { replace: true },
    )

  let primary: ReactNode
  switch (action) {
    case "reconnect":
      primary = (
        <Button nativeButton={false} render={<Link to="/connections" />}>
          <Icon name="action.reconnect" />
          {repo.destinationState === "inaccessible" ? "Reconnect GitLab" : "Reconnect"}
        </Button>
      )
      break
    case "review":
      primary = (
        <Button
          onClick={() => (pending.planId ? setReview(true) : commands.createPlan.mutate(repo.id))}
          disabled={
            commands.createPlan.isPending ||
            (pending.planId === null && (pending.preparingJobId !== null || !pending.complete))
          }
          title={pending.planId === null && !pending.complete ? "Open Sync Center to find this repository's pending plan." : undefined}
        >
          <Icon name="status.blocked" />
          {pending.planId === null && pending.preparingJobId !== null ? "Preparing plan…" : "Review changes"}
        </Button>
      )
      break
    case "running":
      primary = (
        <Button disabled>
          <Icon name="status.running" spinning />
          Sync running
        </Button>
      )
      break
    case "retry":
    case "sync":
      primary = (
        <Button onClick={() => commands.createPlan.mutate(repo.id)} disabled={commands.createPlan.isPending}>
          <Icon name={action === "retry" ? "action.retry" : "action.sync"} />
          {commands.createPlan.isPending ? "Queuing…" : action === "retry" ? "Retry" : "Sync now"}
        </Button>
      )
      break
  }

  return (
    <>
      <PageHeader
        title={<span title={repo.fullName}>{name}</span>}
        description={
          <span className="block truncate" title={repo.fullName}>
            {owner ? `${owner} / ${name}` : name}
          </span>
        }
        actions={
          <>
            <Button
              variant="outline"
              onClick={() => commands.openWorkspace.mutate(repo.id)}
              disabled={folderBlocker !== null || commands.openWorkspace.isPending}
              title={folderBlocker ?? undefined}
              aria-describedby={folderBlocker ? "open-folder-reason" : undefined}
            >
              <Icon name="action.open_folder" />
              Open folder
            </Button>
            {folderBlocker ? (
              <span id="open-folder-reason" className="sr-only">
                {folderBlocker}
              </span>
            ) : null}
            {primary}
          </>
        }
        meta={<MetadataRow repo={repo} />}
      />
      {pending.planId ? <PlanReviewDialog planId={pending.planId} open={reviewOpen} onOpenChange={setReview} /> : null}
    </>
  )
}

function MetadataRow({ repo }: { repo: RepositorySummary }) {
  const visibility = [repo.isPrivate ? "Private" : "Public", repo.isFork && "Fork", repo.isArchived && "Archived", !repo.selected && "Excluded from backup"]
    .filter((flag): flag is string => typeof flag === "string")
    .join(" · ")
  return (
    <>
      <StatusBadge status={REPOSITORY_STATUS[repo.primaryStatus]} />
      <Chip icon="metadata.branch" label={repo.defaultBranch ?? "No branch"} />
      <Chip icon={repo.isPrivate ? "event.auth" : "nav.repositories"} label={visibility} />
      <span className="mx-0.5 h-4 w-px bg-border" aria-hidden="true" />
      <ul aria-label="Backup coverage" className="flex flex-wrap items-center gap-2">
        {coverageChips(repo.coverage).map((chip) => (
          <li key={chip.label}>
            <Chip icon={chip.icon} label={chip.label} tone={chip.tone} />
          </li>
        ))}
      </ul>
    </>
  )
}

const CHIP_ICON_TONE: Record<Tone, string> = {
  success: "text-success",
  warning: "text-warning",
  danger: "text-danger",
  info: "text-primary",
  neutral: "text-muted-foreground",
}

function Chip({ icon, label, tone = "neutral" }: { icon: IconKey; label: string; tone?: Tone }) {
  return (
    <span className="inline-flex h-6 max-w-full items-center gap-1.5 rounded-md border bg-card px-2 text-xs whitespace-nowrap">
      <Icon name={icon} size={14} className={CHIP_ICON_TONE[tone]} />
      <span className="truncate">{label}</span>
    </span>
  )
}

interface Banner {
  status: StatusDescriptor
  title: string
  body: string
  action?: ReactNode
}

/** One banner for the most important blocker, chosen from native fields only. */
function pickBanner(repo: RepositorySummary, actions: { openFolder: ReactNode; viewLogs: ReactNode }): Banner | null {
  const context = `Local mirror: ${MIRROR_STATE[repo.mirrorState]}. GitLab: ${destinationLabel(repo)}.`
  if (repo.lastErrorCode === "AUTH_REQUIRED") {
    return {
      status: { label: "Connection", icon: "status.auth_error", tone: "danger" },
      title: "GitLab connection needs attention",
      body: `${ERROR_COPY.AUTH_REQUIRED} Backups to GitLab are paused for this repository. ${context}`,
    }
  }
  if (repo.primaryStatus === "blocked" || repo.lastErrorCode === "APPROVAL_REQUIRED") {
    return {
      status: { label: "Review", icon: "status.blocked", tone: "warning" },
      title: "This plan changes existing history",
      body: `Nothing is written to GitLab until you approve the exact ref changes. ${context}`,
    }
  }
  if (repo.lastErrorCode !== null && repo.lastErrorCode !== "WORKSPACE_SKIPPED") {
    return {
      status: { label: "Error", icon: "status.failed", tone: "danger" },
      title: ERROR_COPY[repo.lastErrorCode],
      body: `The last attempt ended with ${repo.lastErrorCode}. ${context}`,
      action: actions.viewLogs,
    }
  }
  if (repo.workspaceState === "dirty" || repo.lastErrorCode === "WORKSPACE_SKIPPED") {
    return {
      status: { label: "Workspace", icon: "job.workspace", tone: "warning" },
      title: "Your local edits will not be changed",
      body: "The workspace has local changes, so its update is skipped. Mirror and GitLab backups continue separately.",
      action: actions.openFolder,
    }
  }
  if (repo.sourceState === "unavailable") {
    return {
      status: { label: "Source", icon: "state.warning", tone: "warning" },
      title: ERROR_COPY.SOURCE_UNAVAILABLE,
      body: "GitHub did not return this repository on the last scan. Existing backups are kept.",
    }
  }
  return null
}

const BANNER_TONE: Record<Tone, string> = {
  danger: "border-danger/30 bg-danger-soft text-danger",
  warning: "border-warning/30 bg-warning-soft text-warning",
  success: "border-success/30 bg-success-soft text-success",
  info: "border-primary/30 bg-primary-soft text-primary",
  neutral: "border-border bg-card text-foreground",
}

function BlockerBanner({ repo, onViewLogs }: { repo: RepositorySummary; onViewLogs: () => void }) {
  const commands = useRepositoryCommands()
  const folderBlocker = workspaceOpenBlocker(repo)
  const banner = pickBanner(repo, {
    openFolder: (
      <Button variant="outline" onClick={() => commands.openWorkspace.mutate(repo.id)} disabled={folderBlocker !== null}>
        <Icon name="action.open_folder" />
        Open folder
      </Button>
    ),
    viewLogs: (
      <Button variant="outline" onClick={onViewLogs}>
        <Icon name="nav.activity" />
        View logs
      </Button>
    ),
  })
  if (banner === null) return null
  return (
    <div
      role={banner.status.tone === "danger" ? "alert" : "status"}
      className={cn("flex flex-wrap items-center gap-x-4 gap-y-3 rounded-[10px] border px-4 py-3", BANNER_TONE[banner.status.tone])}
    >
      <Icon name={banner.status.icon} size={18} className="shrink-0" />
      <div className="min-w-0 flex-1 basis-80">
        <p className="text-sm font-semibold">{banner.title}</p>
        <p className="text-[13px] opacity-90">{banner.body}</p>
      </div>
      {banner.action ? <div className="shrink-0">{banner.action}</div> : null}
    </div>
  )
}

