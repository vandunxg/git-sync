import { DIAGNOSTIC_STATUS, ErrorState, PageHeader, RowSkeletons, SectionCard, StaleNotice, StatusBadge } from "@/components/shared"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { keys, useAccounts, useNativeQuery } from "@/lib/api"
import { dateTime, relativeTime } from "@/lib/format"
import { Icon } from "@/lib/icons"

import { accountFor } from "./providerCopy.ts"
import { ProviderSection } from "./ProviderSection.tsx"

const DIAGNOSTIC_LABEL: Record<string, string> = {
  git: "Git",
  "git-lfs": "Git LFS",
  keyring: "System keyring",
  "storage-root": "Backup folder",
  portal: "File chooser portal",
  tray: "System tray",
}

/** Known keys get product names; unknown native keys are humanized, never hidden. */
export function diagnosticLabel(key: string): string {
  const known = DIAGNOSTIC_LABEL[key]
  if (known) return known
  const words = key.replace(/[-_.]+/g, " ").trim()
  return words.length > 0 ? words.charAt(0).toUpperCase() + words.slice(1) : key
}

function LocalStorageSection() {
  const diagnostics = useNativeQuery(keys.diagnostics, "gbm_environment_diagnostics", {})
  const observed = relativeTime(diagnostics.data?.observedAt)
  return (
    <SectionCard
      icon="setting.backup"
      title="Local storage"
      description={
        observed ? (
          <span title={dateTime(diagnostics.data?.observedAt) ?? undefined}>Backup folder and system services · checked {observed}</span>
        ) : (
          "Backup folder and system services on this device"
        )
      }
      action={
        <Button variant="outline" onClick={() => void diagnostics.refetch()} disabled={diagnostics.isFetching}>
          <Icon name="action.retry" />
          {diagnostics.isFetching && !diagnostics.isPending ? "Checking…" : "Check again"}
        </Button>
      }
      contentClassName="flex flex-col gap-4"
    >
      {diagnostics.isPending ? (
        <RowSkeletons rows={4} />
      ) : diagnostics.isError && !diagnostics.data ? (
        <ErrorState
          error={diagnostics.error.appError}
          title="Could not check local storage"
          onRetry={() => void diagnostics.refetch()}
          retrying={diagnostics.isFetching}
        />
      ) : (
        <>
          {diagnostics.isError ? (
            <StaleNotice
              observedAt={diagnostics.data.observedAt}
              error={diagnostics.error.appError}
              onRetry={() => void diagnostics.refetch()}
              retrying={diagnostics.isFetching}
            />
          ) : null}
          {diagnostics.data.items.length === 0 ? (
            <p className="text-[13px] text-muted-foreground">No checks were reported by the native application.</p>
          ) : (
            <ul className="divide-y rounded-lg border">
              {diagnostics.data.items.map((item) => (
                <li key={item.key} className="flex min-h-12 flex-wrap items-center gap-x-4 gap-y-1 px-4 py-2.5">
                  <div className="min-w-0 flex-1">
                    <p className="text-[13px] font-medium">{diagnosticLabel(item.key)}</p>
                    <p className="text-xs break-words text-muted-foreground ui-selectable">{item.detail}</p>
                  </div>
                  <StatusBadge status={DIAGNOSTIC_STATUS[item.status]} />
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      <p className="flex items-start gap-2 text-xs text-muted-foreground">
        <Icon name="status.blocked" size={14} className="mt-0.5" />
        <span>
          Changing the backup folder location is not supported in this version. The folder chosen during setup stays in use.
        </span>
      </p>
    </SectionCard>
  )
}

export function ConnectionsPage() {
  const accounts = useAccounts()
  const items = accounts.data?.items
  const loading = accounts.isPending
  return (
    <>
      <PageHeader title="Connections" description="Source, backup destination and local storage." />
      {accounts.isError && !accounts.data ? (
        <ErrorState
          error={accounts.error.appError}
          title="Could not load connected accounts"
          onRetry={() => void accounts.refetch()}
          retrying={accounts.isFetching}
        />
      ) : null}
      {accounts.isError && accounts.data ? (
        <StaleNotice observedAt={null} error={accounts.error.appError} onRetry={() => void accounts.refetch()} retrying={accounts.isFetching} />
      ) : null}
      {accounts.isError && !accounts.data ? null : (
        <>
          <ProviderSection provider="github" account={accountFor(items, "github")} loading={loading} />
          <ProviderSection provider="gitlab" account={accountFor(items, "gitlab")} loading={loading} />
        </>
      )}
      <LocalStorageSection />
      <Alert role="note" className="w-fit max-w-full border-primary/20 bg-primary-soft px-4 py-3 text-primary">
        <Icon name="status.healthy" />
        <AlertDescription className="text-[13px] font-medium text-primary">
          Credentials are never stored in plain text inside the app database.
        </AlertDescription>
      </Alert>
    </>
  )
}
