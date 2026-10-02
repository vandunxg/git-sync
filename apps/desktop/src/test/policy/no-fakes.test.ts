// TEST-NO-FAKES (source level, REQ-37/REQ-39) - a heuristic guard, not a proof: production
// modules must not import test
// fixtures or shared contract cases, must not fake progress/success with timers or
// randomness, and only lib/native may touch Tauri APIs (R-FE). The built-bundle check
// lives in quality/checks/no_fakes_dist.py.

import { describe, expect, it } from "vitest"

const sources = import.meta.glob<string>(["/src/**/*.{ts,tsx}", "!/src/test/**", "!/src/**/*.test.{ts,tsx}"], {
  query: "?raw",
  import: "default",
  eager: true,
})

const production = Object.entries(sources).map(([path, text]) => ({ path: path.replace(/^\//, ""), text }))

function offenders(predicate: (file: { path: string; text: string }) => boolean): string[] {
  return production.filter(predicate).map((file) => file.path)
}

describe("production source policy", () => {
  it("scans the production tree", () => {
    expect(production.map((f) => f.path)).toContain("src/main.tsx")
    expect(production.map((f) => f.path)).toContain("src/lib/native/client.ts")
  })

  it("does not import test code, fixtures or shared contract cases", () => {
    // Static `from "x"`, side-effect `import "x"` and dynamic `import("x")` specifiers.
    const specifier = /(?:\bfrom\s*|\bimport\s*\(?\s*)["'`]([^"'`]+)["'`]/g
    const forbidden = /(^|\/)(test|fixtures)(\/|$)|generated\/cases/
    expect(offenders((f) => [...f.text.matchAll(specifier)].some((m) => forbidden.test(m[1] ?? "")))).toEqual([])
  })

  it("uses no timers or randomness that could fake progress or success", () => {
    // Any mention, so aliasing (`const t = setTimeout`) and property access are caught too.
    expect(offenders((f) => /\b(setTimeout|setInterval)\b|Math\s*\.\s*random|["'`](setTimeout|setInterval)["'`]/.test(f.text))).toEqual([])
  })

  it("keeps Tauri APIs inside lib/native", () => {
    expect(
      offenders((f) => /@tauri-apps\//.test(f.text) && !f.path.startsWith("src/lib/native/")),
    ).toEqual([])
  })

  it("contains no debug panel or telemetry hooks", () => {
    expect(offenders((f) => /DebugPanel|ctui:debug|analytics|telemetry\s*\(/i.test(f.text))).toEqual([])
  })
})
