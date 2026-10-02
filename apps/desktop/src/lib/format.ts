// Display formatting only. Unknown inputs render as an explicit "Unknown"/"—" label,
// never as a fabricated value.

const RTF = new Intl.RelativeTimeFormat("en", { numeric: "auto", style: "short" })
const DATE_TIME = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" })
const TIME = new Intl.DateTimeFormat("en", { hour: "2-digit", minute: "2-digit", hourCycle: "h23" })
const DAY = new Intl.DateTimeFormat("en", { weekday: "long", month: "short", day: "numeric" })

function parse(iso: string | null | undefined): Date | null {
  if (!iso) return null
  const date = new Date(iso)
  return Number.isNaN(date.getTime()) ? null : date
}

/** "2 min ago", "3 hr ago", "yesterday"; null when the time is unknown. */
export function relativeTime(iso: string | null | undefined, now: number = Date.now()): string | null {
  const date = parse(iso)
  if (date === null) return null
  const seconds = Math.round((date.getTime() - now) / 1000)
  const abs = Math.abs(seconds)
  if (abs < 45) return "just now"
  if (abs < 3600) return RTF.format(Math.round(seconds / 60), "minute")
  if (abs < 86_400) return RTF.format(Math.round(seconds / 3600), "hour")
  if (abs < 30 * 86_400) return RTF.format(Math.round(seconds / 86_400), "day")
  return DATE_TIME.format(date)
}

export function dateTime(iso: string | null | undefined): string | null {
  const date = parse(iso)
  return date === null ? null : DATE_TIME.format(date)
}

export function clockTime(iso: string | null | undefined): string | null {
  const date = parse(iso)
  return date === null ? null : TIME.format(date)
}

/** "Today", "Yesterday" or a weekday heading for grouped timelines. */
export function dayHeading(iso: string, now: number = Date.now()): string {
  const date = parse(iso)
  if (date === null) return "Unknown date"
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const diffDays = Math.round((startOf(new Date(now)) - startOf(date)) / 86_400_000)
  if (diffDays === 0) return "Today"
  if (diffDays === 1) return "Yesterday"
  return DAY.format(date)
}

export function dayKey(iso: string): string {
  const date = parse(iso)
  return date === null ? "unknown" : `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`
}

export function formatBytes(bytes: number | null | undefined): string | null {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return null
  const units = ["B", "KB", "MB", "GB", "TB"]
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value >= 10 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

export function formatDuration(seconds: number | null | undefined): string | null {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds) || seconds < 0) return null
  if (seconds < 60) return `${Math.round(seconds)}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${Math.round(seconds % 60)}s`
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`
}

export function shortOid(oid: string | null | undefined): string {
  return oid ? oid.slice(0, 8) : "—"
}

export function plural(count: number, singular: string, pluralForm = `${singular}s`): string {
  return `${count.toLocaleString("en")} ${count === 1 ? singular : pluralForm}`
}

/** "owner/name" -> { owner, name } for display; the full name stays the identity. */
export function splitFullName(fullName: string): { owner: string; name: string } {
  const index = fullName.lastIndexOf("/")
  return index < 0 ? { owner: "", name: fullName } : { owner: fullName.slice(0, index), name: fullName.slice(index + 1) }
}

/** Visible text for control characters so ANSI/terminal sequences are never interpreted. */
export function visibleLogText(line: string): string {
  let out = ""
  for (const char of line) {
    const code = char.charCodeAt(0)
    if (code === 0x09) out += char
    else if (code < 0x20) out += String.fromCharCode(0x2400 + code)
    else if (code === 0x7f) out += "␡"
    else out += char
  }
  return out
}
