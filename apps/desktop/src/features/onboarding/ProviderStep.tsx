import { useInfiniteQuery } from "@tanstack/react-query"
import { useId, useState } from "react"

import { AUTH_STATE, ErrorState, RowSkeletons, StatusBadge } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { keys, runNative, toAppError, useNativePort, type NativeCommandError } from "@/lib/api"
import { AccountFacts, AuthAttention, PROVIDER_COPY, TokenDialog } from "@/features/connections"
import { BrandMark, Icon, PROVIDER_LABEL } from "@/lib/icons"
import type { AccountSummary, Namespace, Provider } from "@/lib/native"

import { StepLayout } from "./StepLayout.tsx"
import type { Step } from "./Stepper.tsx"

export type Visibility = "private" | "public"

export interface DestinationState {
  namespace: Namespace | null
  visibility: Visibility
}

function ProviderCard({ provider, account }: { provider: Provider; account: AccountSummary | null }) {
  const [dialog, setDialog] = useState<"connect" | "replace" | null>(null)
  const label = PROVIDER_LABEL[provider]
  const copy = PROVIDER_COPY[provider]
  const usable = account !== null && account.authState !== "disconnected" && account.storageMode !== "none"
  return (
    <Card role="region" aria-labelledby={`provider-${provider}`}>
      <div className="flex flex-col gap-4 px-5">
        <div className="flex flex-wrap items-start gap-x-4 gap-y-2">
          <span className="inline-flex size-10 shrink-0 items-center justify-center rounded-lg border bg-card">
            <BrandMark provider={provider} size={22} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 id={`provider-${provider}`} className="text-[15px] font-semibold">
              {label}
            </h2>
            <p className="text-[13px] text-muted-foreground">{copy.role}</p>
          </div>
          {usable && account ? <StatusBadge status={AUTH_STATE[account.authState]} /> : null}
        </div>
        <p className="text-sm">{copy.purpose}</p>
        <p className="flex items-start gap-2 text-xs text-muted-foreground">
          <Icon name="event.auth" size={14} className="mt-0.5" />
          <span>{copy.permissions}</span>
        </p>

        {usable && account ? (
          <div className="flex flex-col gap-4 rounded-lg border px-4 py-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-sm">
                Connected as <span className="font-semibold ui-selectable">{account.username}</span>
              </p>
              <Button variant="outline" onClick={() => setDialog("replace")}>
                Replace token
              </Button>
            </div>
            <AuthAttention account={account} onReconnect={() => setDialog("replace")} />
            <AccountFacts account={account} />
          </div>
        ) : (
          <div className="flex flex-wrap items-center gap-3">
            <Button onClick={() => setDialog("connect")}>
              <Icon name="event.auth" />
              Connect with token
            </Button>
            <p className="text-xs text-muted-foreground">You paste a personal access token; it is validated with {label} before it is stored.</p>
          </div>
        )}
      </div>
      <TokenDialog provider={provider} mode={dialog ?? "connect"} open={dialog !== null} onOpenChange={(open) => (open ? undefined : setDialog(null))} />
    </Card>
  )
}

function namespacePermission(namespace: Namespace | null): string | null {
  if (namespace === null) return null
  if (namespace.canCreateProject === true) return "You can create projects in this namespace."
  if (namespace.canCreateProject === false) return "You cannot create projects here; only existing projects can be adopted."
  return "Permission unknown until confirmed. GitLab checks it when the first project is created."
}

