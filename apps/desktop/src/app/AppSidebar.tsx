import { NavLink, useLocation } from "react-router"

import { IconTile } from "@/components/shared"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { keys, useNativePort, useNativeQuery } from "@/lib/api"
import { relativeTime } from "@/lib/format"
import { Icon, type IconKey } from "@/lib/icons"

import { QueueStatus } from "./QueueStatus.tsx"

const NAV: { to: string; label: string; icon: IconKey; end?: boolean }[] = [
  { to: "/", label: "Dashboard", icon: "nav.dashboard", end: true },
  { to: "/repositories", label: "Repositories", icon: "nav.repositories" },
  { to: "/sync", label: "Sync Center", icon: "nav.sync" },
  { to: "/activity", label: "Activity", icon: "nav.activity" },
  { to: "/connections", label: "Connections", icon: "nav.connections" },
  { to: "/settings", label: "Settings", icon: "nav.settings" },
]

export function AppSidebar() {
  const { pathname } = useLocation()
  const port = useNativePort()
  const dashboard = useNativeQuery(keys.dashboard, "gbm_dashboard_get", {})
  const attention = dashboard.data?.needsAttention ?? 0

  return (
    <Sidebar collapsible="none" className="h-full w-(--sidebar-width) border-r border-sidebar-border">
      <SidebarHeader className="px-4 pt-5 pb-3">
        <div className="flex items-center gap-3 border-b border-sidebar-border pb-4">
          <IconTile icon="nav.repositories" tone="info" />
          <div className="min-w-0 leading-tight">
            <p className="truncate text-[15px] font-semibold">Git Backup</p>
            <p className="truncate text-xs text-muted-foreground">Manager</p>
          </div>
        </div>
      </SidebarHeader>
      <SidebarContent className="px-2">
        <SidebarGroup>
          <SidebarGroupContent>
            <nav aria-label="Main">
              <SidebarMenu className="gap-1">
                {NAV.map((item) => {
                  const active = item.end ? pathname === item.to : pathname.startsWith(item.to)
                  return (
                    <SidebarMenuItem key={item.to}>
                      <SidebarMenuButton
                        isActive={active}
                        className="h-10 gap-3 px-3 text-[13px] data-active:font-semibold"
                        render={<NavLink to={item.to} end={item.end} />}
                      >
                        <Icon name={item.icon} />
                        <span>{item.label}</span>
                      </SidebarMenuButton>
                      {item.to === "/repositories" && attention > 0 ? (
                        <SidebarMenuBadge className="rounded-md bg-warning-soft text-warning peer-data-[size=default]/menu-button:top-2.5">
                          <span className="sr-only">Needs attention: </span>
                          {attention}
                        </SidebarMenuBadge>
                      ) : null}
                    </SidebarMenuItem>
                  )
                })}
              </SidebarMenu>
            </nav>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter className="gap-2 p-3">
        {port.kind === "fixture" ? (
          <p className="rounded-md bg-warning-soft px-2.5 py-1.5 text-xs font-medium text-warning">
            Demo data · fixture adapter
          </p>
        ) : null}
        <QueueStatus />
        {dashboard.data?.observedAt ? (
          <p className="px-1 text-xs text-muted-foreground">Last observed {relativeTime(dashboard.data.observedAt)}</p>
        ) : null}
      </SidebarFooter>
    </Sidebar>
  )
}
