import { Link, useNavigate } from "react-router"

import { IconTile, SectionCard, StatusBadge } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { useScanAction, useSyncRepositoriesAction } from "@/lib/api"
import { dateTime, relativeTime, splitFullName } from "@/lib/format"
import { Icon, type IconKey } from "@/lib/icons"
import type { Dashboard, RepositorySummary } from "@/lib/native"

import { describeAttention, type AttentionAction } from "./attention.ts"

const ACTION_ICON: Record<AttentionAction["kind"], IconKey> = {
  reconnect: "action.reconnect",
  check_access: "nav.connections",
  review: "status.blocked",
  create_local: "event.created",
  sync: "action.sync",
  retry: "action.retry",
  scan: "action.scan",
  details: "action.next",
}

function RowAction({ repo, action }: { repo: RepositorySummary; action: AttentionAction }) {
  const navigate = useNavigate()
  const sync = useSyncRepositoriesAction()
  const scan = useScanAction()
  const name = repo.fullName

  let onClick: () => void
  let pending = false
  let disabledReason: string | null = null
  switch (action.kind) {
    case "reconnect":
    case "check_access":
      onClick = () => void navigate("/connections")
      break
    case "review":
    case "details":
      onClick = () => void navigate(`/repositories/${encodeURIComponent(repo.id)}`)
      break
    case "create_local":
    case "sync":
    case "retry":
      onClick = () => sync.run([repo.id])
      pending = sync.pending
      break
    case "scan":
      onClick = scan.run
      pending = scan.pending
      disabledReason = scan.unavailableReason
      break
  }

  const reasonId = `attention-reason-${repo.id}`
  return (
    <span className="shrink-0" title={disabledReason ?? undefined}>
      <Button
        variant="outline"
        onClick={onClick}
        disabled={pending || disabledReason !== null}
        aria-label={`${action.label} for ${name}`}
        aria-describedby={disabledReason ? reasonId : undefined}
      >
        <Icon name={ACTION_ICON[action.kind]} />
        {pending ? "Queuing…" : action.label}
      </Button>
      {disabledReason ? (
        <span id={reasonId} className="sr-only">
          {disabledReason}
        </span>
      ) : null}
    </span>
  )
}

function AttentionRow({ repo }: { repo: RepositorySummary }) {
  const view = describeAttention(repo)
  const { owner, name } = splitFullName(repo.fullName)
  const observed = relativeTime(repo.observedAt)
  return (
    <li className="flex min-w-0 items-center gap-3 py-3 first:pt-0 last:pb-0">
      <IconTile icon={view.visual.icon} tone={view.visual.tone} spinning={view.visual.spinning} />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <Link
            to={`/repositories/${encodeURIComponent(repo.id)}`}
            title={repo.fullName}
            className="min-w-0 truncate rounded-sm text-sm font-semibold outline-none hover:underline focus-visible:ring-3 focus-visible:ring-ring/50"
          >
            {owner ? <span className="font-normal text-muted-foreground">{owner}/</span> : null}
            {name}
          </Link>
          <StatusBadge status={view.visual} className="hidden lg:inline-flex" />
        </div>
        <p className="mt-0.5 truncate text-[13px]" title={view.problem}>
          {view.problem}
        </p>
        <p className="mt-0.5 truncate text-xs text-muted-foreground">
          {view.context} ·{" "}
          {observed ? (
            <time dateTime={repo.observedAt ?? undefined} title={dateTime(repo.observedAt) ?? undefined}>
              Observed {observed}
            </time>
          ) : (
            "Not observed yet"
          )}
        </p>
      </div>
      {view.action ? <RowAction repo={repo} action={view.action} /> : null}
    </li>
  )
}

export function AttentionList({ dashboard }: { dashboard: Dashboard }) {
  const items = dashboard.attentionItems
  const truncated = dashboard.needsAttention > items.length
  const verified = relativeTime(dashboard.observedAt)

  return (
    <SectionCard
      icon="state.warning"
      tone={items.length > 0 ? "warning" : "success"}
      title="Needs attention"
      description={
        items.length > 0
          ? truncated
            ? `Showing ${items.length} of ${dashboard.needsAttention} repositories that need an action`
            : "Only repositories that need an action"
          : "Repositories that need an action appear here"
      }
      action={
        truncated ? (
          <Button variant="ghost" size="sm" nativeButton={false} render={<Link to="/repositories" />}>
            View all
            <Icon name="action.next" size={14} />
          </Button>
        ) : null
      }
    >
      {items.length > 0 ? (
        <ul className="flex flex-col divide-y divide-border" aria-label="Repositories that need attention">
          {items.map((repo) => (
            <AttentionRow key={repo.id} repo={repo} />
          ))}
        </ul>
      ) : (
        <div className="flex items-center gap-3 rounded-lg bg-muted/50 px-4 py-3">
          <Icon name="status.synced" className="text-success" />
          <p className="text-[13px]">
            No action required at the last verification
            {verified ? (
              <>
                {" "}
                (<time dateTime={dashboard.observedAt ?? undefined} title={dateTime(dashboard.observedAt) ?? undefined}>{verified}</time>)
              </>
            ) : null}
            .
          </p>
        </div>
      )}
    </SectionCard>
  )
}