function DestinationCard({
  accountId,
  destination,
  onChange,
}: {
  accountId: string
  destination: DestinationState
  onChange: (next: DestinationState) => void
}) {
  const ids = useId()
  const port = useNativePort()
  const namespaces = useInfiniteQuery({
    queryKey: [...keys.namespaces(accountId), "pages"],
    queryFn: ({ pageParam }) => runNative(port, "gbm_namespaces_list", { accountId, cursor: pageParam, limit: 50 }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor,
  })
  const items = namespaces.data?.pages.flatMap((page) => page.items) ?? []
  const permission = namespacePermission(destination.namespace)

  return (
    <Card role="region" aria-labelledby={`${ids}-title`}>
      <div className="flex flex-col gap-4 px-5">
        <div>
          <h2 id={`${ids}-title`} className="text-[15px] font-semibold">
            Backup destination
          </h2>
          <p className="text-[13px] text-muted-foreground">Where backup projects are created or adopted. Nothing is created in this step.</p>
        </div>
        {namespaces.isPending ? (
          <RowSkeletons rows={1} />
        ) : namespaces.isError && items.length === 0 ? (
          <ErrorState
            error={toAppError(namespaces.error)}
            title="Could not load GitLab namespaces"
            onRetry={() => void namespaces.refetch()}
            retrying={namespaces.isFetching}
          />
        ) : (
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-[minmax(0,1fr)_200px]">
            <div className="flex min-w-0 flex-col gap-1.5">
              <label id={`${ids}-ns-label`} className="text-[13px] font-medium">
                Namespace
              </label>
              <Select
                items={items.map((item) => ({ value: item.id, label: item.fullPath }))}
                value={destination.namespace?.id ?? null}
                onValueChange={(value) =>
                  onChange({ ...destination, namespace: items.find((item) => item.id === value) ?? null })
                }
              >
                <SelectTrigger className="w-full" aria-labelledby={`${ids}-ns-label`} aria-describedby={`${ids}-ns-help`}>
                  <SelectValue placeholder="Choose a namespace" />
                </SelectTrigger>
                <SelectContent>
                  {items.map((item) => (
                    <SelectItem key={item.id} value={item.id}>
                      {item.fullPath}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p id={`${ids}-ns-help`} className="text-xs text-muted-foreground">
                {permission ?? (items.length === 0 ? "This GitLab account has no namespaces you can use." : "Namespaces come from your GitLab account.")}
              </p>
              {namespaces.hasNextPage ? (
                <Button variant="outline" size="sm" className="w-fit" onClick={() => void namespaces.fetchNextPage()} disabled={namespaces.isFetchingNextPage}>
                  {namespaces.isFetchingNextPage ? "Loading…" : "Load more namespaces"}
                </Button>
              ) : null}
            </div>
            <div className="flex flex-col gap-1.5">
              <label id={`${ids}-vis-label`} className="text-[13px] font-medium">
                Project visibility
              </label>
              <Select
                items={[
                  { value: "private", label: "Private" },
                  { value: "public", label: "Public" },
                ]}
                value={destination.visibility}
                onValueChange={(value) => onChange({ ...destination, visibility: value === "public" ? "public" : "private" })}
              >
                <SelectTrigger className="w-full" aria-labelledby={`${ids}-vis-label`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="private">Private</SelectItem>
                  <SelectItem value="public">Public</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">Private is recommended.</p>
            </div>
          </div>
        )}
        {destination.visibility === "public" ? (
          <p className="flex items-start gap-2 rounded-lg bg-warning-soft px-3 py-2.5 text-[13px] text-warning">
            <Icon name="state.warning" className="mt-0.5" />
            <span>
              Public projects can be read by anyone. Backups of private GitHub repositories would become publicly visible on
              GitLab.
            </span>
          </p>
        ) : null}
      </div>
    </Card>
  )
}

export function ProviderStep({
  provider,
  account,
  accountsLoading,
  accountsError,
  onRetryAccounts,
  destination,
  onDestinationChange,
  onBack,
  onNext,
  onSelectStep,
}: {
  provider: Provider
  account: AccountSummary | null
  accountsLoading: boolean
  accountsError: NativeCommandError | null
  onRetryAccounts: () => void
  destination: DestinationState
  onDestinationChange: (next: DestinationState) => void
  onBack: () => void
  onNext: () => void
  onSelectStep: (step: Step) => void
}) {
  const label = PROVIDER_LABEL[provider]
  const connected = account?.authState === "connected"
  const needsNamespace = provider === "gitlab" && destination.namespace === null
  const blockedReason = accountsLoading
    ? "Checking connections…"
    : !connected
      ? account && account.authState !== "disconnected"
        ? `Reconnect ${label} to continue.`
        : `Connect ${label} to continue.`
      : needsNamespace
        ? "Choose a GitLab namespace to continue."
        : null

  return (
    <StepLayout
      step={provider === "github" ? 3 : 4}
      title={provider === "github" ? "Connect your source" : "Connect your backup destination"}
      description={provider === "github" ? "GitHub is read, never written" : "GitLab receives the mirrored backups"}
      onSelectStep={onSelectStep}
      onBack={onBack}
      blockedReason={blockedReason}
      primary={
        <Button onClick={onNext} disabled={blockedReason !== null} aria-describedby={blockedReason ? "onboarding-blocked-reason" : undefined}>
          Continue
          <Icon name="action.next" />
        </Button>
      }
    >
      {accountsError && !accountsLoading ? (
        <ErrorState error={accountsError.appError} title="Could not load connected accounts" onRetry={onRetryAccounts} />
      ) : null}
      {accountsLoading ? (
        <Card>
          <div className="px-5">
            <RowSkeletons rows={2} />
          </div>
        </Card>
      ) : (
        <ProviderCard provider={provider} account={account} />
      )}
      {provider === "gitlab" && connected && account ? (
        <DestinationCard accountId={account.id} destination={destination} onChange={onDestinationChange} />
      ) : null}
    </StepLayout>
  )
}
