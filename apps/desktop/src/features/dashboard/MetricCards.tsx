import type { ReactNode } from "react"
import { Link } from "react-router"

import { IconTile, type Tone } from "@/components/shared"
import { Skeleton } from "@/components/ui/skeleton"
import type { IconKey } from "@/lib/icons"
import type { Dashboard } from "@/lib/native"

function MetricCard({
  to,
  icon,
  tone,
  label,
  value,
  detail,
  badge,
  linkLabel,
}: {
  to: string
  icon: IconKey
  tone: Tone
  label: string
  value: string
  detail: ReactNode
  badge?: ReactNode
  linkLabel: string
}) {
  return (
    <Link
      to={to}
      aria-label={`${label}: ${value}. ${linkLabel}`}
      className="group flex min-w-0 flex-col gap-3 rounded-[10px] border border-border bg-card p-5 outline-none transition-colors hover:border-primary/40 hover:bg-muted/30 focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <div className="flex min-w-0 items-center gap-3">
        <IconTile icon={icon} tone={tone} />
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-muted-foreground">{label}</span>
        {badge}
      </div>
      <span className="text-[30px] leading-none font-semibold tracking-tight tabular-nums">{value}</span>
      <span className="truncate text-xs text-muted-foreground">{detail}</span>
    </Link>
  )
}

/** Exactly three summary metrics from one revisioned Dashboard read model. */
export function MetricCards({ dashboard }: { dashboard: Dashboard }) {
  const { tracked, upToDate, needsAttention, other, discoveryComplete } = dashboard
  const trackedKnown = tracked !== null
  const partial = !discoveryComplete
  const share = trackedKnown && !partial && tracked > 0 ? Math.round((upToDate / tracked) * 100) : null

  return (
    <div className="grid grid-cols-1 gap-[18px] sm:grid-cols-3">
      <MetricCard
        to="/repositories"
        icon="nav.repositories"
        tone="info"
        label="Tracked repositories"
        value={trackedKnown ? tracked.toLocaleString("en") : "Unknown"}
        detail={partial ? "Discovery incomplete; more may exist" : "Selected for backup from GitHub"}
        badge={
          partial ? (
            <span className="rounded-md bg-warning-soft px-1.5 py-0.5 text-xs font-medium text-warning">Partial</span>
          ) : null
        }
        linkLabel="Open Repositories"
      />
      <MetricCard
        to="/repositories?status=synced"
        icon="status.synced"
        tone="success"
        label="Up to date"
        value={upToDate.toLocaleString("en")}
        detail={share !== null ? `${share}% of tracked, at last verification` : "Synced at last verification"}
        linkLabel="Show synced repositories"
      />
      <MetricCard
        to="/repositories"
        icon="state.warning"
        tone={needsAttention > 0 ? "warning" : "neutral"}
        label="Needs attention"
        value={needsAttention.toLocaleString("en")}
        detail={other > 0 ? `${other.toLocaleString("en")} other in progress or unverified` : "No other repositories pending"}
        linkLabel="Open Repositories"
      />
    </div>
  )
}

export function MetricCardsSkeleton() {
  return (
    <div className="grid grid-cols-1 gap-[18px] sm:grid-cols-3" aria-hidden="true">
      {[0, 1, 2].map((index) => (
        <div key={index} className="flex flex-col gap-3 rounded-[10px] border border-border bg-card p-5">
          <div className="flex items-center gap-3">
            <Skeleton className="size-8 rounded-lg" />
            <Skeleton className="h-3.5 w-28" />
          </div>
          <Skeleton className="h-[30px] w-16" />
          <Skeleton className="h-3 w-40" />
        </div>
      ))}
    </div>
  )
}
