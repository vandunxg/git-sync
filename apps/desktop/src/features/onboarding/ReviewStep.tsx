import { useId, useState, type ReactNode } from "react"

import { ErrorState, RowSkeletons, SectionCard, StatusBadge, type StatusDescriptor } from "@/components/shared"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Spinner } from "@/components/ui/spinner"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { keys, repositoryStateKeys, useNativeMutation, useNativeQuery } from "@/lib/api"
import { clockTime, formatBytes, plural } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { AccountSummary, DestinationPreviewItem, RootSummary, Settings } from "@/lib/native"

import type { DestinationState } from "./ProviderStep.tsx"
import { StepLayout } from "./StepLayout.tsx"
import type { Step } from "./Stepper.tsx"

function planStatus(item: DestinationPreviewItem): StatusDescriptor {
  if (item.needsCreation) return { label: "Create new project", icon: "event.created", tone: "info" }
  if (item.existingProjectId !== null && item.canAdopt) return { label: "Adopt existing", icon: "action.reconnect", tone: "warning" }
  if (item.existingProjectId !== null) return { label: "Conflict — not adopted", icon: "status.blocked", tone: "danger" }
  return { label: "No change", icon: "status.no_changes", tone: "neutral" }
}

function SummaryRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-1 gap-0.5 py-2.5 sm:grid-cols-[180px_minmax(0,1fr)] sm:gap-4">
      <dt className="text-[13px] text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-[13px]">{children}</dd>
    </div>
  )
}

