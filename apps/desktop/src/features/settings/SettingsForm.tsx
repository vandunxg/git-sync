import { zodResolver } from "@hookform/resolvers/zod"
import { useQueryClient } from "@tanstack/react-query"
import { useEffect, useRef, useState } from "react"
import { FormProvider, useForm, useWatch } from "react-hook-form"
import { useBlocker } from "react-router"
import { toast } from "sonner"

import { ConfirmDialog, ErrorState, IconTile } from "@/components/shared"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { keys, useNativeMutation, type NativeCommandError } from "@/lib/api"
import { Icon } from "@/lib/icons"
import type { Diagnostics, Settings, SettingsResult } from "@/lib/native"

import { SECTION_META, settingsSchema, trayAvailability, type SettingsSection } from "./model.ts"
import {
  AdvancedSection,
  AppearanceSection,
  BackupSection,
  GeneralSection,
  NotificationsSection,
  SyncSection,
} from "./sections.tsx"

export type DiagnosticsState = {
  data: Diagnostics | undefined
  error: NativeCommandError | null
  loading: boolean
  fetching: boolean
  recheck: () => void
}

/**
 * One draft over the whole Settings DTO. Switching sections keeps the draft; Save sends
 * everything with the revision the draft was based on, so a concurrent change conflicts.
 */
