import { createHashRouter, Navigate } from "react-router"

import { ActivityPage } from "@/features/activity"
import { ConnectionsPage } from "@/features/connections"
import { DashboardPage } from "@/features/dashboard"
import { OnboardingPage } from "@/features/onboarding"
import { RepositoriesPage, RepositoryDetailPage } from "@/features/repositories"
import { SettingsPage } from "@/features/settings"
import { SyncCenterPage } from "@/features/sync"

import { AppLayout } from "./AppLayout.tsx"
import { NotFound } from "./NotFound.tsx"

// Hash routing: the Tauri asset protocol serves a single index.html.
export function createAppRouter() {
  return createHashRouter([
    { path: "/onboarding", element: <OnboardingPage /> },
    {
      element: <AppLayout />,
      children: [
        { index: true, element: <DashboardPage /> },
        { path: "repositories", element: <RepositoriesPage /> },
        { path: "repositories/:repositoryId", element: <RepositoryDetailPage /> },
        { path: "repositories/:repositoryId/:tab", element: <RepositoryDetailPage /> },
        { path: "sync", element: <SyncCenterPage /> },
        { path: "activity", element: <ActivityPage /> },
        { path: "connections", element: <ConnectionsPage /> },
        { path: "settings", element: <Navigate to="/settings/general" replace /> },
        { path: "settings/:section", element: <SettingsPage /> },
        { path: "*", element: <NotFound /> },
      ],
    },
  ])
}