export function ReviewStep({
  root,
  github,
  gitlab,
  destination,
  settings,
  onBack,
  onFinished,
  onSelectStep,
}: {
  root: RootSummary
  github: AccountSummary
  gitlab: AccountSummary
  destination: DestinationState & { namespace: NonNullable<DestinationState["namespace"]> }
  settings: Settings | undefined
  onBack: () => void
  onFinished: () => void
  onSelectStep: (step: Step) => void
}) {
  const createId = useId()
  const [allowCreate, setAllowCreate] = useState(false)
  const namespace = destination.namespace
  const preview = useNativeQuery(
    ["destination-preview", gitlab.id, namespace.id, destination.visibility],
    "gbm_destination_preview",
    { repositoryIds: [], gitlabAccountId: gitlab.id, namespaceId: namespace.id, visibility: destination.visibility },
    { staleTime: Infinity, refetchOnWindowFocus: false },
  )
  const commit = useNativeMutation("gbm_destination_commit", [keys.bootstrap, keys.accounts, ...repositoryStateKeys])
  const items = preview.data?.items ?? []
  const toCreate = items.filter((item) => item.needsCreation).length
  const createForbidden = namespace.canCreateProject === false && toCreate > 0

  const finish = () => {
    if (!preview.data) return
    commit.mutate(
      { previewId: preview.data.previewId, expectedDigest: preview.data.digest, allowCreateMissing: allowCreate && !createForbidden },
      { onSuccess: onFinished },
    )
  }

  const blockedReason = preview.isPending
    ? "Preparing the destination preview…"
    : !preview.data
      ? "The destination preview is required before finishing."
      : null

  return (
    <StepLayout
      step={5}
      title="Review and start"
      description="Nothing is created on GitLab until you finish"
      onSelectStep={onSelectStep}
      onBack={onBack}
      blockedReason={blockedReason}
      primary={
        <Button onClick={finish} disabled={blockedReason !== null || commit.isPending} aria-describedby={blockedReason ? "onboarding-blocked-reason" : undefined}>
          {commit.isPending ? (
            <>
              <Spinner aria-hidden="true" />
              Finishing…
            </>
          ) : (
            <>
              <Icon name="status.synced" />
              Finish setup
            </>
          )}
        </Button>
      }
    >
      <SectionCard icon="setting.general" tone="info" title="Configuration" description="What this setup saves on this device.">
        <dl className="divide-y">
          <SummaryRow label="Backup folder">
            <span className="font-mono ui-selectable">{root.displayPath}</span>
            <span className="text-muted-foreground">
              {" "}
              · {formatBytes(root.availableBytes) ?? "Unknown"} free · working copy {root.workspaceEnabled ? "on" : "off"}
            </span>
          </SummaryRow>
          <SummaryRow label="Source (read only)">GitHub · {github.username}</SummaryRow>
          <SummaryRow label="Destination">
            GitLab · {gitlab.username} · <span className="font-mono ui-selectable">{namespace.fullPath}</span>
          </SummaryRow>
          <SummaryRow label="Project visibility">{destination.visibility === "private" ? "Private" : "Public"}</SummaryRow>
          <SummaryRow label="Automatic sync">
            {settings ? (settings.autoSync ? "On" : "Off") : "Unknown"}
            <span className="text-muted-foreground"> · change later in Settings</span>
          </SummaryRow>
        </dl>
      </SectionCard>

      <SectionCard
        icon="event.created"
        tone="info"
        title="Planned GitLab projects"
        description={
          preview.data
            ? `${plural(items.length, "repository", "repositories")} currently in scope${clockTime(preview.data.expiresAt) ? ` · preview valid until ${clockTime(preview.data.expiresAt)}` : ""}`
            : "Where each repository in scope will be backed up."
        }
        action={
          <Button variant="outline" onClick={() => void preview.refetch()} disabled={preview.isFetching}>
            <Icon name="action.retry" />
            {preview.isFetching && !preview.isPending ? "Refreshing…" : "Refresh preview"}
          </Button>
        }
        contentClassName="flex flex-col gap-4"
      >
        {preview.isPending ? (
          <RowSkeletons rows={4} />
        ) : preview.isError ? (
          <ErrorState error={preview.error.appError} title="Could not prepare the destination preview" onRetry={() => void preview.refetch()} retrying={preview.isFetching} />
        ) : items.length === 0 ? (
          <p className="rounded-lg border border-dashed px-4 py-4 text-[13px] text-muted-foreground">
            No repositories are in scope yet. Run the first scan after setup to discover repositories from GitHub.
          </p>
        ) : (
          <div className="max-h-[320px] overflow-auto rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>GitLab project path</TableHead>
                  <TableHead className="w-[200px]">Plan</TableHead>
                  <TableHead>Notes</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item) => (
                  <TableRow key={item.repositoryId}>
                    <TableCell className="font-mono text-[13px] break-all whitespace-normal">
                      {item.intendedPath}
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={planStatus(item)} />
                    </TableCell>
                    <TableCell className="text-xs whitespace-normal text-muted-foreground">
                      {item.warnings.length > 0 ? (
                        <ul className="flex flex-col gap-1">
                          {item.warnings.map((warning) => (
                            <li key={warning} className="flex gap-1.5 text-warning">
                              <Icon name="state.warning" size={14} className="mt-px" />
                              <span>{warning}</span>
                            </li>
                          ))}
                        </ul>
                      ) : (
                        "—"
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}

        {preview.data && toCreate > 0 ? (
          <div className="flex items-start gap-3 rounded-lg border px-4 py-3">
            <Checkbox
              id={createId}
              checked={allowCreate && !createForbidden}
              disabled={createForbidden}
              onCheckedChange={(checked) => setAllowCreate(checked)}
              className="mt-0.5"
              aria-describedby={`${createId}-help`}
            />
            <div className="flex flex-col gap-0.5">
              <label htmlFor={createId} className="text-[13px] font-medium">
                Create missing GitLab projects
              </label>
              <p id={`${createId}-help`} className="text-xs text-muted-foreground">
                {createForbidden
                  ? "Your GitLab account cannot create projects in this namespace. Choose another namespace to create projects."
                  : `Creates ${plural(toCreate, "project")} as ${destination.visibility} in ${namespace.fullPath} when you finish. If unchecked, nothing is created and those repositories show as not created.`}
              </p>
            </div>
          </div>
        ) : null}

        <Alert role="note" className="border-border bg-muted/50 px-4 py-3">
          <Icon name="status.healthy" className="text-primary" />
          <AlertTitle className="text-[13px] font-semibold">Safe mirror policy</AlertTitle>
          <AlertDescription className="text-xs">
            Fast-forward updates are mirrored automatically. History rewrites, moved tags and deleted branches are never pushed
            without your review, and a recovery snapshot is taken first. Existing projects are adopted only after you confirm.
          </AlertDescription>
        </Alert>

        {commit.error ? (
          <ErrorState
            error={commit.error.appError}
            title="Setup was not finished"
            action={
              commit.error.appError.code === "PLAN_STALE" ? (
                <Button variant="outline" onClick={() => void preview.refetch()}>
                  Refresh preview
                </Button>
              ) : undefined
            }
          />
        ) : null}
      </SectionCard>
    </StepLayout>
  )
}
