// Runtime validator for the JSON Schema subset in contracts/dtos.schema.json.
// Same semantics as crates/domain/src/contract/schema.rs: string lengths count Unicode
// code points, patterns use ECMA-262 semantics, integers must be whole numbers, and
// date-time requires RFC 3339 with `Z` or a numeric offset.

import { DTO_SCHEMA } from "./generated/contract.ts"

export interface Violation {
  path: string
  reason: string
}

type Node = { readonly [key: string]: unknown }

const MAX_DEPTH = 80
const DEFS = DTO_SCHEMA.$defs as unknown as Record<string, Node>
const patternCache = new Map<string, RegExp>()

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function typeMatches(value: unknown, expected: unknown): boolean {
  switch (expected) {
    case "object":
      return isRecord(value)
    case "array":
      return Array.isArray(value)
    case "string":
      return typeof value === "string"
    case "integer":
      return typeof value === "number" && Number.isInteger(value)
    case "number":
      return typeof value === "number" && Number.isFinite(value)
    case "boolean":
      return typeof value === "boolean"
    case "null":
      return value === null
    default:
      return false
  }
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((item, i) => deepEqual(item, b[i]))
  }
  if (isRecord(a) && isRecord(b)) {
    const keys = Object.keys(a)
    return keys.length === Object.keys(b).length && keys.every((k) => k in b && deepEqual(a[k], b[k]))
  }
  return false
}

export function isRfc3339(text: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(\.\d+)?([Zz]|[+-](\d{2}):(\d{2}))$/.exec(text)
  if (match === null) return false
  const [year, month, day, hour, minute, second] = match.slice(1, 7).map(Number) as [
    number,
    number,
    number,
    number,
    number,
    number,
  ]
  const leap = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0
  const monthDays = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
  const maxDay = monthDays[month - 1]
  if (maxDay === undefined || day < 1 || day > maxDay) return false
  if (hour > 23 || minute > 59 || second > 59) return false
  if (match[9] !== undefined && (Number(match[9]) > 23 || Number(match[10]) > 59)) return false
  return true
}

function compile(pattern: string): RegExp {
  let regex = patternCache.get(pattern)
  if (regex === undefined) {
    regex = new RegExp(pattern, "u")
    patternCache.set(pattern, regex)
  }
  return regex
}

function walk(value: unknown, node: Node, path: string, depth: number, out: Violation[]): void {
  if (depth > MAX_DEPTH) {
    out.push({ path, reason: "schema recursion limit" })
    return
  }
  const ref = node["$ref"]
  if (typeof ref === "string") {
    const target = ref.startsWith("#/$defs/") ? DEFS[ref.slice(8)] : undefined
    if (target === undefined) out.push({ path, reason: "unresolved reference" })
    else walk(value, target, path, depth + 1, out)
  }
  const anyOf = node["anyOf"]
  if (Array.isArray(anyOf)) {
    const matched = anyOf.some((branch: Node) => {
      const scratch: Violation[] = []
      walk(value, branch, path, depth + 1, scratch)
      return scratch.length === 0
    })
    if (!matched) out.push({ path, reason: "no anyOf branch matched" })
  }
  const expected = node["type"]
  if (expected !== undefined) {
    const ok = Array.isArray(expected) ? expected.some((t) => typeMatches(value, t)) : typeMatches(value, expected)
    if (!ok) {
      out.push({ path, reason: "type mismatch" })
      return
    }
  }
  if ("const" in node && !deepEqual(value, node["const"])) out.push({ path, reason: "const mismatch" })
  const members = node["enum"]
  if (Array.isArray(members) && !members.some((m) => deepEqual(m, value))) out.push({ path, reason: "enum mismatch" })

  if (isRecord(value)) {
    const props = (node["properties"] ?? {}) as Record<string, Node>
    const required = node["required"]
    if (Array.isArray(required)) {
      for (const key of required as string[]) {
        if (!Object.hasOwn(value, key)) out.push({ path: `${path}.${key}`, reason: "required" })
      }
    }
    const closed = node["additionalProperties"] === false
    for (const [key, item] of Object.entries(value)) {
      const child = Object.hasOwn(props, key) ? props[key] : undefined
      if (child !== undefined) walk(item, child, `${path}.${key}`, depth + 1, out)
      else if (closed) out.push({ path: `${path}.${key}`, reason: "unexpected property" })
    }
  } else if (Array.isArray(value)) {
    const min = node["minItems"]
    const max = node["maxItems"]
    if (typeof min === "number" && value.length < min) out.push({ path, reason: "too few items" })
    if (typeof max === "number" && value.length > max) out.push({ path, reason: "too many items" })
    const items = node["items"]
    if (isRecord(items)) value.forEach((item, i) => walk(item, items, `${path}[${i}]`, depth + 1, out))
  } else if (typeof value === "string") {
    const length = [...value].length
    const min = node["minLength"]
    const max = node["maxLength"]
    if (typeof min === "number" && length < min) out.push({ path, reason: "too short" })
    if (typeof max === "number" && length > max) out.push({ path, reason: "too long" })
    const pattern = node["pattern"]
    if (typeof pattern === "string" && !compile(pattern).test(value)) out.push({ path, reason: "pattern mismatch" })
    if (node["format"] === "date-time" && !isRfc3339(value)) {
      out.push({ path, reason: "invalid timezone-qualified date-time" })
    }
  } else if (typeof value === "number") {
    const min = node["minimum"]
    const max = node["maximum"]
    if (typeof min === "number" && value < min) out.push({ path, reason: "below minimum" })
    if (typeof max === "number" && value > max) out.push({ path, reason: "above maximum" })
  }
}

/** Validates `value` against `$defs/<definition>`; an empty array means valid. */
export function validateDefinition(definition: string, value: unknown): Violation[] {
  const node = Object.hasOwn(DEFS, definition) ? DEFS[definition] : undefined
  if (node === undefined) return [{ path: "$", reason: "unknown definition" }]
  const out: Violation[] = []
  walk(value, node, "$", 0, out)
  return out
}
