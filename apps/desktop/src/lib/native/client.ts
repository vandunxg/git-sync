// The only module that talks to Tauri IPC (R-FE). Every call sends one typed request
// envelope (arguments are validated natively, not here) and returns a discriminated
// result; transport failures and contract-violating responses become failures, never
// success. A failure may carry `requestId: null` only per the ipc.v1.json envelopeRule.

import { invoke } from "@tauri-apps/api/core"

import { COMMANDS, SCHEMA_VERSION } from "./generated/contract.ts"
import type { AppError, CommandMap, CommandName } from "./generated/contract.ts"
import { validateDefinition } from "./validate.ts"

export type NativeResult<T> =
  | { ok: true; requestId: string; data: T }
  | { ok: false; requestId: string | null; error: AppError }

export type CommandResult<C extends CommandName> = NativeResult<CommandMap[C]["response"]>

function localError(code: AppError["code"], message: string, requestId: string, details: string | null): AppError {
  return { code, message, retryable: false, retryAt: null, correlationId: requestId, details }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

/** Interprets a raw result envelope for `command`; exported for contract tests. */
export function interpretResponse<C extends CommandName>(
  command: C,
  requestId: string,
  raw: unknown,
): CommandResult<C> {
  const mismatch = (details: string): CommandResult<C> => ({
    ok: false,
    requestId,
    error: localError("CONTRACT_MISMATCH", "The application returned an unexpected response.", requestId, details),
  })
  if (!isRecord(raw)) return mismatch("result envelope is not an object")
  const keys = Object.keys(raw).sort().join(",")
  if (raw.ok === true) {
    if (keys !== "data,ok,requestId") return mismatch("success envelope fields")
    if (raw.requestId !== requestId) return mismatch("requestId mismatch")
    const responseType = COMMANDS[command].responseType
    const violations = validateDefinition(responseType, raw.data)
    if (violations.length > 0) return mismatch(`data does not match ${responseType}`)
    return { ok: true, requestId, data: raw.data as CommandMap[C]["response"] }
  }
  if (raw.ok === false) {
    if (keys !== "error,ok,requestId") return mismatch("failure envelope fields")
    if (raw.requestId !== requestId && raw.requestId !== null) return mismatch("requestId mismatch")
    if (validateDefinition("AppError", raw.error).length > 0) return mismatch("error does not match AppError")
    return { ok: false, requestId: raw.requestId, error: raw.error as AppError }
  }
  return mismatch("ok is not a boolean")
}

/** Sends one allowlisted command. Arguments are re-validated natively. */
export async function callNative<C extends CommandName>(
  command: C,
  args: CommandMap[C]["request"],
): Promise<CommandResult<C>> {
  const requestId = crypto.randomUUID()
  const request = { schemaVersion: SCHEMA_VERSION, requestId, args }
  let raw: unknown
  try {
    raw = await invoke<unknown>(command, { request })
  } catch {
    // Details of transport errors are not forwarded: they could echo request content.
    return {
      ok: false,
      requestId,
      error: localError("INTERNAL", "The native application did not respond.", requestId, null),
    }
  }
  return interpretResponse(command, requestId, raw)
}
