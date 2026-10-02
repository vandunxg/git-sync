// TEST-ICONS (REQ-24/REQ-25): the semantic registry matches contracts/icons.json, every
// export resolves in the installed lucide-react, and production sources contain no
// Unicode status/action glyph stand-ins.

import * as lucide from "lucide-react"
import { describe, expect, it } from "vitest"

import contract from "../../../../../contracts/icons.json"
import { ICONS } from "@/lib/icons/registry"

const sources = import.meta.glob<string>(["/src/**/*.tsx", "!/src/test/**", "!/src/**/*.test.tsx"], {
  query: "?raw",
  import: "default",
  eager: true,
})

describe("semantic icon registry", () => {
  it("covers exactly the contract keys with the contracted exports", () => {
    expect(Object.keys(ICONS).sort()).toEqual(contract.entries.map((entry) => entry.key).sort())
    const library = lucide as unknown as Record<string, unknown>
    const mismatched = contract.entries.filter((entry) => library[entry.export] !== ICONS[entry.key as keyof typeof ICONS])
    expect(mismatched.map((entry) => entry.key)).toEqual([])
  })

  it("resolves every contracted export in the installed lucide-react", () => {
    const missing = contract.entries.filter((entry) => !(entry.export in lucide)).map((entry) => entry.export)
    expect(missing).toEqual([])
  })
})

describe("production sources", () => {
  it("use no Unicode status or action glyph placeholders", () => {
    const glyphs = [...contract.forbiddenStatusGlyphs, "⋯"]
    const offenders = Object.entries(sources)
      .filter(([, text]) => glyphs.some((glyph) => text.includes(glyph)))
      .map(([path]) => path)
    expect(offenders).toEqual([])
  })

  it("render no literal three-dot action labels", () => {
    // A JSX text node that is only "..." would be a fake action affordance.
    const offenders = Object.entries(sources)
      .filter(([, text]) => /> *\.\.\. *</.test(text))
      .map(([path]) => path)
    expect(offenders).toEqual([])
  })
})
