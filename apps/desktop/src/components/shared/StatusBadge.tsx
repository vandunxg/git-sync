import { Icon, type IconKey } from "@/lib/icons"
import { cn } from "@/lib/utils"

export type Tone = "success" | "warning" | "danger" | "info" | "neutral"

const TONE_CLASS: Record<Tone, string> = {
  success: "bg-success-soft text-success",
  warning: "bg-warning-soft text-warning",
  danger: "bg-danger-soft text-danger",
  info: "bg-primary-soft text-primary",
  neutral: "bg-muted text-muted-foreground",
}

export interface StatusDescriptor {
  label: string
  icon: IconKey
  tone: Tone
  spinning?: boolean
}

/** Semantic icon + text; color is secondary (UI_CONTRACT icon contract). */
export function StatusBadge({ status, className }: { status: StatusDescriptor; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex h-6 max-w-full shrink-0 items-center gap-1.5 rounded-md px-2 text-xs font-medium whitespace-nowrap",
        TONE_CLASS[status.tone],
        className,
      )}
    >
      <Icon name={status.icon} size={14} spinning={status.spinning} />
      <span className="truncate">{status.label}</span>
    </span>
  )
}

/** Tinted square icon tile used once per row/section, not per field. */
export function IconTile({ icon, tone = "neutral", size = "md", spinning }: { icon: IconKey; tone?: Tone; size?: "sm" | "md"; spinning?: boolean }) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-lg",
        size === "md" ? "size-8" : "size-7",
        TONE_CLASS[tone],
      )}
    >
      <Icon name={icon} size={16} spinning={spinning} />
    </span>
  )
}
