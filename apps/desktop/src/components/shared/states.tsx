import type { ReactNode } from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"
import { ERROR_COPY } from "@/lib/api"
import { relativeTime } from "@/lib/format"
import { Icon, type IconKey } from "@/lib/icons"
import type { AppError } from "@/lib/native"
import { cn } from "@/lib/utils"

/** Empty and filtered-empty are different states; callers pass the right copy/action. */
export function EmptyState({
  icon = "state.empty",
  title,
  description,
  action,
  className,
}: {
  icon?: IconKey
  title: string
  description?: ReactNode
  action?: ReactNode
  className?: string
}) {
  return (
    <Empty className={cn("py-10", className)}>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Icon name={icon} />
        </EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        {description ? <EmptyDescription>{description}</EmptyDescription> : null}
      </EmptyHeader>
      {action ? <EmptyContent>{action}</EmptyContent> : null}
    </Empty>
  )
}

/** Typed native error with the stable code, actionable copy and correlation ID. */
export function ErrorState({
  error,
  title = "Could not load this view",
  onRetry,
  retrying = false,
  action,
  className,
}: {
  error: AppError | null
  title?: string
  onRetry?: () => void
  retrying?: boolean
  action?: ReactNode
  className?: string
}) {
  const code = error?.code ?? "INTERNAL"
  return (
    <Alert variant="destructive" className={cn("border-danger/30 bg-danger-soft px-4 py-3", className)}>
      <Icon name="state.error" />
      <AlertTitle className="text-sm font-semibold">{title}</AlertTitle>
      <AlertDescription className="flex flex-col gap-3 text-[13px]">
        <p>
          {ERROR_COPY[code]}
          {error && error.message !== ERROR_COPY[code] ? <span className="text-danger/80"> {error.message}</span> : null}
        </p>
        <p className="font-mono text-xs text-danger/70 ui-selectable">
          {code}
          {error ? ` · ref ${error.correlationId.slice(0, 8)}` : null}
        </p>
        {onRetry || action ? (
          <div className="flex flex-wrap gap-2">
            {onRetry ? (
              <Button variant="outline" onClick={onRetry} disabled={retrying}>
                <Icon name="action.retry" />
                {retrying ? "Retrying…" : "Try again"}
              </Button>
            ) : null}
            {action}
          </div>
        ) : null}
      </AlertDescription>
    </Alert>
  )
}

/** Cached data remains visible when a refresh fails (UI_CONTRACT async rules). */
export function StaleNotice({
  observedAt,
  error,
  onRetry,
  retrying,
}: {
  observedAt: string | null
  error: AppError | null
  onRetry: () => void
  retrying: boolean
}) {
  const when = relativeTime(observedAt)
  return (
    <Alert className="border-warning/30 bg-warning-soft px-4 py-2.5 text-warning">
      <Icon name="status.stale" />
      <AlertTitle className="text-[13px] font-semibold">
        {when ? `Showing data observed ${when}` : "Showing the last loaded data"}
      </AlertTitle>
      <AlertDescription className="flex flex-wrap items-center justify-between gap-2 text-xs text-warning/90">
        <span>Refresh failed{error ? `: ${ERROR_COPY[error.code]}` : "."}</span>
        <Button size="sm" variant="outline" onClick={onRetry} disabled={retrying}>
          <Icon name="action.retry" size={14} />
          Retry
        </Button>
      </AlertDescription>
    </Alert>
  )
}

/** Row skeletons keep the list geometry while loading. */
export function RowSkeletons({ rows = 4, className }: { rows?: number; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-3", className)} aria-hidden="true">
      {Array.from({ length: rows }, (_, index) => (
        <div key={index} className="flex items-center gap-3">
          <Skeleton className="size-8 rounded-lg" />
          <div className="flex flex-1 flex-col gap-1.5">
            <Skeleton className="h-3.5 w-2/5" />
            <Skeleton className="h-3 w-3/5" />
          </div>
        </div>
      ))}
    </div>
  )
}

export function LoadingLabel({ label = "Loading" }: { label?: string }) {
  return <span className="sr-only" role="status">{label}…</span>
}
