import type { ReactNode } from "react"
import { Link } from "react-router"

import { AUTH_STATE, SectionCard, StatusBadge, type StatusDescriptor } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { useAccounts } from "@/lib/api"
import { BrandMark, Icon, PROVIDER_LABEL } from "@/lib/icons"
import type { AccountSummary, Provider } from "@/lib/native"

const NOT_CONNECTED: StatusDescriptor = { label: "Not connected", icon: "status.no_changes", tone: "neutral" }

function Node({ mark, title, detail, status }: { mark: ReactNode; title: string; detail: string; status: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-1 items-center gap-3 rounded-lg border border-border px-3 py-2.5">
      <span className="inline-flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted">{mark}</span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-semibold">{title}</span>
        <span className="block truncate text-xs text-muted-foreground" title={detail}>
          {detail}
        </span>
      </span>
      {status}
    </div>
  )
}

function Arrow() {
  return (
    <span className="flex shrink-0 items-center justify-center text-muted-foreground max-md:rotate-90 max-md:self-center">
      <Icon name="action.next" />
    </span>
  )
}

function ProviderNode({ provider, account, loading }: { provider: Provider; account: AccountSummary | undefined; loading: boolean }) {
  const role = provider === "github" ? "Source, read-only" : "Backup destination"
  return (
    <Node
      mark={<BrandMark provider={provider} size={18} />}
      title={PROVIDER_LABEL[provider]}
      detail={account ? `${role} · ${account.username}` : role}
      status={loading ? <Skeleton className="h-6 w-20" /> : <StatusBadge status={account ? AUTH_STATE[account.authState] : NOT_CONNECTED} />}
    />
  )
}

/**
 * GitHub -> local mirror -> GitLab. Provider nodes show the account's real auth state;
 * the local node is a plain label because no aggregate mirror state exists in the
 * Dashboard read model. A connected scheduler never implies healthy backups (DD-04).
 */
export function BackupPipeline() {
  const accounts = useAccounts()
  const items = accounts.data?.items ?? []
  const github = items.find((account) => account.provider === "github")
  const gitlab = items.find((account) => account.provider === "gitlab")

  return (
    <SectionCard
      icon="setting.backup"
      title="Backup path"
      description="GitHub → local mirror → GitLab"
      action={
        <Button variant="ghost" size="sm" nativeButton={false} render={<Link to="/connections" />}>
          Connections
          <Icon name="action.next" size={14} />
        </Button>
      }
    >
      {accounts.isError && !accounts.data ? (
        <p className="text-[13px] text-muted-foreground">Connection state could not be loaded.</p>
      ) : (
        <div className="flex min-w-0 flex-col items-stretch gap-2 md:flex-row md:items-center">
          <ProviderNode provider="github" account={github} loading={accounts.isPending} />
          <Arrow />
          <Node
            mark={<Icon name="action.open_folder" className="text-muted-foreground" />}
            title="Local mirror"
            detail="Mirrors kept on this device"
            status={null}
          />
          <Arrow />
          <ProviderNode provider="gitlab" account={gitlab} loading={accounts.isPending} />
        </div>
      )}
    </SectionCard>
  )
}
