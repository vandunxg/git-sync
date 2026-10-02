// Contract parity self-test for the renderer side (TEST-CONTRACT-FIXTURES, TEST-IPC).
// Run with: node apps/desktop/src/lib/native/contract-cases.selftest.ts (Node >= 24,
// native type stripping). P01-T03 moves this into the Vitest harness.

import { interpretResponse } from "./client.ts"
import { CASES as cases } from "./generated/cases.ts"
import { COMMANDS } from "./generated/contract.ts"
import { validateDefinition } from "./validate.ts"

const failures: string[] = []
const check = (condition: boolean, message: string) => {
  if (!condition) failures.push(message)
}

for (const c of cases) {
  const violations = validateDefinition(c.def, c.value)
  check((violations.length === 0) === c.valid, `${c.def} [${c.label}] expected valid=${c.valid}: ${JSON.stringify(violations)}`)
}

const ack = cases.find((c) => c.def === "Ack" && c.valid)?.value
const err = cases.find((c) => c.def === "AppError" && c.valid)?.value
check(ack !== undefined && err !== undefined, "fixture samples for Ack/AppError exist")
const ok = interpretResponse("gbm_queue_set_paused", "r1", { ok: true, requestId: "r1", data: ack })
check(ok.ok, "valid success envelope accepted")
const failed = interpretResponse("gbm_queue_set_paused", "r1", { ok: false, requestId: null, error: err })
check(!failed.ok && failed.error.code !== "CONTRACT_MISMATCH", "valid failure envelope accepted")
const bad: Array<[string, unknown]> = [
  ["both payloads", { ok: true, requestId: "r1", data: ack, error: err }],
  ["neither payload", { ok: true, requestId: "r1" }],
  ["wrong request id", { ok: true, requestId: "r2", data: ack }],
  ["invalid data", { ok: true, requestId: "r1", data: { revision: -1 } }],
  ["ok not boolean", { ok: "true", requestId: "r1", data: ack }],
  ["invalid error", { ok: false, requestId: "r1", error: { code: "NOPE" } }],
  ["not an object", "ok"],
]
for (const [label, raw] of bad) {
  const result = interpretResponse("gbm_queue_set_paused", "r1", raw)
  check(!result.ok && result.error.code === "CONTRACT_MISMATCH", `rejects ${label}`)
}
check(Object.keys(COMMANDS).length === 31, "31 commands in generated map")

if (failures.length > 0) {
  // An uncaught error gives a non-zero exit status under Node.
  throw new Error(`FAIL ${failures.length} of ${cases.length + bad.length + 4} checks\n${failures.slice(0, 40).join("\n")}`)
}
console.log(`PASS ${cases.length} shared cases + ${bad.length + 4} envelope checks`)
