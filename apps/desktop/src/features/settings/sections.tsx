import { Controller, useFormContext, useWatch } from "react-hook-form"
import { Link } from "react-router"

import { DIAGNOSTIC_STATUS, ErrorState, RowSkeletons, StatusBadge } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { NativeCommandError } from "@/lib/api"
import { relativeTime } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { Diagnostics, Settings } from "@/lib/native"

import { SelectControl, SwitchRow, type Option } from "./fields.tsx"
import {
  CLOSE_BEHAVIOR_LABEL,
  diagnosticLabel,
  intervalOptions,
  JOBS_MAX,
  JOBS_MIN,
  type TrayAvailability,
} from "./model.ts"
import { describedBy, ReadOnlyValue, RowGroupHeading, rowIds, SettingRow } from "./SettingRow.tsx"

function DisabledReason({ children }: { children: string }) {
  return (
    <span className="flex items-start gap-1.5 text-[12px] text-muted-foreground">
      <Icon name="status.blocked" size={14} className="mt-px" />
      <span>{children}</span>
    </span>
  )
}

export function GeneralSection({ tray }: { tray: TrayAvailability }) {
  const { control } = useFormContext<Settings>()
  return (
    <>
      <SwitchRow
        name="startMinimized"
        label="Start minimized"
        description="Open the window minimized when the app starts."
      />
      <Controller
        control={control}
        name="closeBehavior"
        render={({ field }) => {
          const hideBlocked = !tray.available
          const options: Option<Settings["closeBehavior"]>[] = (["ask", "quit", "hide_when_reopen_available"] as const).map(
            (value) => ({
              value,
              label:
                value === "hide_when_reopen_available" && hideBlocked
                  ? `${CLOSE_BEHAVIOR_LABEL[value]} (unavailable)`
                  : CLOSE_BEHAVIOR_LABEL[value],
              disabled: value === "hide_when_reopen_available" && hideBlocked,
            }),
          )
          return (
            <SettingRow
              name="closeBehavior"
              label="When the window is closed"
              description="Scans and syncs run only while the app is running."
              note={
                tray.available ? null : (
                  <DisabledReason>
                    {field.value === "hide_when_reopen_available"
                      ? `Hiding to the tray is saved but unavailable now: ${tray.reason}`
                      : `Hide to the system tray is unavailable: ${tray.reason}`}
                  </DisabledReason>
                )
              }
              control={
                <SelectControl
                  name="closeBehavior"
                  value={field.value}
                  options={options}
                  onChange={field.onChange}
                  onBlur={field.onBlur}
                />
              }
            />
          )
        }}
      />
    </>
  )
}

export function BackupSection() {
  return (
    <>
      <SettingRow
        name="backupFolder"
        labelFor={false}
        label="Backup folder"
        description="Moving the backup folder is not supported in this version."
        note={
          <span>
            Connections and destinations are managed in{" "}
            <Link to="/connections" className="font-medium text-primary underline-offset-4 hover:underline">
              Connections
            </Link>
            .
          </span>
        }
        control={<ReadOnlyValue>Set during setup</ReadOnlyValue>}
      />
      <SwitchRow
        name="workspaceEnabled"
        label="Keep working copies"
        description="Also keep a normal checkout of the current branch. Mirrors stay the authoritative backup, and local edits are never overwritten."
      />
      <RowGroupHeading
        title="Repository scope"
        description="Which repositories discovery includes when your token grants access to them."
      />
      <SwitchRow name="includePrivate" label="Private repositories" description="Include private repositories you own." />
      <SwitchRow name="includeForks" label="Forks" description="Include forks you own." />
      <SwitchRow name="includeArchived" label="Archived repositories" description="Include repositories archived on GitHub." />
      <SwitchRow
        name="includeCollaborations"
        label="Collaborations"
        description="Opt-in: repositories you collaborate on but do not own."
      />
      <SwitchRow
        name="includeOrganizations"
        label="Organization repositories"
        description="Opt-in: repositories owned by organizations you belong to."
      />
      <SettingRow
        name="retentionCleanup"
        labelFor={false}
        label="Retention cleanup"
        description="Recovery snapshots taken before history changes are never expired automatically in this version."
        control={<ReadOnlyValue>Manual</ReadOnlyValue>}
      />
    </>
  )
}