export function SettingsForm({
  section,
  result,
  diagnostics,
  onReload,
}: {
  section: SettingsSection
  result: SettingsResult
  diagnostics: DiagnosticsState
  onReload: () => Promise<SettingsResult | undefined>
}) {
  const form = useForm<Settings>({
    resolver: zodResolver(settingsSchema),
    defaultValues: result.settings,
    mode: "onChange",
  })
  const { isDirty, isValid } = form.formState
  const baseRevision = useRef(result.revision)
  const queryClient = useQueryClient()
  const save = useNativeMutation("gbm_settings_update", [keys.bootstrap])
  const [confirmAutoSync, setConfirmAutoSync] = useState(false)
  const [reloading, setReloading] = useState(false)

  // Adopt a newer persisted revision (e.g. background refetch) only while there is no draft.
  useEffect(() => {
    if (result.revision !== baseRevision.current && !form.formState.isDirty) {
      baseRevision.current = result.revision
      form.reset(result.settings)
    }
  }, [form, result])

  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      isDirty && currentLocation.pathname !== nextLocation.pathname && !nextLocation.pathname.startsWith("/settings/"),
  )

  const saveDraft = (settings: Settings) => {
    if (save.isPending) return
    save.mutate(
      { settings, expectedRevision: baseRevision.current },
      {
        onSuccess: (next) => {
          // The response is the new persisted state; bootstrap is invalidated by the mutation hook.
          queryClient.setQueryData(keys.settings, next)
          baseRevision.current = next.revision
          form.reset(next.settings)
          toast.success("Settings saved")
        },
      },
    )
  }

  // Reset restores the persisted values; it never deletes caches or backups.
  const resetDraft = () => {
    baseRevision.current = result.revision
    form.reset(result.settings)
    save.reset()
  }

  const reloadLatest = async () => {
    setReloading(true)
    try {
      const latest = await onReload()
      if (latest) {
        baseRevision.current = latest.revision
        form.reset(latest.settings)
        save.reset()
      }
    } finally {
      setReloading(false)
    }
  }

  const draft = useWatch({ control: form.control })
  const meta = SECTION_META[section]
  const tray = trayAvailability(diagnostics.data, diagnostics.loading)
  const conflict = save.error?.appError.code === "PLAN_STALE"

  return (
    <FormProvider {...form}>
      <form
        noValidate
        aria-labelledby="settings-section-title"
        onSubmit={(event) => void form.handleSubmit(saveDraft)(event)}
        className="flex min-w-0 flex-col rounded-[10px] border border-border bg-card text-sm"
      >
        <header className="flex items-center gap-3 border-b px-5 pt-5 pb-4">
          <IconTile icon={meta.icon} tone="info" />
          <div className="min-w-0">
            <h2 id="settings-section-title" className="text-[15px] leading-snug font-semibold">
              {meta.label}
            </h2>
            <p className="text-[13px] text-muted-foreground">{meta.description}</p>
          </div>
        </header>

        <div className="flex flex-col divide-y px-5 pb-3">
          {section === "general" ? <GeneralSection tray={tray} /> : null}
          {section === "backup" ? <BackupSection /> : null}
          {section === "sync" ? <SyncSection onRequestAutoSync={() => setConfirmAutoSync(true)} /> : null}
          {section === "notifications" ? <NotificationsSection /> : null}
          {section === "appearance" ? <AppearanceSection /> : null}
          {section === "advanced" ? (
            <AdvancedSection
              diagnostics={diagnostics.data}
              diagnosticsError={diagnostics.error}
              loading={diagnostics.loading}
              fetching={diagnostics.fetching}
              onRecheck={diagnostics.recheck}
            />
          ) : null}
        </div>

        {save.isError ? (
          <div className="px-5 pb-4">
            {conflict ? (
              <Alert className="border-warning/30 bg-warning-soft px-4 py-3 text-warning">
                <Icon name="state.warning" />
                <AlertTitle className="text-sm font-semibold">Settings changed elsewhere</AlertTitle>
                <AlertDescription className="flex flex-col gap-2 text-[13px] text-warning/90">
                  <p>
                    Your changes were not saved because the settings were updated after you started editing. Reload the
                    latest settings, then apply your changes again.
                  </p>
                  <div>
                    <Button type="button" variant="outline" onClick={() => void reloadLatest()} disabled={reloading}>
                      <Icon name="action.retry" />
                      {reloading ? "Reloading…" : "Reload latest"}
                    </Button>
                  </div>
                </AlertDescription>
              </Alert>
            ) : (
              <ErrorState
                error={save.error.appError}
                title="Settings were not saved"
                action={<p className="self-center text-[13px]">Your edits are kept in this form.</p>}
              />
            )}
          </div>
        ) : null}

        <div className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-3 rounded-b-[10px] border-t bg-card px-5 py-3">
          <p className="flex items-center gap-2 text-[13px]" aria-live="polite">
            {isDirty ? (
              <>
                <Icon name="state.warning" className="text-warning" />
                <span className="font-medium">Unsaved changes</span>
              </>
            ) : (
              <>
                <Icon name="status.ready" className="text-muted-foreground" />
                <span className="text-muted-foreground">All changes saved</span>
              </>
            )}
          </p>
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" onClick={resetDraft} disabled={!isDirty || save.isPending}>
              <Icon name="action.retry" />
              Reset
            </Button>
            <Button type="submit" disabled={!isDirty || !isValid || save.isPending}>
              {save.isPending ? "Saving…" : "Save changes"}
            </Button>
          </div>
        </div>
      </form>

      <ConfirmDialog
        open={confirmAutoSync}
        onOpenChange={setConfirmAutoSync}
        title="Turn on auto sync?"
        icon="action.sync"
        description={
          <p>
            When a scan finds changes, the app will queue sync jobs for your selected repositories to their mapped GitLab
            projects and local mirrors, using the safe mirror policy. Plans that rewrite or delete existing history still
            wait for your review. Nothing changes until you save.
          </p>
        }
        confirmLabel="Turn on auto sync"
        onConfirm={() => {
          form.setValue("autoSync", true, { shouldDirty: true, shouldValidate: true })
          setConfirmAutoSync(false)
        }}
      >
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 rounded-lg border bg-muted/30 px-3 py-2.5">
          <dt className="text-muted-foreground">Also includes</dt>
          <dd>{scopeSummary(draft)}</dd>
          <dt className="text-muted-foreground">Auto scan</dt>
          <dd>{draft.autoScan ? `On, every ${draft.scanIntervalMinutes ?? "—"} minutes` : "Off — sync runs after manual scans"}</dd>
          <dt className="text-muted-foreground">Max jobs</dt>
          <dd className="tabular-nums">{Number.isFinite(draft.maxConcurrentJobs) ? draft.maxConcurrentJobs : "—"}</dd>
        </dl>
      </ConfirmDialog>

      <AlertDialog open={blocker.state === "blocked"} onOpenChange={(open) => (!open && blocker.state === "blocked" ? blocker.reset() : undefined)}>
        <AlertDialogContent className="data-[size=default]:sm:max-w-md">
          <AlertDialogHeader>
            <AlertDialogMedia className="bg-warning-soft text-warning">
              <Icon name="state.warning" size={20} />
            </AlertDialogMedia>
            <AlertDialogTitle className="font-semibold">Discard unsaved changes?</AlertDialogTitle>
            <AlertDialogDescription>
              Your settings changes have not been saved. Leaving this page discards them; saved settings and backups are not
              affected.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep editing</AlertDialogCancel>
            <Button variant="destructive" onClick={() => (blocker.state === "blocked" ? blocker.proceed() : undefined)}>
              Discard changes
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </FormProvider>
  )
}

function scopeSummary(draft: Partial<Settings>): string {
  const included = [
    draft.includePrivate ? "private" : null,
    draft.includeForks ? "forks" : null,
    draft.includeArchived ? "archived" : null,
    draft.includeCollaborations ? "collaborations" : null,
    draft.includeOrganizations ? "organizations" : null,
  ].filter((item) => item !== null)
  return included.length > 0 ? included.join(", ") : "No optional categories"
}
