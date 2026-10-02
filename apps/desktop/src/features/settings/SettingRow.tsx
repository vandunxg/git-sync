import type { ReactNode } from "react"

import { cn } from "@/lib/utils"

export function rowIds(name: string) {
  return { control: `setting-${name}`, label: `setting-${name}-label`, description: `setting-${name}-description`, error: `setting-${name}-error` }
}

/** One setting: label + description on the left, the typed control on the right. No per-row icon tile. */
export function SettingRow({
  name,
  label,
  description,
  control,
  note,
  error,
  labelFor = true,
}: {
  name: string
  label: ReactNode
  description?: ReactNode
  control: ReactNode
  /** Extra context under the description, e.g. a disabled reason. */
  note?: ReactNode
  error?: string
  /** False for read-only values that have no focusable control. */
  labelFor?: boolean
}) {
  const ids = rowIds(name)
  const LabelTag = labelFor ? "label" : "p"
  return (
    <div className="flex min-h-16 flex-wrap items-center justify-between gap-x-8 gap-y-3 py-4">
      <div className="min-w-0 flex-[1_1_16rem]">
        <LabelTag id={ids.label} {...(labelFor ? { htmlFor: ids.control } : {})} className="text-sm font-medium">
          {label}
        </LabelTag>
        {description || note ? (
          <div id={ids.description} className="mt-0.5 flex flex-col gap-1 text-[13px] leading-snug text-muted-foreground">
            {description ? <p>{description}</p> : null}
            {note}
          </div>
        ) : null}
        {error ? (
          <p id={ids.error} role="alert" className="mt-1.5 text-[13px] font-medium text-danger">
            {error}
          </p>
        ) : null}
      </div>
      <div className={cn("flex shrink-0 items-center justify-end")}>{control}</div>
    </div>
  )
}

/** aria-describedby for a control: description first, then the field error when present. */
export function describedBy(name: string, hasDescription: boolean, hasError: boolean): string | undefined {
  const ids = rowIds(name)
  const parts = [hasDescription ? ids.description : null, hasError ? ids.error : null].filter((part) => part !== null)
  return parts.length > 0 ? parts.join(" ") : undefined
}

export function RowGroupHeading({ title, description }: { title: string; description?: string }) {
  return (
    <div className="pt-6 pb-1">
      <h3 className="text-[15px] font-semibold">{title}</h3>
      {description ? <p className="mt-0.5 text-[13px] text-muted-foreground">{description}</p> : null}
    </div>
  )
}

export function ReadOnlyValue({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex h-9 items-center rounded-lg border border-dashed border-border bg-muted/40 px-3 text-[13px] text-muted-foreground">
      {children}
    </span>
  )
}