export function SyncSection({ onRequestAutoSync }: { onRequestAutoSync: () => void }) {
  const { control, register, formState } = useFormContext<Settings>()
  const autoScan = useWatch({ control, name: "autoScan" })
  const jobsError = formState.errors.maxConcurrentJobs?.message
  const intervalError = formState.errors.scanIntervalMinutes?.message
  return (
    <>
      <SwitchRow
        name="autoScan"
        label="Auto scan"
        description="Check GitHub for new repositories and changed refs while the app is running."
      />
      <Controller
        control={control}
        name="scanIntervalMinutes"
        render={({ field }) => (
          <SettingRow
            name="scanIntervalMinutes"
            label="Scan interval"
            description="How often an automatic scan runs. Default: every 30 minutes."
            note={autoScan ? null : <DisabledReason>Turn on auto scan to choose an interval.</DisabledReason>}
            error={intervalError}
            control={
              <SelectControl
                name="scanIntervalMinutes"
                value={field.value}
                options={intervalOptions(field.value)}
                onChange={field.onChange}
                onBlur={field.onBlur}
                disabled={!autoScan}
                invalid={intervalError !== undefined}
                hasError={intervalError !== undefined}
              />
            }
          />
        )}
      />
      <SwitchRow
        name="autoSync"
        label="Auto sync"
        description="Queue sync jobs for selected repositories when a scan finds changes. Plans that change existing history still wait for your review."
        onTurnOn={onRequestAutoSync}
      />
      <SettingRow
        name="maxConcurrentJobs"
        label="Max concurrent jobs"
        description={`Git jobs that may run at the same time (${JOBS_MIN}–${JOBS_MAX}). Default: 3.`}
        error={jobsError}
        control={
          <Input
            id={rowIds("maxConcurrentJobs").control}
            type="number"
            inputMode="numeric"
            min={JOBS_MIN}
            max={JOBS_MAX}
            step={1}
            className="w-24 text-[13px] tabular-nums md:text-[13px]"
            aria-invalid={jobsError !== undefined || undefined}
            aria-describedby={describedBy("maxConcurrentJobs", true, jobsError !== undefined)}
            {...register("maxConcurrentJobs", { valueAsNumber: true })}
          />
        }
      />
    </>
  )
}

export function NotificationsSection() {
  return (
    <>
      <SwitchRow
        name="notifyErrors"
        label="Failures and items that need attention"
        description="Notify when a scan or sync fails or a plan needs your review."
      />
      <SwitchRow
        name="notifySuccess"
        label="Successful syncs"
        description="Notify when syncs complete. Off by default to avoid frequent notifications."
      />
    </>
  )
}

export function AppearanceSection() {
  return (
    <SettingRow
      name="theme"
      label="Theme"
      description="Light is the only reviewed theme."
      note={<DisabledReason>Dark and system themes are not available in this version.</DisabledReason>}
      control={
        <SelectControl
          name="theme"
          value="light"
          options={[{ value: "light", label: "Light" }]}
          onChange={() => undefined}
          disabled
        />
      }
    />
  )
}

export function AdvancedSection({
  diagnostics,
  diagnosticsError,
  loading,
  fetching,
  onRecheck,
}: {
  diagnostics: Diagnostics | undefined
  diagnosticsError: NativeCommandError | null
  loading: boolean
  fetching: boolean
  onRecheck: () => void
}) {
  const checked = relativeTime(diagnostics?.observedAt)
  return (
    <>
      <SettingRow
        name="telemetry"
        labelFor={false}
        label="Telemetry"
        description="This app collects no telemetry or usage data about your accounts or repositories."
        control={<ReadOnlyValue>Off</ReadOnlyValue>}
      />
      <SettingRow
        name="logs"
        labelFor={false}
        label="Job logs"
        description="Logs are kept per job with credentials redacted. Open a job from Activity to read its log."
        control={
          <Button variant="outline" render={<Link to="/activity" />} nativeButton={false}>
            <Icon name="nav.activity" />
            Open Activity
          </Button>
        }
      />
      <section aria-labelledby="diagnostics-heading" className="pb-2">
        <div className="flex flex-wrap items-end justify-between gap-3 pt-6 pb-3">
          <div className="min-w-0">
            <h3 id="diagnostics-heading" className="text-[15px] font-semibold">
              Environment diagnostics
            </h3>
            <p className="mt-0.5 text-[13px] text-muted-foreground">
              Tools and desktop services this app depends on.{checked ? ` Checked ${checked}.` : ""}
            </p>
          </div>
          <Button variant="outline" onClick={onRecheck} disabled={fetching}>
            <Icon name="action.retry" />
            {fetching ? "Checking…" : "Check again"}
          </Button>
        </div>
        {loading ? (
          <RowSkeletons rows={4} className="py-2" />
        ) : diagnostics === undefined ? (
          <ErrorState
            error={diagnosticsError?.appError ?? null}
            title="Could not check the environment"
            onRetry={onRecheck}
            retrying={fetching}
          />
        ) : diagnostics.items.length === 0 ? (
          <p className="py-3 text-[13px] text-muted-foreground">No diagnostics were reported.</p>
        ) : (
          <ul className="divide-y rounded-lg border">
            {diagnostics.items.map((item) => (
              <li key={item.key} className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 px-4 py-3">
                <div className="min-w-0 flex-[1_1_16rem]">
                  <p className="text-sm font-medium">{diagnosticLabel(item)}</p>
                  <p className="mt-0.5 text-[13px] break-words text-muted-foreground ui-selectable">{item.detail}</p>
                </div>
                <StatusBadge status={DIAGNOSTIC_STATUS[item.status]} />
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  )
}
