import { z } from "zod"

import type { IconKey } from "@/lib/icons"
import type { Diagnostic, Diagnostics, Settings } from "@/lib/native"

export const SETTINGS_SECTIONS = ["general", "backup", "sync", "notifications", "appearance", "advanced"] as const
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]

export function isSettingsSection(value: string | undefined): value is SettingsSection {
  return SETTINGS_SECTIONS.some((section) => section === value)
}

export const SECTION_META: Record<SettingsSection, { label: string; icon: IconKey; description: string }> = {
  general: { label: "General", icon: "setting.general", description: "Startup and window behavior" },
  backup: { label: "Backup", icon: "setting.backup", description: "Backup folder, working copies and repository scope" },
  sync: { label: "Sync", icon: "nav.sync", description: "Automatic scans, syncs and job limits while the app runs" },
  notifications: { label: "Notifications", icon: "setting.notifications", description: "Which outcomes show a desktop notification" },
  appearance: { label: "Appearance", icon: "setting.appearance", description: "Visual theme" },
  advanced: { label: "Advanced", icon: "setting.advanced", description: "Diagnostics, logs and data collection" },
}

// Bounds mirror contracts/dtos.schema.json (Settings) and PRODUCT.md default policies.
export const SCAN_INTERVAL_MIN = 5
export const SCAN_INTERVAL_MAX = 1440
export const JOBS_MIN = 1
export const JOBS_MAX = 8

const JOBS_ERROR = `Enter a whole number from ${JOBS_MIN} to ${JOBS_MAX}.`
const INTERVAL_ERROR = `Choose an interval from ${SCAN_INTERVAL_MIN} minutes to 24 hours.`

/** Client-side validation of the whole Settings DTO; Rust validates again. */
export const settingsSchema = z.object({
  scanIntervalMinutes: z
    .int({ error: INTERVAL_ERROR })
    .min(SCAN_INTERVAL_MIN, { error: INTERVAL_ERROR })
    .max(SCAN_INTERVAL_MAX, { error: INTERVAL_ERROR }),
  autoScan: z.boolean(),
  autoSync: z.boolean(),
  maxConcurrentJobs: z.int({ error: JOBS_ERROR }).min(JOBS_MIN, { error: JOBS_ERROR }).max(JOBS_MAX, { error: JOBS_ERROR }),
  includePrivate: z.boolean(),
  includeForks: z.boolean(),
  includeArchived: z.boolean(),
  includeCollaborations: z.boolean(),
  includeOrganizations: z.boolean(),
  workspaceEnabled: z.boolean(),
  notifyErrors: z.boolean(),
  notifySuccess: z.boolean(),
  startMinimized: z.boolean(),
  closeBehavior: z.enum(["ask", "quit", "hide_when_reopen_available"]),
  theme: z.literal("light"),
  retentionCleanup: z.literal("manual"),
  telemetry: z.literal(false),
})

// Compile-time parity: the schema must describe exactly the generated Settings DTO.
type Exact<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false
const schemaMatchesContract: Exact<z.infer<typeof settingsSchema>, Settings> = true
void schemaMatchesContract

const STANDARD_INTERVALS = [15, 30, 60, 120, 360, 720, 1440] as const

export function intervalLabel(minutes: number): string {
  if (minutes === 1440) return "Once a day"
  if (minutes === 60) return "Every hour"
  if (minutes % 60 === 0) return `Every ${minutes / 60} hours`
  return `Every ${minutes} minutes`
}

/** Standard choices, plus the persisted value when it is not one of them (never silently changed). */
export function intervalOptions(current: number): { value: number; label: string }[] {
  const values: number[] = [...STANDARD_INTERVALS]
  if (Number.isInteger(current) && !values.includes(current)) values.push(current)
  return values.sort((a, b) => a - b).map((value) => ({ value, label: intervalLabel(value) }))
}

export const CLOSE_BEHAVIOR_LABEL: Record<Settings["closeBehavior"], string> = {
  ask: "Ask every time",
  quit: "Quit the app",
  hide_when_reopen_available: "Hide to the system tray",
}

export type TrayAvailability = { available: true } | { available: false; reason: string }

/** Close-to-tray is allowed only with a verified reopen path (AUTH_AND_LIFECYCLE.md). */
export function trayAvailability(diagnostics: Diagnostics | undefined, loading: boolean): TrayAvailability {
  if (diagnostics === undefined) {
    return { available: false, reason: loading ? "Checking whether a system tray is available." : "System tray availability could not be checked." }
  }
  const tray = diagnostics.items.find((item) => item.key.toLowerCase().includes("tray"))
  if (tray === undefined) return { available: false, reason: "This session did not report a system tray." }
  if (tray.status === "available") return { available: true }
  return { available: false, reason: tray.detail }
}

const DIAGNOSTIC_LABEL: Record<string, string> = {
  git: "Git",
  "git-lfs": "Git LFS",
  keyring: "System keyring",
  "storage-root": "Backup folder",
  portal: "File chooser portal",
  tray: "System tray",
  notifications: "Notification service",
}

export function diagnosticLabel(item: Diagnostic): string {
  return DIAGNOSTIC_LABEL[item.key] ?? item.key
}
