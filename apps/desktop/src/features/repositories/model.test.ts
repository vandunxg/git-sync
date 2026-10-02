import { describe, expect, it } from "vitest"

import { compareRefs, hasActiveFilters, parseFilters, primaryActionFor } from "./model.ts"

describe("compareRefs", () => {
  it("classifies same, differing, missing and destination-only refs", () => {
    const rows = compareRefs({
      source: [
        { name: "refs/heads/main", oid: "a".repeat(40) },
        { name: "refs/heads/dev", oid: "b".repeat(40) },
        { name: "refs/tags/v1", oid: "c".repeat(40) },
      ],
      destination: [
        { name: "refs/heads/main", oid: "a".repeat(40) },
        { name: "refs/heads/dev", oid: "d".repeat(40) },
        { name: "refs/heads/old", oid: "e".repeat(40) },
      ],
    })
    expect(rows.map((row) => [row.name, row.comparison])).toEqual([
      ["refs/heads/main", "same"],
      ["refs/heads/dev", "differs"],
      ["refs/tags/v1", "missing_destination"],
      ["refs/heads/old", "only_destination"],
    ])
  })
})

describe("parseFilters", () => {
  it("falls back to defaults for unknown URL values", () => {
    const filters = parseFilters(new URLSearchParams("status=bogus&visibility=x&sort=y"))
    expect(filters).toEqual({ search: "", status: null, visibility: "all", sort: "updated_desc", selectedOnly: false })
    expect(hasActiveFilters(filters)).toBe(false)
  })

  it("reads valid values", () => {
    const filters = parseFilters(new URLSearchParams("q=api&status=blocked&visibility=private&sort=name_asc&included=1"))
    expect(filters).toEqual({ search: "api", status: "blocked", visibility: "private", sort: "name_asc", selectedOnly: true })
    expect(hasActiveFilters(filters)).toBe(true)
  })
})

describe("primaryActionFor", () => {
  it("prefers reconnect for auth errors, then review/retry/sync by status", () => {
    expect(primaryActionFor({ primaryStatus: "failed", lastErrorCode: "AUTH_REQUIRED" })).toBe("reconnect")
    expect(primaryActionFor({ primaryStatus: "blocked", lastErrorCode: "APPROVAL_REQUIRED" })).toBe("review")
    expect(primaryActionFor({ primaryStatus: "failed", lastErrorCode: "PUSH_PARTIAL" })).toBe("retry")
    expect(primaryActionFor({ primaryStatus: "running", lastErrorCode: null })).toBe("running")
    expect(primaryActionFor({ primaryStatus: "update", lastErrorCode: null })).toBe("sync")
  })
})
