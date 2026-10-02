import { useId, useState } from "react"

import { ErrorState, SectionCard, StatusBadge } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Spinner } from "@/components/ui/spinner"
import { ERROR_COPY, keys, useNativeMutation } from "@/lib/api"
import { formatBytes } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { RootSummary, StorageCandidate } from "@/lib/native"

import { StepLayout } from "./StepLayout.tsx"
import type { Step } from "./Stepper.tsx"

export interface FolderState {
  candidate: StorageCandidate | null
  workspaceEnabled: boolean
  /** The native-confirmed root plus the inputs it was committed with. */
  committed: { candidateId: string; workspaceEnabled: boolean; root: RootSummary } | null
}

export const INITIAL_FOLDER: FolderState = { candidate: null, workspaceEnabled: false, committed: null }

export function isFolderCommitted(folder: FolderState): boolean {
  return (
    folder.committed !== null &&
    folder.candidate !== null &&
    folder.committed.candidateId === folder.candidate.candidateId &&
    folder.committed.workspaceEnabled === folder.workspaceEnabled
  )
}

export function FolderStep({
  folder,
  onChange,
  onBack,
  onNext,
  onSelectStep,
}: {
  folder: FolderState
  onChange: (next: FolderState) => void
  onBack: () => void
  onNext: () => void
  onSelectStep: (step: Step) => void
}) {
  const workspaceId = useId()
  const [canceled, setCanceled] = useState(false)
  const pick = useNativeMutation("gbm_storage_pick_root")
  const commit = useNativeMutation("gbm_storage_commit_root", [keys.diagnostics, keys.bootstrap])
  const candidate = folder.candidate
  const ready = isFolderCommitted(folder)
  const root = ready ? folder.committed?.root ?? null : null

  const choose = () => {
    setCanceled(false)
    commit.reset()
    pick.mutate(
      {},
      {
        onSuccess: (result) => {
          if (result.outcome === "canceled" || result.candidate === null) {
            setCanceled(true)
            return
          }
          onChange({ ...folder, candidate: result.candidate })
        },
      },
    )
  }

  const confirm = () => {
    if (!candidate) return
    commit.mutate(
      { candidateId: candidate.candidateId, workspaceEnabled: folder.workspaceEnabled },
      {
        onSuccess: (result) =>
          onChange({
            ...folder,
            committed: { candidateId: candidate.candidateId, workspaceEnabled: folder.workspaceEnabled, root: result },
          }),
      },
    )
  }

  const blockedReason = !candidate
    ? "Choose a backup folder to continue."
    : !candidate.valid
      ? "This folder cannot be used. Choose another folder."
      : null

  return (
    <StepLayout
      step={2}
      title="Choose a backup folder"
      description="Local mirrors are stored on this device"
      onSelectStep={onSelectStep}
      onBack={onBack}
      blockedReason={blockedReason}
      primary={
        ready ? (
          <Button onClick={onNext}>
            Continue
            <Icon name="action.next" />
          </Button>
        ) : (
          <Button
            onClick={confirm}
            disabled={!candidate?.valid || commit.isPending}
            aria-describedby={blockedReason ? "onboarding-blocked-reason" : undefined}
          >
            {commit.isPending ? (
              <>
                <Spinner aria-hidden="true" />
                Checking folder…
              </>
            ) : (
              <>
                Use this folder
                <Icon name="action.next" />
              </>
            )}
          </Button>
        )
      }
    >
      <SectionCard
        icon="action.open_folder"
        tone="info"
        title="Backup folder"
        description="Git Backup Manager keeps a verified bare mirror of every selected repository inside this folder."
        contentClassName="flex flex-col gap-4"
      >
        {candidate ? (
          <div className="flex flex-wrap items-center gap-3 rounded-lg border px-4 py-3">
            <Icon name="action.open_folder" className="text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate font-mono text-[13px] ui-selectable" title={candidate.displayPath}>
              {candidate.displayPath}
            </span>
            <StatusBadge
              status={
                candidate.valid
                  ? { label: "Usable", icon: "status.ready", tone: "success" }
                  : { label: "Cannot be used", icon: "status.failed", tone: "danger" }
              }
            />
            <Button variant="outline" onClick={choose} disabled={pick.isPending || commit.isPending}>
              {pick.isPending ? "Waiting for picker…" : "Choose another folder"}
            </Button>
          </div>
        ) : (
          <div className="flex flex-col items-start gap-3 rounded-lg border border-dashed px-4 py-5 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <p className="text-sm font-medium">No folder chosen yet</p>
              <p className="text-[13px] text-muted-foreground">Pick a folder on a disk with enough free space for your repositories.</p>
            </div>
            <Button onClick={choose} disabled={pick.isPending}>
              <Icon name="action.open_folder" />
              {pick.isPending ? "Waiting for picker…" : "Choose folder"}
            </Button>
          </div>
        )}

        {canceled ? (
          <p className="text-xs text-muted-foreground" role="status">
            Folder selection was canceled. Nothing changed.
          </p>
        ) : null}
        {pick.error ? <ErrorState error={pick.error.appError} title="The folder picker could not open" onRetry={choose} retrying={pick.isPending} /> : null}

        {candidate && candidate.errors.length > 0 ? (
          <ul className="flex flex-col gap-2 rounded-lg bg-danger-soft px-4 py-3 text-[13px] text-danger">
            {candidate.errors.map((error) => (
              <li key={error.correlationId} className="flex gap-2">
                <Icon name="state.error" className="mt-0.5" />
                <span>
                  {ERROR_COPY[error.code]} {error.message !== ERROR_COPY[error.code] ? error.message : null}
                </span>
              </li>
            ))}
          </ul>
        ) : null}

        <div className="flex items-start gap-3">
          <Checkbox
            id={workspaceId}
            checked={folder.workspaceEnabled}
            onCheckedChange={(checked) => onChange({ ...folder, workspaceEnabled: checked })}
            className="mt-0.5"
            aria-describedby={`${workspaceId}-help`}
          />
          <div className="flex flex-col gap-0.5">
            <label htmlFor={workspaceId} className="text-[13px] font-medium">
              Also keep a working copy (checked-out files)
            </label>
            <p id={`${workspaceId}-help`} className="text-xs text-muted-foreground">
              Adds browsable files next to each mirror. Local edits in a working copy are never overwritten.
            </p>
          </div>
        </div>

        {commit.error ? <ErrorState error={commit.error.appError} title="This folder was not accepted" onRetry={confirm} retrying={commit.isPending} /> : null}

        {root ? (
          <div className="flex flex-col gap-3 rounded-lg border border-success/30 bg-success-soft/60 px-4 py-3" role="status">
            <p className="flex items-center gap-2 text-sm font-semibold text-success">
              <Icon name="status.synced" />
              Backup folder ready
            </p>
            <dl className="grid grid-cols-1 gap-x-6 gap-y-2 text-[13px] sm:grid-cols-3">
              <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">Location</dt>
                <dd className="truncate font-mono ui-selectable" title={root.displayPath}>
                  {root.displayPath}
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">Free space</dt>
                <dd>{formatBytes(root.availableBytes) ?? "Unknown"}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">Working copy</dt>
                <dd>{root.workspaceEnabled ? "On" : "Off — mirrors only"}</dd>
              </div>
            </dl>
            <p className="text-xs text-muted-foreground">
              {root.ownershipVerified ? "Folder access verified. " : "Folder ownership could not be verified. "}
              Mirrors are managed by the app inside this folder.
            </p>
          </div>
        ) : null}
      </SectionCard>
    </StepLayout>
  )
}
