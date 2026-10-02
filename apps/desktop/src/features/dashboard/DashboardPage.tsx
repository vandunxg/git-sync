import { useState } from "react"
import { Link } from "react-router"

import { EmptyState, ErrorState, PageHeader, RowSkeletons, StaleNotice } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { keys, useAccounts, useNativeQuery, useScanAction } from "@/lib/api"
import { dateTime, relativeTime } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { Dashboard } from "@/lib/native"

import { AttentionList } from "./AttentionList.tsx"
import { BackupPipeline } from "./BackupPipeline.tsx"
import { MetricCards, MetricCardsSkeleton } from "./MetricCards.tsx"
import { RecentActivity } from "./RecentActivity.tsx"
import { SyncAllDialog } from "./SyncAllDialog.tsx"

function Freshness({ dashboard }: { dashboard: Dashboard | undefined }) {
  if (!dashboard) return <span>Backup health and the actions that need you</span>
  const when = relativeTime(dashboard.observedAt)
  return (
    <span>
      {when ? (
        <>
          Last observed{" "}
          <time dateTime={dashboard.observedAt ?? undefined} title={dateTime(dashboard.observedAt) ?? undefined}>
            {when}
          </time>
        </>
      ) : (
        "Not observed yet"
      )}
      {dashboard.discoveryComplete ? null : " · Discovery incomplete"}
    </span>
  )
}

function ScanButton({ scan }: { scan: ReturnType<typeof useScanAction> }) {
  const reason = scan.unavailableReason
  return (
    <span title={reason ?? undefined}>
      <Button
        variant="outline"
        onClick={scan.run}
        disabled={scan.pending || !scan.available}
        aria-describedby={reason ? "dashboard-scan-reason" : undefined}
      >
        <Icon name="action.scan" />
        {scan.pending ? "Queuing scan…" : "Scan"}
      </Button>
      {reason ? (
        <span id="dashboard-scan-reason" className="sr-only">
          {reason}
        </span>
      ) : null}
    </span>
  )
}

function LoadingBody() {
  return (
    <>
      <span className="sr-only" role="status">
        Loading dashboard…
      </span>
      <MetricCardsSkeleton />
      <div className="grid min-w-0 grid-cols-1 gap-5 xl:grid-cols-[minmax(0,2fr)_minmax(300px,1fr)]" aria-hidden="true">
        <Card className="px-5">
          <RowSkeletons rows={3} />
        </Card>
        <Card className="px-5">
          <RowSkeletons rows={5} />
        </Card>
      </div>
    </>
  )
}

function EmptyDashboard({ scan }: { scan: ReturnType<typeof useScanAction> }) {
  const accounts = useAccounts()
  const githubConnected = accounts.data?.items.some((item) => item.provider === "github" && item.authState === "connected") ?? false
  return (
    <Card>
      <EmptyState
        icon="nav.repositories"
        title={githubConnected ? "No repositories are tracked yet" : "Connect GitHub to discover repositories"}
        description={
          githubConnected
            ? "Scan GitHub to discover repositories, then select the ones to back up in Repositories."
            : "Backups start from a read-only GitHub connection. Nothing is copied until you select repositories."
        }
        action={
          <div className="flex flex-wrap justify-center gap-2">
            {githubConnected ? (
              <>
                <ScanButton scan={scan} />
                <Button variant="outline" nativeButton={false} render={<Link to="/repositories" />}>
                  Open Repositories
                </Button>
              </>
            ) : (
              <Button nativeButton={false} render={<Link to="/connections" />}>
                <Icon name="nav.connections" />
                Go to Connections
              </Button>
            )}
          </div>
        }
      />
    </Card>
  )
}

export function DashboardPage() {
  const dashboard = useNativeQuery(keys.dashboard, "gbm_dashboard_get", {})
  const scan = useScanAction()
  const [syncAllOpen, setSyncAllOpen] = useState(false)

  const data = dashboard.data
  const hasTracked = data !== undefined && (data.tracked === null || data.tracked > 0)
  const syncAllReason = !data ? "Dashboard has not loaded." : hasTracked ? null : "No repositories are tracked."

  return (
    <>
      <PageHeader
        title="Dashboard"
        description={<Freshness dashboard={data} />}
        actions={
          <>
            <ScanButton scan={scan} />
            <span title={syncAllReason ?? undefined}>
              <Button onClick={() => setSyncAllOpen(true)} disabled={syncAllReason !== null}>
                <Icon name="action.sync" />
                Sync all
              </Button>
            </span>
          </>
        }
      />

      {dashboard.isError && data ? (
        <StaleNotice
          observedAt={data.observedAt}
          error={dashboard.error.appError}
          onRetry={() => void dashboard.refetch()}
          retrying={dashboard.isFetching}
        />
      ) : null}

      {dashboard.isPending ? (
        <LoadingBody />
      ) : dashboard.isError && !data ? (
        <ErrorState
          error={dashboard.error.appError}
          title="Could not load the dashboard"
          onRetry={() => void dashboard.refetch()}
          retrying={dashboard.isFetching}
        />
      ) : data && !hasTracked ? (
        <EmptyDashboard scan={scan} />
      ) : data ? (
        <>
          <MetricCards dashboard={data} />
          <div className="grid min-w-0 grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,2fr)_minmax(300px,1fr)]">
            <div className="flex min-w-0 flex-col gap-5">
              <AttentionList dashboard={data} />
              <BackupPipeline />
            </div>
            <RecentActivity />
          </div>
        </>
      ) : null}

      <SyncAllDialog open={syncAllOpen} onOpenChange={setSyncAllOpen} />
    </>
  )
}
