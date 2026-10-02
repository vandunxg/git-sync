import { Navigate, NavLink, useParams } from "react-router"

import { ErrorState, PageHeader, StaleNotice } from "@/components/shared"
import { Skeleton } from "@/components/ui/skeleton"
import { keys, useNativeQuery } from "@/lib/api"
import { Icon } from "@/lib/icons"
import { cn } from "@/lib/utils"

import { isSettingsSection, SECTION_META, SETTINGS_SECTIONS, type SettingsSection } from "./model.ts"
import { SettingsForm } from "./SettingsForm.tsx"

export function SettingsPage() {
  const { section } = useParams()
  const settings = useNativeQuery(keys.settings, "gbm_settings_get", {})
  const diagnostics = useNativeQuery(keys.diagnostics, "gbm_environment_diagnostics", {})

  if (!isSettingsSection(section)) return <Navigate to="/settings/general" replace />

  return (
    <>
      <PageHeader title="Settings" description="Preferences with safe defaults. Changes apply when you save." />
      <div className="grid w-full max-w-[1120px] min-w-0 grid-cols-[200px_minmax(0,1fr)] items-start gap-[18px] xl:grid-cols-[220px_minmax(0,1fr)]">
        <SectionNav active={section} />
        <div className="flex min-w-0 flex-col gap-4">
          {settings.isError && settings.data ? (
            <StaleNotice
              observedAt={null}
              error={settings.error.appError}
              onRetry={() => void settings.refetch()}
              retrying={settings.isFetching}
            />
          ) : null}
          {settings.data ? (
            <SettingsForm
              section={section}
              result={settings.data}
              diagnostics={{
                data: diagnostics.data,
                error: diagnostics.error,
                loading: diagnostics.isPending,
                fetching: diagnostics.isFetching,
                recheck: () => void diagnostics.refetch(),
              }}
              onReload={async () => (await settings.refetch()).data}
            />
          ) : settings.isError ? (
            <div className="rounded-[10px] border bg-card p-5">
              <ErrorState
                error={settings.error.appError}
                title="Could not load settings"
                onRetry={() => void settings.refetch()}
                retrying={settings.isFetching}
              />
            </div>
          ) : (
            <PanelSkeleton />
          )}
        </div>
      </div>
    </>
  )
}

function SectionNav({ active }: { active: SettingsSection }) {
  return (
    <nav aria-label="Settings sections" className="rounded-[10px] border bg-card p-2">
      <ul className="flex flex-col gap-1">
        {SETTINGS_SECTIONS.map((id) => {
          const meta = SECTION_META[id]
          return (
            <li key={id}>
              <NavLink
                to={`/settings/${id}`}
                className={({ isActive }) =>
                  cn(
                    "flex h-10 items-center gap-3 rounded-lg px-3 text-[13px] outline-none focus-visible:ring-3 focus-visible:ring-ring/50",
                    isActive || id === active
                      ? "bg-muted font-semibold text-foreground"
                      : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                  )
                }
              >
                <Icon name={meta.icon} />
                <span className="truncate">{meta.label}</span>
              </NavLink>
            </li>
          )
        })}
      </ul>
    </nav>
  )
}

function PanelSkeleton() {
  return (
    <div className="rounded-[10px] border bg-card" aria-busy="true">
      <span className="sr-only" role="status">
        Loading settings…
      </span>
      <div className="flex items-center gap-3 border-b px-5 pt-5 pb-4" aria-hidden="true">
        <Skeleton className="size-8 rounded-lg" />
        <div className="flex flex-col gap-1.5">
          <Skeleton className="h-4 w-28" />
          <Skeleton className="h-3 w-52" />
        </div>
      </div>
      <div className="divide-y px-5" aria-hidden="true">
        {Array.from({ length: 4 }, (_, index) => (
          <div key={index} className="flex min-h-16 items-center justify-between gap-8 py-4">
            <div className="flex flex-1 flex-col gap-1.5">
              <Skeleton className="h-3.5 w-40" />
              <Skeleton className="h-3 w-3/5" />
            </div>
            <Skeleton className="h-9 w-40" />
          </div>
        ))}
      </div>
      <div className="flex justify-end gap-2 border-t px-5 py-3" aria-hidden="true">
        <Skeleton className="h-9 w-20" />
        <Skeleton className="h-9 w-32" />
      </div>
    </div>
  )
}
