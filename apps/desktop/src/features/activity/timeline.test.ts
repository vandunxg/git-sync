import { describe, expect, it } from "vitest"

import type { ActivityEntry } from "@/lib/native"

import { groupByDay, parseSeverity, parseType } from "./timeline.ts"

const entry = (id: string, occurredAt: string): ActivityEntry => ({
  id,
  repositoryId: null,
  jobId: null,
  occurredAt,
  severity: "info",
  type: "system",
  message: id,
  errorCode: null,
})

describe("activity timeline", () => {
  it("groups by local day in native order", () => {
    const now = new Date(2026, 9, 2, 12).getTime()
    const groups = groupByDay(
      [entry("a", new Date(2026, 9, 2, 11).toISOString()), entry("b", new Date(2026, 9, 2, 9).toISOString()), entry("c", new Date(2026, 9, 1, 9).toISOString())],
      now,
    )
    expect(groups.map((group) => [group.heading, group.entries.map((item) => item.id)])).toEqual([
      ["Today", ["a", "b"]],
      ["Yesterday", ["c"]],
    ])
  })

  it("rejects unknown URL filter values", () => {
    expect(parseSeverity("warning")).toBe("warning")
    expect(parseSeverity("bogus")).toBe("all")
    expect(parseType("auth")).toBe("auth")
    expect(parseType(null)).toBe("all")
  })
})
