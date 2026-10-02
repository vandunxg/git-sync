import { dayHeading, dayKey } from "@/lib/format"
import type { ActivityEntry, ActivityRequest } from "@/lib/native"

export type SeverityFilter = ActivityRequest["severity"]
export type TypeFilter = ActivityEntry["type"] | "all"

export const SEVERITY_OPTIONS: SeverityFilter[] = ["all", "info", "warning", "error"]
export const TYPE_OPTIONS: TypeFilter[] = ["all", "scan", "sync", "auth", "project", "system"]

export function parseSeverity(value: string | null): SeverityFilter {
  return SEVERITY_OPTIONS.find((option) => option === value) ?? "all"
}

export function parseType(value: string | null): TypeFilter {
  return TYPE_OPTIONS.find((option) => option === value) ?? "all"
}

export interface DayGroup {
  key: string
  heading: string
  entries: ActivityEntry[]
}

/** Groups entries by local day, preserving the native (most recent first) order. */
export function groupByDay(entries: readonly ActivityEntry[], now: number = Date.now()): DayGroup[] {
  const groups: DayGroup[] = []
  const index = new Map<string, DayGroup>()
  for (const entry of entries) {
    const key = dayKey(entry.occurredAt)
    let group = index.get(key)
    if (!group) {
      group = { key, heading: dayHeading(entry.occurredAt, now), entries: [] }
      index.set(key, group)
      groups.push(group)
    }
    group.entries.push(entry)
  }
  return groups
}
