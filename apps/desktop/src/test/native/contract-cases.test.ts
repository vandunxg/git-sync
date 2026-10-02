// TEST-CONTRACT-FIXTURES / TEST-IPC (renderer side): the TS validator must give the
// oracle's verdict for every shared case in contracts/fixtures/cases.json, and the
// envelope interpreter must never turn a malformed response into success.

import { describe, expect, it } from "vitest"

import { interpretResponse } from "@/lib/native/client.ts"
import { CASES } from "@/lib/native/generated/cases.ts"
import { COMMANDS } from "@/lib/native/generated/contract.ts"
import { validateDefinition } from "@/lib/native/validate.ts"

const byDefinition = new Map<string, typeof CASES>()
for (const c of CASES) byDefinition.set(c.def, [...(byDefinition.get(c.def) ?? []), c])

describe("shared contract cases", () => {
  it("covers every definition with valid and invalid cases", () => {
    expect(CASES.length).toBeGreaterThan(1500)
    for (const [def, cases] of byDefinition) {
      expect(cases.some((c) => c.valid), `${def} has a valid sample`).toBe(true)
      expect(cases.some((c) => !c.valid), `${def} has a rejected mutation`).toBe(true)
    }
  })

  for (const [def, cases] of byDefinition) {
    it(`${def}: validator verdicts match the oracle`, () => {
      const mismatches = cases
        .filter((c) => (validateDefinition(def, c.value).length === 0) !== c.valid)
        .map((c) => `${c.label} expected valid=${String(c.valid)}`)
      expect(mismatches).toEqual([])
    })
  }

  it("rejects unknown definitions", () => {
    expect(validateDefinition("NoSuchDto", {})).not.toEqual([])
  })
})

describe("result envelope interpretation", () => {
  const ack = CASES.find((c) => c.def === "Ack" && c.valid)?.value
  const appError = CASES.find((c) => c.def === "AppError" && c.valid)?.value

  it("accepts well-formed success and failure envelopes", () => {
    expect(interpretResponse("gbm_queue_set_paused", "r1", { ok: true, requestId: "r1", data: ack })).toEqual({
      ok: true,
      requestId: "r1",
      data: ack,
    })
    const failed = interpretResponse("gbm_queue_set_paused", "r1", { ok: false, requestId: null, error: appError })
    expect(failed.ok).toBe(false)
    expect(failed.ok ? null : failed.error).toEqual(appError)
  })

  const malformed: Array<[string, unknown]> = [
    ["both payloads", { ok: true, requestId: "r1", data: ack, error: appError }],
    ["neither payload", { ok: true, requestId: "r1" }],
    ["failure without error", { ok: false, requestId: "r1" }],
    ["wrong request id", { ok: true, requestId: "r2", data: ack }],
    ["invalid data", { ok: true, requestId: "r1", data: { revision: -1 } }],
    ["extra data field", { ok: true, requestId: "r1", data: { revision: 1, extra: true } }],
    ["ok not boolean", { ok: "true", requestId: "r1", data: ack }],
    ["invalid error", { ok: false, requestId: "r1", error: { code: "NOPE" } }],
    ["null", null],
    ["array", []],
    ["string", "ok"],
  ]
  it.each(malformed)("maps %s to CONTRACT_MISMATCH, never success", (_label, raw) => {
    const result = interpretResponse("gbm_queue_set_paused", "r1", raw)
    expect(result.ok).toBe(false)
    expect(result.ok ? null : result.error.code).toBe("CONTRACT_MISMATCH")
  })

  it("exposes exactly the 31 contract commands", () => {
    expect(Object.keys(COMMANDS)).toHaveLength(31)
    expect(Object.keys(COMMANDS).every((name) => name.startsWith("gbm_"))).toBe(true)
  })
})
