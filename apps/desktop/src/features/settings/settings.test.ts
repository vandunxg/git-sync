import { describe, expect, it } from "vitest"

import type { Diagnostics, Settings } from "@/lib/native"

import { intervalOptions, isSettingsSection, settingsSchema, trayAvailability } from "./model.ts"

const BASE: Settings = {
  scanIntervalMinutes: 30,
  autoScan: true,
  autoSync: false,
  maxConcurrentJobs: 3,
  includePrivate: true,
  includeForks: true,
  includeArchived: true,
  includeCollaborations: false,
  includeOrganizations: false,
  workspaceEnabled: false,
  notifyErrors: true,
  notifySuccess: false,
  startMinimized: false,
  closeBehavior: "ask",
  theme: "light",
  retentionCleanup: "manual",
  telemetry: false,
}

const diagnostics = (items: Diagnostics["items"]): Diagnostics => ({ items, observedAt: "2026-10-02T00:00:00Z" })

describe("settings schema (TEST-UI-09)", () => {
  it("accepts the defaults", () => {
    expect(settingsSchema.safeParse(BASE).success).toBe(true)
  })

  it.each([0, 9, 2.5, Number.NaN])("rejects max concurrent jobs %s", (value) => {
    const result = settingsSchema.safeParse({ ...BASE, maxConcurrentJobs: value })
    expect(result.success).toBe(false)
    expect(result.error?.issues[0]?.message).toBe("Enter a whole number from 1 to 8.")
  })

  it.each([1, 8])("accepts max concurrent jobs %s", (value) => {
    expect(settingsSchema.safeParse({ ...BASE, maxConcurrentJobs: value }).success).toBe(true)
  })

  it.each([4, 1441, 7.5])("rejects scan interval %s", (value) => {
    expect(settingsSchema.safeParse({ ...BASE, scanIntervalMinutes: value }).success).toBe(false)
  })

  it("rejects telemetry and unreviewed themes", () => {
    expect(settingsSchema.safeParse({ ...BASE, telemetry: true }).success).toBe(false)
    expect(settingsSchema.safeParse({ ...BASE, theme: "dark" }).success).toBe(false)
  })
})

describe("interval options", () => {
  it("keeps a non-standard persisted value", () => {
    const values = intervalOptions(45).map((option) => option.value)
    expect(values).toContain(45)
    expect(values).toEqual([...values].sort((a, b) => a - b))
  })

  it("does not duplicate a standard value", () => {
    expect(intervalOptions(30).filter((option) => option.value === 30)).toHaveLength(1)
  })
})

describe("tray availability", () => {
  it("is available only when the tray diagnostic says so", () => {
    expect(trayAvailability(diagnostics([{ key: "tray", status: "available", detail: "ok" }]), false)).toEqual({ available: true })
    expect(trayAvailability(diagnostics([{ key: "tray-host", status: "missing", detail: "No host" }]), false)).toEqual({
      available: false,
      reason: "No host",
    })
    expect(trayAvailability(diagnostics([]), false).available).toBe(false)
    expect(trayAvailability(undefined, true).available).toBe(false)
  })
})

describe("sections", () => {
  it("recognizes only the six sections", () => {
    expect(isSettingsSection("advanced")).toBe(true)
    expect(isSettingsSection("theme")).toBe(false)
    expect(isSettingsSection(undefined)).toBe(false)
  })
})
