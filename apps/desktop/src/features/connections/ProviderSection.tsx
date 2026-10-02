import { useState } from "react"
import { toast } from "sonner"

import { AUTH_STATE, ConfirmDialog, ErrorState, RowSkeletons, StatusBadge } from "@/components/shared"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { keys, repositoryStateKeys, useNativeMutation } from "@/lib/api"
import { BrandMark, Icon, PROVIDER_LABEL } from "@/lib/icons"
import type { AccountSummary, Provider } from "@/lib/native"

import { AccountFacts } from "./AccountFacts.tsx"
import { hasCredential, PROVIDER_COPY } from "./providerCopy.ts"
import { TokenDialog } from "./TokenDialog.tsx"

/** Inline diagnosis for a credential that cannot be used (UI-10 pattern 2). */
export function AuthAttention({ account, onReconnect }: { account: AccountSummary; onReconnect: () => void }) {
  if (account.authState !== "expired" && account.authState !== "forbidden") return null
  const label = PROVIDER_LABEL[account.provider]
  return (
    <Alert variant="destructive" className="border-danger/30 bg-danger-soft px-4 py-3">
      <Icon name="status.auth_error" />
      <AlertTitle className="text-sm font-semibold">{label} connection needs attention</AlertTitle>
      <AlertDescription className="flex flex-col gap-3 text-[13px] text-danger/90">
        <span className="block">
          {account.authState === "expired"
            ? `The stored token for ${account.username} has expired. Backups that need ${label} are paused until you reconnect; existing backups are kept.`
            : `The token for ${account.username} no longer grants the required access. Replace it with a token that has the permissions below.`}
        </span>
        <div>
          <Button onClick={onReconnect}>
            <Icon name="action.reconnect" />
            Reconnect {label}
          </Button>
        </div>
      </AlertDescription>
    </Alert>
  )
}

export function ProviderSection({
  provider,
  account,
  loading,
}: {
  provider: Provider
  account: AccountSummary | null
  loading: boolean
}) {
  const label = PROVIDER_LABEL[provider]
  const copy = PROVIDER_COPY[provider]
  const [tokenMode, setTokenMode] = useState<"connect" | "replace" | null>(null)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const disconnect = useNativeMutation("gbm_accounts_disconnect", [keys.accounts, keys.bootstrap, ...repositoryStateKeys])
  const connected = hasCredential(account)
  const headingId = `connection-${provider}`

  const runDisconnect = () => {
    if (!account) return
    disconnect.mutate(
      { accountId: account.id },
      {
        onSuccess: () => {
          setConfirmOpen(false)
          toast.success(`${label} disconnected`, { description: "The local credential was removed. Backups are retained." })
        },
      },
    )
  }

  return (
    <Card role="region" aria-labelledby={headingId}>
      <div className="flex flex-col gap-4 px-5">
        <div className="flex flex-wrap items-start gap-x-4 gap-y-3">
          <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-lg border bg-card">
            <BrandMark provider={provider} size={22} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 id={headingId} className="text-[15px] leading-snug font-semibold">
              {label}
            </h2>
            <p className="truncate text-[13px] text-muted-foreground" title={account?.username}>
              {account && connected ? `${copy.accountRole} · ${account.username}` : `${copy.accountRole} · not connected`}
            </p>
          </div>
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            {loading ? null : account ? <StatusBadge status={AUTH_STATE[account.authState]} /> : <StatusBadge status={{ label: "Not connected", icon: "status.no_changes", tone: "neutral" }} />}
            {!loading && connected ? (
              <DropdownMenu>
                <DropdownMenuTrigger render={<Button variant="outline" aria-label={`Manage ${label} connection`} />}>
                  <Icon name="setting.general" />
                  Manage
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-52">
                  <DropdownMenuItem onClick={() => setTokenMode("replace")}>
                    <Icon name="event.auth" />
                    Replace token
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem variant="destructive" onClick={() => setConfirmOpen(true)}>
                    <Icon name="action.cancel" />
                    Disconnect
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            ) : null}
          </div>
        </div>

        {loading ? (
          <RowSkeletons rows={1} />
        ) : account && connected ? (
          <>
            <AuthAttention account={account} onReconnect={() => setTokenMode("replace")} />
            <AccountFacts account={account} />
          </>
        ) : (
          <div className="flex flex-col gap-3 rounded-lg border border-dashed px-4 py-4 sm:flex-row sm:items-center sm:justify-between">
            <div className="min-w-0">
              <p className="text-sm font-medium">
                {provider === "github" ? "Connect GitHub to discover repositories" : "Connect GitLab to store backup copies"}
              </p>
              <p className="text-[13px] text-muted-foreground">
                {account && account.authState === "disconnected"
                  ? `${account.username} was disconnected. Existing local and GitLab backups are retained.`
                  : copy.purpose}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">{copy.permissions}</p>
            </div>
            <Button className="shrink-0" onClick={() => setTokenMode("connect")}>
              <Icon name="event.auth" />
              Connect with token
            </Button>
          </div>
        )}
      </div>

      <TokenDialog
        provider={provider}
        mode={tokenMode ?? "connect"}
        open={tokenMode !== null}
        onOpenChange={(open) => (open ? undefined : setTokenMode(null))}
      />
      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        destructive
        icon="state.warning"
        title={`Disconnect ${label}?`}
        description={`This removes the ${label} credential${account ? ` for ${account.username}` : ""} from this device.`}
        confirmLabel={`Disconnect ${label}`}
        pendingLabel="Disconnecting…"
        pending={disconnect.isPending}
        onConfirm={runDisconnect}
      >
        <ul className="flex flex-col gap-2">
          <li className="flex gap-2">
            <Icon name="action.pause" className="mt-0.5 text-muted-foreground" />
            <span>Scans and syncs that need {label} stop until you connect again.</span>
          </li>
          <li className="flex gap-2">
            <Icon name="event.auth" className="mt-0.5 text-muted-foreground" />
            <span>The locally stored credential is removed.</span>
          </li>
          <li className="flex gap-2">
            <Icon name="status.healthy" className="mt-0.5 text-muted-foreground" />
            <span>Local mirrors and GitLab backup projects are retained.</span>
          </li>
          <li className="flex gap-2">
            <Icon name="status.unknown" className="mt-0.5 text-muted-foreground" />
            <span>The token is not revoked on {label}. Revoke it in your {label} settings if you no longer need it.</span>
          </li>
        </ul>
        {disconnect.error ? <ErrorState className="mt-3" error={disconnect.error.appError} title="Disconnect failed" /> : null}
      </ConfirmDialog>
    </Card>
  )
}
