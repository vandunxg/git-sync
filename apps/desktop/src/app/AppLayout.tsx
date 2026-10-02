import { Navigate, Outlet, useLocation } from "react-router"

import { ErrorState } from "@/components/shared"
import { SidebarProvider } from "@/components/ui/sidebar"
import { Skeleton } from "@/components/ui/skeleton"
import { keys, useNativeQuery } from "@/lib/api"

import { AppSidebar } from "./AppSidebar.tsx"

/** Route guard: an unconfigured profile goes to onboarding, never a fake dashboard. */
export function useBootstrap() {
  return useNativeQuery(keys.bootstrap, "gbm_app_bootstrap", {}, { staleTime: 60_000 })
}

export function StartupFailure({ error, onRetry, retrying }: Parameters<typeof ErrorState>[0] & { onRetry: () => void; retrying: boolean }) {
  return (
    <div className="flex h-full items-center justify-center bg-background p-8">
      <div className="w-full max-w-lg">
        <h1 className="mb-2 text-xl font-semibold">Git Backup Manager could not start</h1>
        <p className="mb-4 text-sm text-muted-foreground">
          The native application did not return its configuration. No backup state is shown until it does.
        </p>
        <ErrorState error={error} title="Startup check failed" onRetry={onRetry} retrying={retrying} />
      </div>
    </div>
  )
}

export function AppLayout() {
  const bootstrap = useBootstrap()
  const location = useLocation()

  if (bootstrap.isPending) {
    return (
      <div className="flex h-full" aria-busy="true">
        <div className="w-[220px] border-r bg-sidebar p-4">
          <Skeleton className="h-9 w-full" />
        </div>
        <div className="flex-1 p-7">
          <Skeleton className="h-8 w-60" />
        </div>
      </div>
    )
  }
  if (bootstrap.isError) {
    return <StartupFailure error={bootstrap.error.appError} onRetry={() => void bootstrap.refetch()} retrying={bootstrap.isFetching} />
  }
  if (!bootstrap.data.onboardingComplete) {
    return <Navigate to="/onboarding" replace state={{ from: location.pathname }} />
  }

  return (
    <SidebarProvider className="h-full min-h-0">
      <AppSidebar />
      <main className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto bg-background" data-ui-scroll-container>
        <div className="mx-auto flex w-full max-w-[1600px] min-w-0 flex-col gap-5 px-7 py-7">
          <Outlet />
        </div>
      </main>
    </SidebarProvider>
  )
}
